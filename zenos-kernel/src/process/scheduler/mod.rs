use crate::process::{PROCESSES, ProcessState, ProcessStatus, set_current_pid};
use log::{debug, info, trace};

/// Round-robin scheduler for preemptive multitasking
pub struct Scheduler {
    cursor: usize,
    /// PID of the currently running process (if any)
    current_pid: CurrentProcessAction,
    #[cfg(debug_assertions)]
    times_scheduled: u64,
    /// Time quantum in milliseconds
    quantum: u64,
    /// Time used by the current process in its time slice
    time: u64,
}

#[derive(Debug, Copy, Clone)]
enum CurrentProcessAction {
    SaveAndSwitch(u64),
    SwitchOnly(u64),
    None,
}
impl CurrentProcessAction {
    fn unwrap_or(&self, default: u64) -> u64 {
        match self {
            CurrentProcessAction::SaveAndSwitch(pid) => *pid,
            CurrentProcessAction::SwitchOnly(pid) => *pid,
            CurrentProcessAction::None => default,
        }
    }
}
impl Scheduler {
    pub const fn new() -> Self {
        Scheduler {
            cursor: 0,
            current_pid: CurrentProcessAction::None,
            times_scheduled: 0,
            quantum: 50, // milliseconds
            time: 0,
        }
    }
    /// Get the PID of the currently running process
    pub fn current_pid(&self) -> Option<u64> {
        let pid = self.current_pid;
        if let CurrentProcessAction::SaveAndSwitch(pid) = pid {
            return Some(pid);
        };
        None
    }

    /// Set the currently running process
    /// This updates both the scheduler's internal state and the per-CPU data
    pub fn set_current(&mut self, pid: u64) {
        self.current_pid = CurrentProcessAction::SaveAndSwitch(pid);
        set_current_pid(pid);
    }

    /// Force the scheduler to switch on the next timer tick
    pub fn force_reschedule(&mut self) {
        self.time = self.quantum;
        // switch to pid 1 (init) on next schedule
        self.current_pid = CurrentProcessAction::SwitchOnly(1);
    }

    /// Schedule: save current process state and switch to next
    /// Called from timer interrupt handler
    /// Returns (next_pid, next_cr3, next_context) if there's a process to switch to
    pub fn schedule(&mut self, current_state: &ProcessState) -> Option<(u64, u64, ProcessState)> {
        if self.quantum == self.time {
            // Use try_lock to avoid deadlock with syscalls holding the lock
            let mut procs = PROCESSES.try_lock()?;

            // Save state of current process if there is one
            if let CurrentProcessAction::SaveAndSwitch(current_pid) = self.current_pid {
                if let Some(current_proc) = procs.iter_mut().find(|p| p.pid == current_pid) {
                    if current_proc.status == ProcessStatus::Running {
                        current_proc.save_context(current_state);
                        current_proc.status = ProcessStatus::Ready;
                        trace!("Saved context for pid {}", current_pid);
                    }
                }
            } else if let CurrentProcessAction::None = self.current_pid {
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
                if let ProcessStatus::Blocked(flag) = next_proc.status
                    && !flag.load(core::sync::atomic::Ordering::SeqCst)
                {
                    // Found next process to run
                    let next_pid = next_proc.pid;
                    let next_cr3 = next_proc.cr3.as_u64();
                    let next_state = next_proc.state;
                    let curr = self.current_pid.unwrap_or(0);
                    self.current_pid = CurrentProcessAction::SaveAndSwitch(next_pid);

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
                    self.time = 0; // reset time for new process
                    return Some((next_pid, next_cr3, next_state));
                } else if next_proc.status == ProcessStatus::Ready {
                    // Found next process to run
                    let next_pid = next_proc.pid;
                    let next_cr3 = next_proc.cr3.as_u64();
                    let next_state = next_proc.state;
                    let curr = self.current_pid.unwrap_or(0);
                    self.current_pid = CurrentProcessAction::SaveAndSwitch(next_pid);

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
                    self.time = 0; // reset time for new process
                    return Some((next_pid, next_cr3, next_state));
                }
                // Move cursor to next for future calls
                self.cursor = (self.cursor + 1) % len;
            }

            // No other process ready, continue with current if it exists
            None
        } else {
            self.time += 1;
            None
        }
    }
}
