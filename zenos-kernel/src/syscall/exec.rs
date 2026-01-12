use crate::disk::{FS, get_len};
use crate::process;
use crate::process::PROCESSES;
use crate::syscall::copy_from_user;
use crate::syscall::errors::{EINVAL, file_error_to_errno};
use crate::syscall::table::SyscallPtr;
use core::ffi::CStr;
use core::mem::size_of;
use log::info;
use zenos_macros::syscall;

const MAX_PATH_LEN: usize = 4096;
const MAX_ARGC: usize = 256;
const MAX_ARG_LEN: usize = 4096;
const MAX_ARG_BYTES: usize = 128 * 1024;

#[syscall(0x3b)]
fn exec(rdi: u64, rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    info!("exec rdi {:#x} rsi {:#x}", rdi, rsi);

    let path_ptr = rdi as *const u8;
    let argv_ptr = rsi as *const *const u8;

    // ---- copy path ----
    let path_buf = match copy_from_user(path_ptr, MAX_PATH_LEN) {
        Ok(b) => b,
        Err(_) => return -EINVAL as u64,
    };

    let path_len = match path_buf.iter().position(|&b| b == 0) {
        Some(p) => p,
        None => return -EINVAL as u64,
    };

    let path = &path_buf[..path_len];
    let path = match core::str::from_utf8(path) {
        Ok(s) => s,
        Err(_) => return -EINVAL as u64,
    };

    // ---- copy argv pointer array ----
    let mut argv_ptrs: alloc::vec::Vec<*const u8> = alloc::vec::Vec::new();

    for i in 0..MAX_ARGC {
        let ptr_addr = unsafe { argv_ptr.add(i) };

        let ptr_bytes = match copy_from_user(ptr_addr as *const u8, size_of::<*const u8>()) {
            Ok(b) => b,
            Err(_) => return -EINVAL as u64,
        };

        let arg_ptr = unsafe { *(ptr_bytes.as_ptr() as *const *const u8) };

        if arg_ptr.is_null() {
            break;
        }

        argv_ptrs.push(arg_ptr);
    }

    // ---- copy argv strings ----
    let mut kargv: alloc::vec::Vec<alloc::vec::Vec<u8>> = alloc::vec::Vec::new();
    let mut total_bytes = 0usize;

    for arg_ptr in argv_ptrs {
        if arg_ptr.is_null() {
            break;
        }

        let arg_buf = match copy_from_user(arg_ptr, MAX_ARG_LEN) {
            Ok(b) => b,
            Err(_) => return -EINVAL as u64,
        };

        let arg_len = match arg_buf.iter().position(|&b| b == 0) {
            Some(p) => p,
            None => return -EINVAL as u64,
        };

        total_bytes += arg_len + 1;
        if total_bytes > MAX_ARG_BYTES {
            return -EINVAL as u64;
        }

        kargv.push(arg_buf[..arg_len].to_vec());
    }

    exec_inner(path, &kargv)
}

fn exec_inner(path: &str, argv: &[alloc::vec::Vec<u8>]) -> u64 {
    let fs = FS.lock();

    let mut file = match fs.open_file(path) {
        Ok(f) => f,
        Err(e) => return file_error_to_errno(&e),
    };

    let file_len = match get_len(file.as_mut()) {
        Ok(l) => l,
        Err(_) => return -EINVAL as u64,
    };

    let mut file_buf = alloc::vec![0u8; file_len as usize];
    if let Err(e) = file.read(&mut file_buf) {
        return file_error_to_errno(&e);
    }

    let parent_pid = process::current_pid();
    let mut procs = PROCESSES.lock();

    let proc = match procs.iter_mut().find(|p| p.pid == parent_pid) {
        Some(p) => p,
        None => return -EINVAL as u64,
    };

    let argc = argv.len();
    let argv: alloc::vec::Vec<*const u8> = argv
        .iter()
        .map(|arg| arg.as_ptr())
        .chain(core::iter::once(core::ptr::null()))
        .collect();

    proc.exec_replace(path, argc, argv.as_ptr(), core::ptr::null());
    proc.load(&file_buf);

    let (entry, stack, pid) = match proc.prepare_run() {
        Some(v) => (v.0, v.1, proc.pid),
        None => return -EINVAL as u64,
    };

    {
        let mut sched = process::SCHEDULER.lock();
        sched.set_current(pid);
    }

    drop(fs);
    drop(procs);

    process::enter_user_mode(entry, stack);
}
