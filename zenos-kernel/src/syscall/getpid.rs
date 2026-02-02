use crate::process::{PROCESSES, SCHEDULER};
use crate::syscall::table::SyscallPtr;
use zenos_macros::syscall;

#[syscall(39)]
fn getpid(_rdi: u64, _rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    let scheduler = SCHEDULER.lock();

    match scheduler.current_pid() {
        Some(pid) => pid as u64,
        None => {
            // Kernel context or scheduler not fully initialized.
            // Unix convention: PID 0 is the idle/swapper task.
            0
        }
    }
}

#[syscall(110)]
fn getppid(_rdi: u64, _rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    let scheduler = SCHEDULER.lock();

    match scheduler.current_pid() {
        Some(pid) => {
            drop(scheduler);
            let proc = PROCESSES.lock();
            match proc.iter().find(|p| p.pid == pid) {
                Some(proc) => proc.parent_pid,
                None => 0,
            }
        }
        None => 0, // Kernel context or scheduler not fully initialized.
    }
}
