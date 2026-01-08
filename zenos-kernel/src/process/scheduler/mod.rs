use crate::process::{PROCESSES, Process, ProcessState, ProcessStatus, set_current_pid};
use log::{debug, info, trace};
use pc_keyboard::KeyCode::N;

/// Round-robin scheduler for preemptive multitasking
pub struct Scheduler {
    cursor: usize,
    /// PID of the currently running process (if any)
    current_pid: Option<u64>,
    #[cfg(debug_assertions)]
    times_scheduled: u64,
}

impl Scheduler {
    pub const fn new() -> Self {
        Scheduler {
            cursor: 0,
            current_pid: None,
            times_scheduled: 0,
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
    /// This updates both the scheduler's internal state and the per-CPU data
    pub fn set_current(&mut self, pid: u64) {
        self.current_pid = Some(pid);
        set_current_pid(pid);
    }

    /// Schedule: save current process state and switch to next
    /// Called from timer interrupt handler
    /// Returns (next_pid, next_cr3, next_context) if there's a process to switch to
    pub fn schedule(&mut self, current_state: &ProcessState) -> Option<(u64, u64, ProcessState)> {
        // Use try_lock to avoid deadlock with syscalls holding the lock
        let mut procs = PROCESSES.try_lock()?;

        // Save state of current process if there is one
        if let Some(current_pid) = self.current_pid {
            if let Some(current_proc) = procs.iter_mut().find(|p| p.pid == current_pid) {
                if current_proc.status == ProcessStatus::Running {
                    current_proc.save_context(current_state);
                    current_proc.status = ProcessStatus::Ready;
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

        if self.cursor > len {
            self.cursor = 0;
        }

        let proc = procs.get_mut(self.cursor);
        if let Some(next_proc) = proc {
            if next_proc.status == ProcessStatus::Ready {
                // Found next process to run
                let next_pid = next_proc.pid;
                let next_cr3 = next_proc.cr3.as_u64();
                let next_state = next_proc.state;
                let curr = self.current_pid.unwrap_or(0);
                self.current_pid = Some(next_pid);

                // Move cursor to next for future calls
                self.cursor = (self.cursor + 1) % len;
                next_proc.status = ProcessStatus::Running;
                self.set_current(next_pid);
                if next_pid == curr {
                    info!(
                        "Scheduler chose the same process (pid {}) to run again",
                        next_pid
                    );
                    return None; // early return if switching to the same process
                }

                #[cfg(debug_assertions)]
                {
                    self.times_scheduled += 1;
                    debug!(
                        "Scheduling switch to pid {} (times scheduled: {})",
                        next_pid, self.times_scheduled
                    );
                }
                return Some((next_pid, next_cr3, next_state));
            }
            // Move cursor to next for future calls
            self.cursor = (self.cursor + 1) % len;
        }

        // No other process ready, continue with current if it exists
        None
    }
}
