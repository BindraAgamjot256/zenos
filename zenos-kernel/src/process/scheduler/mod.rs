use crate::process::{PROCESSES, ProcessState, ProcessStatus, set_current_pid};
use log::{debug, error, info, trace};

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

            // Iterate through all processes to find one that's ready
            let start_cursor = self.cursor;

            // Debug: log all processes and their states
            debug!("Schedule: {} processes, cursor={}", len, start_cursor);

            loop {
                if self.cursor >= len {
                    self.cursor = 0;
                }

                // Pre-check for WaitingFor status and get target exit info
                let waiting_info: Option<(u64, Option<u64>, Option<usize>)> =
                    procs.get(self.cursor).and_then(|p| {
                        if let ProcessStatus::WaitingFor(target_pid) = p.status {
                            let target_idx = procs.iter().position(|t| t.pid == target_pid);
                            let exit_code = target_idx.and_then(|idx| procs[idx].exit_code);
                            if exit_code.is_some() || target_idx.is_none() {
                                Some((target_pid, exit_code, target_idx))
                            } else {
                                None
                            }
                        } else {
                            None
                        }
                    });

                let proc = procs.get_mut(self.cursor);
                if let Some(next_proc) = proc {
                    if let ProcessStatus::Blocked(flag) = next_proc.status
                        && !flag.load(core::sync::atomic::Ordering::SeqCst)
                    {
                        // Blocked process became unblocked
                        let next_pid = next_proc.pid;
                        let next_cr3 = next_proc.cr3.as_u64();
                        let next_state = next_proc.state;
                        let curr = self.current_pid.unwrap_or(0);
                        self.current_pid = CurrentProcessAction::SaveAndSwitch(next_pid);

                        self.cursor = (self.cursor + 1) % len;
                        next_proc.status = ProcessStatus::Running;
                        self.set_current(next_pid);
                        if next_pid == curr {
                            info!(
                                "Scheduler chose the same process (pid {}) to run again",
                                next_pid
                            );
                            return None;
                        }

                        #[cfg(debug_assertions)]
                        {
                            self.times_scheduled += 1;
                            debug!(
                                "Scheduling switch to pid {} (times scheduled: {})",
                                next_pid, self.times_scheduled
                            );
                        }
                        self.time = 0;
                        return Some((next_pid, next_cr3, next_state));
                    } else if let ProcessStatus::WaitingFor(_target_pid) = next_proc.status {
                        // Use pre-computed waiting_info to avoid borrow conflicts
                        if let Some((target_pid, maybe_exit_code, target_idx)) = waiting_info {
                            let exit_code = maybe_exit_code.unwrap_or(u64::MAX);
                            next_proc.state.rax = exit_code;

                            let next_pid = next_proc.pid;
                            let next_cr3 = next_proc.cr3.as_u64();
                            let next_state = next_proc.state;
                            self.current_pid = CurrentProcessAction::SaveAndSwitch(next_pid);

                            self.cursor = (self.cursor + 1) % len;
                            next_proc.status = ProcessStatus::Running;
                            self.set_current(next_pid);

                            // Reap the zombie process
                            if let Some(idx) = target_idx {
                                debug!("Reaping zombie process pid {}", target_pid);
                                procs.remove(idx);
                            }

                            #[cfg(debug_assertions)]
                            {
                                self.times_scheduled += 1;
                                debug!(
                                    "Scheduling switch to pid {} (wait completed, exit_code={}, times scheduled: {})",
                                    next_pid, exit_code, self.times_scheduled
                                );
                            }
                            self.time = 0;
                            return Some((next_pid, next_cr3, next_state));
                        }
                        // Target hasn't exited yet, try next process
                    } else if next_proc.status == ProcessStatus::Ready {
                        // Found a ready process
                        let next_pid = next_proc.pid;
                        let next_cr3 = next_proc.cr3.as_u64();
                        let next_state = next_proc.state;
                        let curr = self.current_pid.unwrap_or(0);
                        self.current_pid = CurrentProcessAction::SaveAndSwitch(next_pid);

                        self.cursor = (self.cursor + 1) % len;
                        next_proc.status = ProcessStatus::Running;
                        self.set_current(next_pid);
                        if next_pid == curr {
                            info!(
                                "Scheduler chose the same process (pid {}) to run again",
                                next_pid
                            );
                            return None;
                        }

                        #[cfg(debug_assertions)]
                        {
                            self.times_scheduled += 1;
                            debug!(
                                "Scheduling switch to pid {} (times scheduled: {})",
                                next_pid, self.times_scheduled
                            );
                        }
                        self.time = 0;
                        return Some((next_pid, next_cr3, next_state));
                    }
                }

                // Move to next process
                self.cursor = (self.cursor + 1) % len;

                // If we've checked all processes, no one is ready
                if self.cursor == start_cursor {
                    break;
                }
            }

            // No process ready
            None
        } else {
            self.time += 1;
            None
        }
    }
}
