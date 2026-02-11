use crate::disk::FS;
use crate::disk::vfs::{Inode, Permissions};
use crate::process;
use crate::process::PROCESSES;
use crate::syscall::copy_from_user;
use crate::syscall::errors::{
    E2BIG, EFAULT, EINVAL, ENAMETOOLONG, ENOEXEC, ESRCH, file_error_to_errno,
};
use crate::syscall::table::SyscallPtr;
use alloc::string::ToString;
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::mem::size_of;
use core::sync::atomic::Ordering;
use log::info;
use spin::Mutex;
use zenos_macros::syscall;

const MAX_PATH_LEN: usize = 4096;
const MAX_ARGC: usize = 256;
const MAX_ARG_LEN: usize = 4096;
const MAX_ARG_BYTES: usize = 128 * 1024;

#[syscall(0x3b)]
fn exec(rdi: u64, rsi: u64, rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    info!("exec rdi {:#x} rsi {:#x}, rds {:#x}", rdi, rsi, rdx);

    let path_ptr = rdi as *const u8;
    let argv_ptr = rsi as *const *const u8;
    let envp_ptr = rdx as *const *const u8;

    // ---- copy path ----
    let path_buf = match copy_from_user(path_ptr, MAX_PATH_LEN) {
        Ok(b) => b,
        Err(_) => return (-EFAULT) as u64,
    };

    let path_len = match path_buf.iter().position(|&b| b == 0) {
        Some(p) => p,
        None => return (-ENAMETOOLONG) as u64,
    };

    let path = &path_buf[..path_len];
    let path = match core::str::from_utf8(path) {
        Ok(s) => s.to_string(),
        Err(_) => return (-EINVAL) as u64,
    };

    drop(path_buf);

    // ---- copy argv ----
    let kargv = match copy_string_array(argv_ptr) {
        Ok(v) => v,
        Err(e) => return e,
    };

    // ---- copy envp ----
    let kenvp = match copy_string_array(envp_ptr) {
        Ok(v) => v,
        Err(e) => return e,
    };

    let fs = FS.lock();
    let file = match fs.open_file(&path) {
        Ok(f) => f,
        Err(e) => return file_error_to_errno(&e),
    };
    drop(fs);

    exec_inner(file, &path, &kargv, &kenvp)
}

fn copy_string_array(ptr: *const *const u8) -> Result<Vec<Vec<u8>>, u64> {
    info!("copy_string_array ptr: {:p}", ptr);

    let mut result = Vec::new();
    let mut total_bytes = 0usize;

    for i in 0..MAX_ARGC {
        // ---- read pointer i safely ----
        let ptr_addr = unsafe { ptr.add(i) } as *const u8;

        let ptr_bytes =
            copy_from_user(ptr_addr, size_of::<*const u8>()).map_err(|_| (-EFAULT) as u64)?;

        let str_ptr = unsafe { *(ptr_bytes.as_ptr() as *const *const u8) };

        if str_ptr.is_null() {
            break;
        }

        // ---- read string byte-by-byte ----
        let mut buf = Vec::new();

        for j in 0..MAX_ARG_LEN {
            let byte =
                copy_from_user(unsafe { str_ptr.add(j) }, 1).map_err(|_| (-EFAULT) as u64)?[0];

            buf.push(byte);
            total_bytes += 1;

            if total_bytes > MAX_ARG_BYTES {
                return Err((-E2BIG) as u64);
            }

            if byte == 0 {
                break;
            }
        }

        // no NUL before MAX_ARG_LEN
        if *buf.last().unwrap() != 0 {
            return Err((-E2BIG) as u64);
        }

        result.push(buf);
    }

    Ok(result)
}

fn exec_inner(file: Arc<Mutex<Inode>>, path: &str, argv: &[Vec<u8>], envp: &[Vec<u8>]) -> u64 {
    let mut file = file.lock();
    let file_len = file.size.load(Ordering::SeqCst);

    let mut file_buf = alloc::vec![0u8; file_len as usize];
    let fread_res = file.data.read(0, &mut file_buf);
    if let Err(e) = fread_res {
        return file_error_to_errno(&e);
    }

    let len = fread_res.unwrap();
    if len != file_len as usize {
        panic!("exec read short: {} != {}", len, file_len);
    }

    if !file.perms.contains(Permissions::OWNER_EXEC) {
        return (-ENOEXEC) as u64;
    }

    let parent_pid = process::current_pid();
    let mut procs = PROCESSES.lock();

    let proc = match procs.iter_mut().find(|p| p.pid == parent_pid) {
        Some(p) => p,
        None => return (-ESRCH) as u64,
    };

    let argv_ptrs: Vec<*const u8> = argv
        .iter()
        .map(|a| a.as_ptr())
        .chain(core::iter::once(core::ptr::null()))
        .collect();

    let envp_ptrs: Vec<*const u8> = envp
        .iter()
        .map(|e| e.as_ptr())
        .chain(core::iter::once(core::ptr::null()))
        .collect();

    let mut full_argv = Vec::new();
    let mut pth = path.to_string();
    pth.push(0 as char);
    full_argv.push(pth.as_ptr());
    full_argv.extend(argv_ptrs);

    let argc = full_argv.len();

    info!(
        "exec: path={}, argc={}, argv={:?}, envp={:?}",
        path, argc, full_argv, envp_ptrs
    );

    proc.exec_replace(path, argc, full_argv.as_ptr(), envp_ptrs.as_ptr());

    proc.load(&file_buf);

    let (entry, stack, pid) = match proc.prepare_run() {
        Some(v) => (v.0, v.1, proc.pid),
        None => return (-ENOEXEC) as u64,
    };

    {
        let mut sched = process::SCHEDULER.lock();
        sched.set_current(pid);
    }

    drop(procs);
    drop(file);

    process::enter_user_mode(entry, stack);
}
