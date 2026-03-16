use crate::interrupts::gdt::GDT;
use crate::process;
use crate::process::{
    IDLE_STACK, PROCESSES, Process, ProcessState, ProcessStatus, set_current_pid, set_current_proc,
};
use log::{debug, info, trace};
use x86_64::registers::control::Cr3;

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
    pub fn set_current(&mut self, pid: u64, proc_ptr: *mut Process) {
        self.current_pid = Some(pid);
        set_current_pid(pid);
        unsafe {
            set_current_proc(proc_ptr);
        }
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
        if self.quantum >= self.time {
            // Use try_lock to avoid deadlock with syscalls holding the lock
            let mut procs = PROCESSES.try_lock()?;

            // Save state of current process if there is one
            if let Some(current_pid) = self.current_pid {
                if let Some(current_proc) = procs.iter_mut().find(|p| p.pid == current_pid) {
                    match current_proc.status {
                        ProcessStatus::Running => {
                            current_proc.save_context(current_state);
                            current_proc.status = ProcessStatus::Ready;
                            trace!("Saved context for pid {}", current_pid);
                        }
                        ProcessStatus::Blocked(_) => {
                            // Also save context for blocked processes so they resume correctly
                            // when unblocked (e.g., from stdin read inside syscall)
                            current_proc.save_context(current_state);
                            trace!("Saved context for blocked pid {}", current_pid);
                        }
                        _ => {}
                    }
                }
            } else if let None = self.current_pid {
                debug!("schedule: no current_pid set in scheduler!");
            }

            let len = procs.len();
            if len == 0 {
                return None;
            }

            // First pass: handle WaitingFor processes that can be woken up
            // and find the best candidate process index
            let mut best_idx: Option<usize> = None;
            let mut best_priority: u8 = u8::MAX;
            let mut reap_idx: Option<usize> = None;
            let mut wake_idx: Option<(usize, u64)> = None; // (idx, exit_code to set in rax)

            for (i, proc) in procs.iter().enumerate() {
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
                            info!(
                                "Waking process pid {} (index {}, priority {}) waiting for pid {} with exit code {} (reap idx {:?})",
                                proc.pid, i, proc.priority, target_pid, ec, reap_idx
                            );
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
                            info!(
                                "Unblocked process pid {} (index {}, priority {}) is now ready to run",
                                proc.pid, i, proc.priority
                            );
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
            let mut pstate = ProcessState::default();
            pstate.rflags = 0x202; // Interrupt Enable flag set
            pstate.cs = GDT.code_selector.0 as u64;
            pstate.ss = GDT.data_selector.0 as u64;
            pstate.rip = process::idle_loop as *mut () as u64;
            pstate.rsp = unsafe { IDLE_STACK.as_ptr() as u64 + IDLE_STACK.len() as u64 };

            let best_idx = match best_idx {
                Some(idx) => idx,
                None => {
                    return Some((0, Cr3::read().0.start_address().as_u64(), pstate)); // No process to run, return dummy values
                }
            };

            // Handle waking from wait if needed
            if let Some((_, exit_code)) = wake_idx {
                procs[best_idx].state.rax = exit_code;
            }

            let next_proc = &mut procs[best_idx];
            let next_pid = next_proc.pid;
            let next_cr3 = next_proc.cr3.as_u64();
            let next_state = next_proc.state;
            let proc_ptr = next_proc as *mut Process;

            next_proc.status = ProcessStatus::Running;
            self.current_pid = Some(next_pid);
            self.set_current(next_pid, proc_ptr);

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
