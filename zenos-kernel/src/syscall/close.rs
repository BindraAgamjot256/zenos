use crate::syscall::errors::{EBADF, ESRCH};
use crate::syscall::table::SyscallPtr;
use zenos_macros::syscall;

#[syscall(3)]
fn close(rdi: u64, _rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    let fd = rdi;
    let process = match unsafe { crate::process::current_proc_mut() } {
        Some(p) => p,
        None => return (-ESRCH) as u64,
    };
    match process.close_file_handle(fd) {
        Ok(_) => 0,
        Err(_) => (-EBADF) as u64,
    }
}
