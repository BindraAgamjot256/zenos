use self::thread::{Thread, ThreadRef};
use crate::{
    arch::{
        CpuContext,
        common::{SpinLock, timers::Instant},
        context_switch::context_switch,
    },
    irq::IRQ_CONTROLLER,
};
pub use ::scheduler::Tid;
use ::scheduler::{Scheduler as CoreScheduler, Timer as CoreTimer};
use alloc::vec;
use core::{
    mem::ManuallyDrop,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};

pub mod sync;
mod thread;

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
    let next = SCHEDULER.on_timer(cx);
    IRQ_CONTROLLER.read().send_eoi();
    *cx = unsafe { next.context().clone() };
}

#[derive(Debug)]
pub struct Scheduler {
    core: SpinLock<CoreScheduler<Thread, Timer>>,
    timer: Timer,
}

impl Scheduler {
    pub const fn new() -> Self {
        Self {
            core: SpinLock::new(CoreScheduler::new(Timer)),
            timer: Timer,
        }
    }

    pub fn insert_current(&self) {
        let current = Thread::new_current(Tid::new(1)).expect("failed to allocate current thread");
        self.core
            .lock()
            .insert_current(current)
            .expect("failed to register current thread");
    }

    pub fn new_thread(&self, entry: fn(), stack_ptr: *mut u8) -> Tid {
        static NEXT_TID: AtomicU64 = AtomicU64::new(2);

        let tid = Tid::new(NEXT_TID.fetch_add(1, Ordering::Relaxed));
        let thread =
            Thread::new_kernel(tid, entry, stack_ptr).expect("failed to allocate kernel thread");
        self.core
            .lock()
            .new_thread(thread)
            .expect("failed to register kernel thread");
        tid
    }

    pub fn save_current(&self, cx: &mut CpuContext) {
        let Some(current) = self.core.lock().current() else {
            panic!("what current?");
        };

        unsafe { current.update_context(cx) };
    }

    pub fn block_current_after<F>(&self, op: F)
    where
        F: FnOnce(ThreadRef),
    {
        let mut lock = self.core.lock();
        let curr = lock.current().unwrap();

        op(curr.clone());

        let next = lock.block_current();

        drop(lock);

        unsafe { context_switch(&raw mut *(curr.context()), &raw mut *(next.context())) };
    }

    pub fn wake(&self, thread: ThreadRef) {
        self.core.lock().wake(thread)
    }

    pub fn on_timer(&self, cx: &mut CpuContext) -> ThreadRef {
        self.save_current(cx);
        let next = self.core.lock().on_timer();

        unsafe { *cx = next.context().clone() };
        next
    }

    pub fn sleep_current(&self, duration: Duration) {
        let mut lock = self.core.lock();

        let curr = lock.current().unwrap();
        let next = lock.sleep_current(duration);

        drop(lock);

        unsafe { context_switch(&raw mut *(curr.context()), &raw mut *(next.context())) };
    }

    fn program_first_timer(&self) {
        self.timer.program_next(Duration::from_millis(5));
    }
}

impl Default for Scheduler {
    fn default() -> Self {
        Self::new()
    }
}

pub static SCHEDULER: Scheduler = Scheduler::new();

pub fn init() {
    log::info!("scheduler::init");

    let scheduler = &SCHEDULER;
    scheduler.insert_current();

    log::info!("scheduler: {:?}", scheduler);
    let timer = crate::irq::IRQ_CONTROLLER.read().timer().unwrap().0;
    timer.toggle();
    SCHEDULER.program_first_timer();

    log::info!("scheduler: {:#?}", scheduler);
    crate::arch::restore_interrupts(0x202);

    #[cfg(feature = "__test_mutex")]
    run_mutex_tests();
}

pub fn init_finished(_init_proc_tid: Tid) {
    let mut idle_stack = ManuallyDrop::new(vec![0u8; 4096]);
    unsafe {
        SCHEDULER.new_thread(idle_thread, idle_stack.as_mut_ptr().add(4096));
    }
    todo!("starting the init process after kernel initialization is not implemented")
}

fn idle_thread() {
    log::info!("idle thread started");
    loop {
        crate::halt_and_catch_fire!();
    }
}

