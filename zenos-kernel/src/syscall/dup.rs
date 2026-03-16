use crate::syscall::errors::{EBADF, ESRCH};
use crate::syscall::table::SyscallPtr;
use zenos_macros::syscall;

#[syscall(32)]
fn dup(rdi: u64, _rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    let old_fd = rdi;
    let curr_proc = match unsafe { crate::process::current_proc_mut() } {
        Some(p) => p,
        None => return (-ESRCH) as u64,
    };
    let fd = curr_proc.dup_file_handle(old_fd, None);
    if fd.is_err() {
        (-EBADF) as u64
    } else {
        fd.unwrap()
    }
}

#[syscall(33)]
fn dup2(rdi: u64, rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    let old_fd = rdi;
    let new_fd = rsi;
    let curr_proc = match unsafe { crate::process::current_proc_mut() } {
        Some(p) => p,
        None => return (-ESRCH) as u64,
    };
    if old_fd == new_fd {
        if curr_proc.get_file_handle(old_fd).is_none() {
            return (-EBADF) as u64;
        }
        return new_fd;
    }
    let fd = curr_proc.dup_file_handle(old_fd, Some(new_fd));
    if fd.is_err() {
        (-EBADF) as u64
    } else {
        fd.unwrap()
    }
}
