use crate::{
    arch::{CpuContext, common::timers::Instant},
    irq::IRQ_CONTROLLER,
    thread::{Thread, ThreadRef},
};
use ::scheduler::{Scheduler as CoreScheduler, Tid, Timer as CoreTimer};
use alloc::vec;
use core::mem::ManuallyDrop;
use core::{
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use kprimitives::spinlock::SpinLock;

#[derive(Debug, Clone, Copy)]
struct Timer;

impl CoreTimer for Timer {
    fn now(&self) -> Instant {
        crate::arch::CLOCKSOURCE
            .read()
            .as_ref()
            .map(|c| c.now())
            .unwrap_or_default()
    }

    fn program_next(&self, duration: Duration) {
        let cont = crate::irq::IRQ_CONTROLLER.read();
        let (timer, mut guard) = cont.timer().expect("We don't have a fucking timer yet");

        let res = guard.set_handler_locked(timer_handler);
        #[expect(
            unpredictable_function_pointer_comparisons,
            reason = "timer_handler is a static function pointer"
        )]
        if res.is_err_and(|f| f != timer_handler) {
            panic!("I OWN THE TIMER. WHO DARES TO STEAL MY TIMER.")
        }
        timer.next_tick(duration)
    }
}

fn timer_handler(cx: &mut CpuContext) {
    log::info!("TIMER.");
    let mut guard = SCHEDULER.lock();
    let guard = guard.as_mut().expect("The Scheduler is initialised.");
    guard.on_timer(cx);

    IRQ_CONTROLLER.read().send_eoi();
}

#[derive(Debug)]
pub struct Scheduler {
    core: CoreScheduler<Thread, Timer>,
    timer: Timer,
}

impl Scheduler {
    pub fn new() -> Self {
        Self {
            core: CoreScheduler::new(Timer),
            timer: Timer,
        }
    }

    pub fn insert_current(&mut self) {
        let current = Thread::new_current(Tid::new(1)).expect("failed to allocate current thread");
        self.core
            .insert_current(current)
            .expect("failed to register current thread");
    }

    pub fn new_thread(&mut self, entry: fn(), stack_ptr: *mut u8) -> Tid {
        static NEXT_TID: AtomicU64 = AtomicU64::new(2);

        let tid = Tid::new(NEXT_TID.fetch_add(1, Ordering::Relaxed));
        let thread =
            Thread::new_kernel(tid, entry, stack_ptr).expect("failed to allocate kernel thread");
        self.core
            .new_thread(thread)
            .expect("failed to register kernel thread");
        tid
    }

    pub fn save_current(&mut self, cx: &mut CpuContext) {
        let Some(current) = self.core.current() else {
            panic!("what current?");
        };

        unsafe { current.update_context(cx) };
    }

    pub fn on_timer(&mut self, cx: &mut CpuContext) -> ThreadRef {
        self.save_current(cx);
        let next = self.core.on_timer();

        *cx = next.context().clone();
        next
    }

    pub fn sleep_current(&mut self, duration: Duration) -> ThreadRef {
        self.core.sleep_current(duration)
    }

    fn program_first_timer(&self) {
        self.timer.program_next(Duration::from_millis(5));
    }

    pub fn current_tid(&self) -> Option<&Tid> {
        self.core.curr_tid()
    }
}

pub static SCHEDULER: SpinLock<Option<Scheduler>> = SpinLock::new(None);

pub fn init() {
    log::info!("scheduler::init");

    let int = crate::arch::disable_interrupts_and_save();

    let mut scheduler = Scheduler::new();
    scheduler.insert_current();

    log::info!("scheduler: {:?}", scheduler);

    let mut guard = SCHEDULER.lock();
    assert!(guard.is_none(), "scheduler initialized more than once");
    *guard = Some(scheduler);

    drop(guard);

    let timer = crate::irq::IRQ_CONTROLLER.read().timer().unwrap().0;
    timer.toggle();
    SCHEDULER.lock().as_ref().unwrap().program_first_timer();
    crate::arch::restore_interrupts(int | (1 << 9));
}

pub fn init_finished(_init_proc_tid: Tid) {
    let int = crate::arch::disable_interrupts_and_save();
    let mut idle_stack = ManuallyDrop::new(vec![0u8; 4096]);
    unsafe {
        SCHEDULER
            .lock()
            .as_mut()
            .unwrap()
            .new_thread(idle_thread, idle_stack.as_mut_ptr().add(4096));
    }
    crate::arch::restore_interrupts(int);

    todo!("starting the init process after kernel initialization is not implemented")
}

fn idle_thread() {
    loop {
        crate::halt_and_catch_fire!();
    }
}
