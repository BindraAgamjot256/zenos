use crate::process::{PROCESSES, Process, ProcessState, ProcessStatus};
use log::{debug, trace};

/// Round-robin scheduler for preemptive multitasking
pub struct Scheduler {
    cursor: usize,
    /// PID of the currently running process (if any)
    current_pid: Option<u64>,
}

impl Scheduler {
    pub const fn new() -> Self {
        Scheduler {
            cursor: 0,
            current_pid: None,
        }
    }

    /// Add a new process to the scheduler
    pub fn add_proc(&mut self, proc: Process) {
        PROCESSES.lock().push(proc);
    }

    /// Get the PID of the currently running process
    pub fn current_pid(&self) -> Option<u64> {
        self.current_pid
    }

    /// Set the currently running process
    pub fn set_current(&mut self, pid: u64) {
        self.current_pid = Some(pid);
    }

    /// Get next ready process index to run (round-robin)
    /// Returns the index in the PROCESSES array
    pub fn next_proc(&mut self) -> Option<usize> {
        let procs = PROCESSES.lock();
        if procs.is_empty() {
            return None;
        }

        let len = procs.len();
        // Search for a ready process starting from cursor
        for i in 0..len {
            let idx = (self.cursor + i) % len;
            if procs[idx].status == ProcessStatus::Ready
                || procs[idx].status == ProcessStatus::Running
            {
                self.cursor = (idx + 1) % len;
                return Some(idx);
            }
        }

        None
    }

    /// Schedule: save current process state and switch to next
    /// Called from timer interrupt handler
    /// Returns (next_pid, next_cr3, next_context) if there's a process to switch to
    pub fn schedule(&mut self, current_state: &ProcessState) -> Option<(u64, u64, ProcessState)> {
        // Use try_lock to avoid deadlock with syscalls holding the lock
        let mut procs = PROCESSES.try_lock()?;

        // Debug: log all process states
        for p in procs.iter() {
            debug!("schedule: pid {} status {:?}", p.pid, p.status);
        }

        // Save state of current process if there is one
        if let Some(current_pid) = self.current_pid {
            if let Some(current_proc) = procs.iter_mut().find(|p| p.pid == current_pid) {
                if current_proc.status == ProcessStatus::Running {
                    current_proc.save_context(current_state);
                    trace!("Saved context for pid {}", current_pid);
                }
            }
        } else {
            debug!("schedule: no current_pid set in scheduler!");
        }

        // Find next ready process (skip current process for fairness)
        let len = procs.len();
        if len == 0 {
            return None;
        }

        // Start searching from cursor, but skip the current process
        for i in 0..len {
            let idx = (self.cursor + i) % len;
            let proc = &procs[idx];

            // Skip the current process - we want to give other processes a chance
            if self.current_pid == Some(proc.pid) {
                continue;
            }

            if proc.status == ProcessStatus::Ready {
                self.cursor = (idx + 1) % len;
                let pid = proc.pid;
                let cr3 = proc.get_cr3().as_u64();
                let ctx = *proc.get_context();

                debug!(
                    "schedule: switching to pid {} (rip={:#x}, rsp={:#x}, rax={:#x}, cs={:#x}, ss={:#x})",
                    pid, ctx.rip, ctx.rsp, ctx.rax, ctx.cs, ctx.ss
                );

                // Mark the process as running (we already hold the lock)
                drop(procs);
                if let Some(mut procs) = PROCESSES.try_lock() {
                    if let Some(p) = procs.iter_mut().find(|p| p.pid == pid) {
                        p.status = ProcessStatus::Running;
                    }
                }

                self.current_pid = Some(pid);
                trace!("Switching to pid {}", pid);
                return Some((pid, cr3, ctx));
            }
        }

        // No other process ready, continue with current if it exists
        None
    }
}