#[cfg(feature = "__test_mutex")]
fn run_mutex_tests() {
    use self::sync::Mutex;

    use core::{
        hint::spin_loop,
        mem::ManuallyDrop,
        sync::atomic::{AtomicBool, AtomicUsize, Ordering},
    };

    const STACK_SIZE: usize = 4096;
    const N_WAITERS: usize = 4;
    const N_STRESS_THREADS: usize = 4;
    const ITERATIONS: usize = 10_000;
    const HANDOFF_ROUNDS: usize = 128;

    // =========================================================
    // Test infrastructure
    // =========================================================

    fn spawn(entry: fn()) {
        let stack = ManuallyDrop::new(vec![0u8; STACK_SIZE]);

        SCHEDULER.new_thread(entry, unsafe { stack.as_ptr().add(STACK_SIZE).cast_mut() });
    }

    fn park_forever() -> ! {
        loop {
            crate::halt_and_catch_fire!();
        }
    }

    fn wait_until(counter: &AtomicUsize, value: usize) {
        while counter.load(Ordering::Acquire) < value {
            spin_loop();
        }
    }

    // =========================================================
    // TEST 1: Uncontended locking
    // =========================================================

    static BASIC: Mutex<usize> = Mutex::new(0);

    {
        let mut guard = BASIC.lock();

        assert_eq!(*guard, 0);

        *guard = 42;

        assert_eq!(*guard, 42);
    }

    {
        let mut guard = BASIC.lock();

        assert_eq!(*guard, 42);

        *guard += 1;
    }

    assert_eq!(*BASIC.lock(), 43);

    assert_eq!(
        BASIC.test_queued_count(),
        0,
        "uncontended mutex unexpectedly queued"
    );

    log::info!("mutex: basic operations PASSED");

    // =========================================================
    // TEST 2: Forced blocking and wakeup
    //
    // Main thread owns the mutex.
    // Contender must enter the wait queue.
    // Only then is the mutex released.
    // =========================================================

    static HANDOFF: Mutex<usize> = Mutex::new(0);

    static HANDOFF_DONE: AtomicBool = AtomicBool::new(false);

    static HANDOFF_RELEASED: AtomicBool = AtomicBool::new(false);

    fn handoff_worker() {
        let mut guard = HANDOFF.lock();

        assert!(
            HANDOFF_RELEASED.load(Ordering::Acquire),
            "contender acquired before owner released"
        );

        assert_eq!(*guard, 200);

        *guard = 300;

        drop(guard);

        HANDOFF_DONE.store(true, Ordering::Release);

        park_forever();
    }

    {
        let mut guard = HANDOFF.lock();

        *guard = 100;

        spawn(handoff_worker);

        // Require actual wait-queue insertion.
        while HANDOFF.test_queued_count() < 1 {
            spin_loop();
        }

        assert_eq!(*guard, 100);

        assert!(
            !HANDOFF_DONE.load(Ordering::Acquire),
            "contender ran critical section early"
        );

        *guard = 200;

        HANDOFF_RELEASED.store(true, Ordering::Release);

        drop(guard);
    }

    while !HANDOFF_DONE.load(Ordering::Acquire) {
        spin_loop();
    }

    assert_eq!(*HANDOFF.lock(), 300);

    assert!(HANDOFF.test_queued_count() >= 1, "contender never queued");

    log::info!("mutex: forced block/wakeup PASSED");

    // =========================================================
    // TEST 3: Multiple blocked waiters
    //
    // Force all N_WAITERS into the wait queue before
    // releasing the mutex.
    // =========================================================

    static WAITERS: Mutex<usize> = Mutex::new(0);

    static WAITERS_DONE: AtomicUsize = AtomicUsize::new(0);

    static ACTIVE_IN_CS: AtomicUsize = AtomicUsize::new(0);

    static MAX_IN_CS: AtomicUsize = AtomicUsize::new(0);

    fn waiter_worker() {
        let mut guard = WAITERS.lock();

        let active = ACTIVE_IN_CS.fetch_add(1, Ordering::SeqCst) + 1;

        MAX_IN_CS.fetch_max(active, Ordering::SeqCst);

        assert_eq!(active, 1, "multiple mutex owners detected");

        let old = *guard;
        *guard = old + 1;

        assert_eq!(
            ACTIVE_IN_CS.fetch_sub(1, Ordering::SeqCst,),
            1,
            "critical section state corrupted"
        );

        drop(guard);

        WAITERS_DONE.fetch_add(1, Ordering::AcqRel);

        park_forever();
    }

    {
        let guard = WAITERS.lock();

        for _ in 0..N_WAITERS {
            spawn(waiter_worker);
        }

        // All waiters must join the queue while
        // the coordinator still owns the mutex.
        while WAITERS.test_queued_count() < N_WAITERS {
            spin_loop();
        }

        assert_eq!(*guard, 0);

        assert_eq!(
            WAITERS_DONE.load(Ordering::Acquire),
            0,
            "waiter completed before release"
        );

        assert_eq!(ACTIVE_IN_CS.load(Ordering::SeqCst), 0);

        drop(guard);
    }

    wait_until(&WAITERS_DONE, N_WAITERS);

    assert_eq!(*WAITERS.lock(), N_WAITERS);

    assert_eq!(WAITERS_DONE.load(Ordering::Acquire), N_WAITERS);

    assert_eq!(ACTIVE_IN_CS.load(Ordering::SeqCst), 0);

    assert_eq!(MAX_IN_CS.load(Ordering::SeqCst), 1);

    log::info!("mutex: multiple blocked waiters PASSED");

    // =========================================================
    // TEST 4: High-frequency contention
    //
    // Four workers, synchronized start gate,
    // 40,000 total increments.
    // =========================================================

    static STRESS: Mutex<usize> = Mutex::new(0);

    static STRESS_READY: AtomicUsize = AtomicUsize::new(0);

    static STRESS_START: AtomicBool = AtomicBool::new(false);

    static STRESS_DONE: AtomicUsize = AtomicUsize::new(0);

    static STRESS_ACTIVE: AtomicUsize = AtomicUsize::new(0);

    fn stress_worker() {
        STRESS_READY.fetch_add(1, Ordering::AcqRel);

        while !STRESS_START.load(Ordering::Acquire) {
            spin_loop();
        }

        for iteration in 0..ITERATIONS {
            let mut guard = STRESS.lock();

            assert_eq!(
                STRESS_ACTIVE.fetch_add(1, Ordering::SeqCst,),
                0,
                "simultaneous mutex ownership"
            );

            let old = *guard;

            *guard = old + 1;

            assert_eq!(*guard, old + 1);

            assert_eq!(
                STRESS_ACTIVE.fetch_sub(1, Ordering::SeqCst,),
                1,
                "critical section accounting corrupted"
            );

            drop(guard);

            // Small timing variation outside the lock.
            if iteration % 16 == 0 {
                for _ in 0..32 {
                    spin_loop();
                }
            }
        }

        STRESS_DONE.fetch_add(1, Ordering::AcqRel);

        park_forever();
    }

    for _ in 0..N_STRESS_THREADS {
        spawn(stress_worker);
    }

    wait_until(&STRESS_READY, N_STRESS_THREADS);

    STRESS_START.store(true, Ordering::Release);

    wait_until(&STRESS_DONE, N_STRESS_THREADS);

    assert_eq!(
        *STRESS.lock(),
        N_STRESS_THREADS * ITERATIONS,
        "lost increments in contention test"
    );

    assert_eq!(STRESS_ACTIVE.load(Ordering::SeqCst), 0);

    log::info!(
        "mutex: stress PASSED ({} increments)",
        N_STRESS_THREADS * ITERATIONS
    );

    log::info!(
        "mutex: stress queue insertions = {}",
        STRESS.test_queued_count()
    );

    // =========================================================
    // TEST 5: Repeated forced block/wakeup
    //
    // Reuse one worker for 128 rounds.
    //
    // Every round:
    //   Coordinator holds mutex.
    //   Worker is instructed to acquire it.
    //   Worker joins wait queue.
    //   Coordinator unlocks.
    //   Worker acquires and increments.
    //   Repeat.
    // =========================================================

    static REPEAT: Mutex<usize> = Mutex::new(0);

    static REPEAT_REQUEST: AtomicUsize = AtomicUsize::new(0);

    static REPEAT_DONE: AtomicUsize = AtomicUsize::new(0);

    fn repeat_worker() {
        for round in 1..=HANDOFF_ROUNDS {
            // Wait until coordinator holds the mutex
            // and authorizes this round.
            wait_until(&REPEAT_REQUEST, round);

            let mut guard = REPEAT.lock();

            assert_eq!(*guard, round - 1, "repeated handoff state mismatch");

            *guard += 1;

            drop(guard);

            REPEAT_DONE.store(round, Ordering::Release);
        }

        park_forever();
    }

    spawn(repeat_worker);

    for round in 1..=HANDOFF_ROUNDS {
        let guard = REPEAT.lock();

        // Authorize exactly one acquisition.
        REPEAT_REQUEST.store(round, Ordering::Release);

        // Force the worker into the wait queue.
        while REPEAT.test_queued_count() < round {
            spin_loop();
        }

        assert_eq!(*guard, round - 1, "mutex modified while owned");

        drop(guard);

        // Worker must complete the current round.
        wait_until(&REPEAT_DONE, round);

        assert_eq!(*REPEAT.lock(), round, "incorrect value after handoff");
    }

    assert_eq!(*REPEAT.lock(), HANDOFF_ROUNDS);

    assert_eq!(
        REPEAT.test_queued_count(),
        HANDOFF_ROUNDS,
        "unexpected number of forced queue entries"
    );

    log::info!("mutex: {} forced handoffs PASSED", HANDOFF_ROUNDS);

    // =========================================================
    // TEST 6: Independent mutexes
    //
    // Holding mutex A must not prevent acquiring B.
    // =========================================================

    static INDEPENDENT_A: Mutex<usize> = Mutex::new(10);

    static INDEPENDENT_B: Mutex<usize> = Mutex::new(20);

    {
        let mut a = INDEPENDENT_A.lock();
        let mut b = INDEPENDENT_B.lock();

        *a += 1;
        *b += 2;

        assert_eq!(*a, 11);
        assert_eq!(*b, 22);
    }

    assert_eq!(*INDEPENDENT_A.lock(), 11);
    assert_eq!(*INDEPENDENT_B.lock(), 22);

    log::info!("mutex: independent locks PASSED");

    // =========================================================
    // Final results
    // =========================================================

    log::info!("==================================");
    log::info!("ALL MUTEX TESTS PASSED");
    log::info!("==================================");

    idle_thread();
}

pub fn current_thread() -> Option<ThreadRef> {
    SCHEDULER.core.lock().current().clone()
}
