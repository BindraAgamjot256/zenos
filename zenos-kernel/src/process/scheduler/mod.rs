use crate::process::{Process, PROCESSES};

// Scheduler
pub struct Scheduler {
    cursor: usize,
}

impl Scheduler {
    pub const fn new() -> Self {
        Scheduler { cursor: 0 }
    }

    // Add a new process
    pub fn add_proc(&mut self, proc: Process) {
        PROCESSES.lock().push(proc);
    }

    // Get next process to run (round-robin)
    pub fn next_proc(&mut self) -> Option<usize> {
        let procs = PROCESSES.lock();
        if procs.is_empty() {
            return None;
        }

        let idx = self.cursor % procs.len();
        self.cursor = (self.cursor + 1) % procs.len();
        Some(idx)
    }
}
