use crate::disk::{FS, get_len};
use crate::process;
use crate::process::PROCESSES;
use crate::syscall::copy_from_user;
use crate::syscall::errors::{EINVAL, file_error_to_errno};
use crate::syscall::table::SyscallPtr;
use log::info;
use zenos_macros::syscall;

const MAX_PATH_LEN: usize = 4096;

#[syscall(0x3b)]
fn exec(rdi: u64, _rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    info!("exec rdi {:#x}", rdi);
    let path_buf = rdi as *const u8;
    let buf = copy_from_user(path_buf, MAX_PATH_LEN);
    if buf.is_err() {
        -EINVAL as u64
    } else {
        let cstr_bytes = buf.unwrap();
        let nul_pos = cstr_bytes
            .iter()
            .position(|&b| b == 0)
            .unwrap_or(cstr_bytes.len());
        let path = core::str::from_utf8(&cstr_bytes[..nul_pos]);
        if path.is_err() {
            -EINVAL as u64
        } else {
            let ret = exec_inner(path.unwrap());
            info!("exec returning {}", ret as isize);
            ret
        }
    }
}

fn exec_inner(path: &str) -> u64 {
    let fs = FS.lock();
    match fs.open_file(path) {
        Ok(mut file) => {
            let len = get_len(file.as_mut()).ok();
            if len.is_none() {
                return -EINVAL as u64;
            }
            let file_len = len.unwrap();
            let mut buf = alloc::vec![0u8; file_len as usize];
            let res = file.read(&mut buf);
            if res.is_err() {
                return file_error_to_errno(&res.err().unwrap());
            }
            let parent = process::current_pid();
            let mut binding = PROCESSES.lock();
            let parent_process = binding.iter_mut().find(|p| p.pid == parent);
            if parent_process.is_none() {
                return -EINVAL as u64;
            }
            let parent_process = parent_process.unwrap();
            parent_process.exec_replace(path);
            parent_process.load(&buf);
            let (entry, stack, pid) = {
                let pinit = parent_process;
                let (e, s) = pinit.prepare_run().unwrap();
                (e, s, pinit.pid)
            };

            // Tell the scheduler which process is currently running
            {
                let mut sched = process::SCHEDULER.lock();
                sched.set_current(pid);
            }
            drop(fs);
            drop(binding);
            process::enter_user_mode(entry, stack);
        }
        Err(e) => file_error_to_errno(&e), // File isn't found or other error
    }
}
