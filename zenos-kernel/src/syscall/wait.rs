use crate::process::isolation::teardown_address_space;
use crate::process::{PROCESSES, ProcessStatus, current_pid};
use crate::syscall::errors::{ECHILD, ESRCH};
use crate::syscall::table::SyscallPtr;
use log::info;
use zenos_macros::syscall;

#[syscall(61)]
fn waitpid(rdi: u64, _rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    let mut target_pid = rdi as i64; // -1 means wait for any child
    let caller_pid = current_pid();

    info!(
        "wait syscall: pid {} waiting for pid {}",
        caller_pid, target_pid
    );

    loop {
        let mut procs = PROCESSES.lock();

        // If wait(-1), try to find any exited child
        if target_pid == -1 {
            if let Some(idx) = procs
                .iter()
                .position(|p| p.parent_pid == caller_pid && p.exit_code.is_some())
            {
                let exit_code = procs[idx].exit_code.unwrap();
                let pid_reaped = procs[idx].pid;
                let cr3 = procs[idx].cr3;
                procs.swap_remove(idx);
                info!(
                    "wait: pid {} reaped child pid {} with exit code {}",
                    caller_pid, pid_reaped, exit_code
                );
                // Teardown the reaped process's address space
                unsafe { teardown_address_space(cr3) };
                return exit_code;
            }

            // No exited child, pick any child to wait for
            if let Some(child) = procs.iter().find(|p| p.parent_pid == caller_pid) {
                target_pid = child.pid as i64;
            } else {
                // No children at all
                info!("wait: pid {} has no children", caller_pid);
                return (-ECHILD) as u64;
            }
        }

        // Try to reap the specific target
        if let Some(idx) = procs
            .iter()
            .position(|p| p.pid == target_pid as u64 && p.exit_code.is_some())
        {
            let exit_code = procs[idx].exit_code.unwrap();
            let cr3 = procs[idx].cr3;
            procs.swap_remove(idx);
            info!(
                "wait: pid {} reaped child pid {} with exit code {}",
                caller_pid, target_pid, exit_code
            );
            // Teardown the reaped process's address space
            unsafe { teardown_address_space(cr3) };
            return exit_code;
        }

        // If target exists but hasn't exited, mark as waiting
        if procs.iter().any(|p| p.pid == target_pid as u64) {
            if let Some(caller) = unsafe { crate::process::current_proc_mut() } {
                caller.status = ProcessStatus::WaitingFor(target_pid as u64);
                info!(
                    "wait: pid {} now waiting for pid {}",
                    caller_pid, target_pid
                );
            }
        } else {
            // Target doesn't exist
            info!("wait: target pid {} doesn't exist", target_pid);
            return (-ESRCH) as u64;
        }

        drop(procs); // unlock before yielding
        crate::process::schedule_next();
    }
}
