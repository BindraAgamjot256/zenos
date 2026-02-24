use crate::process::{PROCESSES, ProcessState, ProcessStatus, set_current_pid};
use log::{debug, trace};

/// Priority-based scheduler for preemptive multitasking
/// Lower priority number = higher priority (runs first)
pub struct Scheduler {
    /// PID of the currently running process (if any)
    current_pid: Option<u64>,
    #[cfg(debug_assertions)]
    times_scheduled: u64,
    /// Time quantum in milliseconds
    quantum: u64,
    /// Time used by the current process in its time slice
    time: u64,
}

impl Scheduler {
    pub const fn new() -> Self {
        Scheduler {
            current_pid: None,
            #[cfg(debug_assertions)]
            times_scheduled: 0,
            quantum: 5, // milliseconds
            time: 0,
        }
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

    /// Force the scheduler to switch on the next timer tick
    pub fn force_reschedule(&mut self) {
        self.time = self.quantum;
    }

    /// Schedule: save the current process state and switch to next
    /// Called from timer interrupt handler
    /// Returns (next_pid, next_cr3, next_context) if there's a process to switch to
    /// Uses priority-based scheduling: lower priority number = higher priority
    pub fn schedule(&mut self, current_state: &ProcessState) -> Option<(u64, u64, ProcessState)> {
        if self.quantum == self.time {
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
            } else if let None = self.current_pid {
                debug!("schedule: no current_pid set in scheduler!");
            }

            let len = procs.len();
            if len == 0 {
                return None;
            }

            // Debug: log all processes and their states
            debug!("Schedule: {} processes", len);

            // First pass: handle WaitingFor processes that can be woken up
            // and find the best candidate process index
            let mut best_idx: Option<usize> = None;
            let mut best_priority: u8 = u8::MAX;
            let mut reap_idx: Option<usize> = None;
            let mut wake_idx: Option<(usize, u64)> = None; // (idx, exit_code to set in rax)

            for i in 0..len {
                let proc = &procs[i];

                // Check if this is a WaitingFor process that can be woken
                if let ProcessStatus::WaitingFor(target_pid) = proc.status {
                    let target_idx = procs.iter().position(|t| t.pid == target_pid);
                    let exit_code = target_idx.and_then(|idx| procs[idx].exit_code);
                    if exit_code.is_some() || target_idx.is_none() {
                        let ec = exit_code.unwrap_or(u64::MAX);
                        if proc.priority < best_priority {
                            best_priority = proc.priority;
                            best_idx = Some(i);
                            wake_idx = Some((i, ec));
                            reap_idx = target_idx;
                        }
                        continue;
                    }
                }

                // Check if blocked process became unblocked
                if let ProcessStatus::Blocked(flag) = proc.status {
                    if !flag.load(core::sync::atomic::Ordering::SeqCst) {
                        if proc.priority < best_priority {
                            best_priority = proc.priority;
                            best_idx = Some(i);
                        }
                        continue;
                    }
                }

                // Check if process is ready
                if proc.status == ProcessStatus::Ready && proc.priority < best_priority {
                    best_priority = proc.priority;
                    best_idx = Some(i);
                }
            }

            // No runnable process found
            let best_idx = best_idx?;

            // Handle waking from wait if needed
            if let Some((_, exit_code)) = wake_idx {
                procs[best_idx].state.rax = exit_code;
            }

            let next_proc = &mut procs[best_idx];
            let next_pid = next_proc.pid;
            let next_cr3 = next_proc.cr3.as_u64();
            let next_state = next_proc.state;

            next_proc.status = ProcessStatus::Running;
            self.current_pid = Some(next_pid);
            self.set_current(next_pid);

            // Reap zombie process if needed
            if let Some(idx) = reap_idx {
                debug!("Reaping zombie process at index {}", idx);
                procs.remove(idx);
            }

            #[cfg(debug_assertions)]
            {
                self.times_scheduled += 1;
            }
            self.time = 0;
            Some((next_pid, next_cr3, next_state))
        } else {
            self.time += 1;
            None
        }
    }
}
