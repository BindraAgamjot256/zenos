use crate::process::{PROCESSES, ProcessStatus, current_pid};
use crate::syscall::table::SyscallPtr;
use log::info;
use zenos_macros::syscall;

#[syscall(61)]
fn wait(rdi: u64, _rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    let target_pid = rdi;
    let caller_pid = current_pid();
    info!(
        "wait syscall: pid {} waiting for pid {}",
        caller_pid, target_pid
    );

    {
        let mut procs = PROCESSES.lock();

        // Find target process and check if already exited
        let target_idx = procs.iter().position(|p| p.pid == target_pid);

        if let Some(idx) = target_idx {
            if let Some(exit_code) = procs[idx].exit_code {
                // Target already exited - reap the zombie and return
                info!(
                    "wait: pid {} already finished with exit code {}, reaping",
                    target_pid, exit_code
                );
                procs.remove(idx);
                return exit_code;
            }

            // Target exists but hasn't exited - mark ourselves as waiting
            if let Some(caller) = procs.iter_mut().find(|p| p.pid == caller_pid) {
                caller.status = ProcessStatus::WaitingFor(target_pid);
                info!(
                    "wait: pid {} now waiting for pid {}",
                    caller_pid, target_pid
                );
            }
        } else {
            // Target doesn't exist
            info!("wait: target pid {} doesn't exist", target_pid);
            return u64::MAX; // -1
        }
    }

    // Yield to scheduler - it will wake us when target exits
    crate::process::schedule_next();
}
