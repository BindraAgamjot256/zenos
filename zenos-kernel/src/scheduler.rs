use crate::{arch::common::timers::Instant, thread::Thread};
use ::scheduler::{Scheduler, Tid, Timer};
use core::time::Duration;
use kprimitives::rwlock::RwLock;

struct KernelTimer;

impl Timer for KernelTimer {
    fn now(&self) -> Instant {
        crate::arch::CLOCKSOURCE
            .read()
            .as_ref()
            .expect("clocksource is not initialized")
            .now()
    }

    fn program_next(&self, _duration: Duration) {
        todo!("timer interrupt scheduling is not wired up yet")
    }
}

static SCHEDULER: RwLock<Option<Scheduler<Thread, KernelTimer>>> = RwLock::new(None);

pub fn init() {
    let current = Thread::new_current(Tid::new(1)).expect("failed to allocate current thread");
    let idle =
        Thread::new_kernel(Tid::new(2), idle_thread).expect("failed to allocate idle thread");

    let mut scheduler = Scheduler::new(KernelTimer);
    scheduler
        .insert_current(current)
        .expect("failed to register current thread");
    scheduler
        .new_thread(idle)
        .expect("failed to register idle thread");

    let mut guard = SCHEDULER.write();
    assert!(guard.is_none(), "scheduler initialized more than once");
    *guard = Some(scheduler);
}

fn idle_thread() {
    loop {
        crate::halt_and_catch_fire!();
    }
}
