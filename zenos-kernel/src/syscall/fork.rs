use crate::interrupts::gdt::GDT;
use crate::process::{PROCESSES, Process, current_pid};
use crate::syscall::errors::{EAGAIN, ENOMEM};
use crate::syscall::table::SyscallPtr;
use log::{debug, error, info};
use zenos_macros::syscall;

#[syscall(57)]
fn fork(_rdi: u64, _rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    // fork() - create a child process
    // Returns: 0 to child, child PID to parent, -1 on error

    let parent_pid = current_pid();
    debug!("fork() called by pid {}", parent_pid);

    // Get parent's state while holding the lock briefly
    let child_state = {
        let procs = PROCESSES.lock();
        let parent = match procs.iter().find(|p| p.pid == parent_pid) {
            Some(p) => p,
            None => {
                error!("fork: parent process {} not found", parent_pid);
                return (-EAGAIN) as u64;
            }
        };

        // Copy parent's current state for the child (already saved by syscall_main)
        // Child gets identical state except RAX=0 (fork return value)
        let mut child_state = parent.state;
        child_state.rax = 0;

        // Ensure child has valid user CS/SS segments
        let user_cs = (GDT.user_code_segment.0 | 3) as u64;
        let user_ss = (GDT.user_data_segment.0 | 3) as u64;
        if child_state.cs == 0 {
            child_state.cs = user_cs;
        }
        if child_state.ss == 0 {
            child_state.ss = user_ss;
        }

        // Ensure interrupts are enabled in child (IF flag = 0x200)
        child_state.rflags |= 0x200;

        debug!(
            "fork: child_state rip={:#x}, rsp={:#x}, cs={:#x}, ss={:#x}",
            child_state.rip, child_state.rsp, child_state.cs, child_state.ss
        );

        child_state
    };
    // Lock is now released - safe to do heavy operations

    // Create child process (this does memory allocation for address space clone)
    let child_process = {
        let procs = PROCESSES.lock();
        let parent = match procs.iter().find(|p| p.pid == parent_pid) {
            Some(p) => p,
            None => {
                error!(
                    "fork: parent process {} not found after re-lock",
                    parent_pid
                );
                return (-EAGAIN) as u64;
            }
        };

        match Process::fork_from(parent, child_state) {
            Ok(child) => child,
            Err(_e) => {
                error!("fork: failed to create child process");
                return (-ENOMEM) as u64;
            }
        }
    };

    let child_pid = child_process.pid;

    // Add child to process list
    {
        let mut procs = PROCESSES.lock();
        procs.push(child_process);
    }

    info!(
        "fork: created child pid {} from parent pid {}",
        child_pid, parent_pid
    );

    // Parent returns child's PID
    child_pid
}
