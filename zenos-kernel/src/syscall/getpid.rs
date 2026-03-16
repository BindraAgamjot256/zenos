use crate::process::current_pid;
use crate::syscall::table::SyscallPtr;
use zenos_macros::syscall;

#[syscall(39)]
fn getpid(_rdi: u64, _rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    current_pid()
}

#[syscall(110)]
fn getppid(_rdi: u64, _rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    match unsafe { crate::process::current_proc_mut() } {
        Some(proc) => proc.parent_pid,
        None => 0,
    }
}

#[syscall(102)]
fn getuid(_rdi: u64, _rsi: u64, _rdx: u64, _r10: u64) -> u64 {
    match unsafe { crate::process::current_proc_mut() } {
        Some(proc) => proc.uid,
        None => 0,
    }
}

#[syscall(104)]
fn getgid(_rdi: u64, _rsi: u64, _rdx: u64, _r10: u64) -> u64 {
    match unsafe { crate::process::current_proc_mut() } {
        Some(proc) => proc.gid,
        None => 0,
    }
}
