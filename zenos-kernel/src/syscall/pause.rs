use crate::syscall::errors::ENOSYS;
use crate::syscall::table::SyscallPtr;
use log::debug;
use zenos_macros::syscall;

#[syscall(34)]
fn pause(_rdi: u64, _rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    // pause()
    debug!("syscall pause");

    let pid = crate::process::current_pid();
    debug!("pause called by pid {}", pid);
    let procs = crate::process::PROCESSES.lock();
    match procs.iter().clone().find(|p| p.pid == pid) {
        Some(p) => debug!("pause called with process {:?}", p),
        None => {
            debug!("pause: process {} not found", pid);
        }
    }
    -ENOSYS as u64
}
