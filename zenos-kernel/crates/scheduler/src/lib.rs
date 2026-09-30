#![cfg_attr(not(test), no_std)]
#![expect(unused)]

extern crate alloc;

use crate::threads::SleepEntry;
use alloc::collections::{BTreeMap, btree_map::Entry};
use core::{
    cmp::min,
    sync::atomic::{AtomicU64, Ordering},
    time::Duration,
};
use kprimitives::{alloc::sync::KArc, time::Instant};

mod queue;
pub mod threads;
pub mod timing;

pub use threads::{Thread, ThreadState, Tid};
pub use timing::Timer;

#[expect(type_alias_bounds)]
pub type ThreadKArc<T: Thread> = KArc<T, T::Allocator>;

const QUANTUM: Duration = Duration::from_millis(5);

#[derive(Debug)]
pub struct Scheduler<T: Thread, U: timing::Timer> {
    current: Option<Tid>,
    sleep_queue: queue::SleepQueue<T>,
    threads: BTreeMap<threads::Tid, ThreadKArc<T>>,
    run_queue: queue::RunQueue<T>,
    timer: U,
    quantum: Option<Instant>,
}

impl<T: Thread, U: timing::Timer> Scheduler<T, U> {
    pub const fn new(timer: U) -> Self {
        Self {
            current: None,
            sleep_queue: queue::SleepQueue::new(),
            threads: BTreeMap::new(),
            run_queue: queue::RunQueue::new(),
            timer,
            quantum: None,
        }
    }

    fn program_next_deadline(&mut self, now: Instant) {
        let sleep_deadline = self
            .sleep_queue
            .peek()
            .map(|entry| entry.deadline)
            .unwrap_or(Instant::new(u128::MAX));

        let deadline = min(
            self.quantum.unwrap_or(Instant::new(u128::MAX)),
            sleep_deadline,
        );

        self.timer.program_next(deadline - now);
    }

    pub fn insert(&mut self, thread: ThreadKArc<T>) -> Result<(), ThreadKArc<T>> {
        let tid = thread.tid();
        let entry = self.threads.entry(tid);
        match entry {
            Entry::Occupied(_) => Err(thread),
            Entry::Vacant(e) => {
                e.insert(thread);
                Ok(())
            }
        }
    }

    pub fn insert_current(&mut self, thread: ThreadKArc<T>) -> Result<(), ThreadKArc<T>> {
        let None = self.current else {
            let thread = self.threads.get(self.current.as_ref().unwrap()).unwrap();
            return Err(ThreadKArc::clone(thread));
        };

        self.insert(ThreadKArc::clone(&thread))?;
        self.current = Some(thread.tid());
        Ok(())
    }

    pub fn new_thread(&mut self, thread: ThreadKArc<T>) -> Result<(), ThreadKArc<T>> {
        self.insert(ThreadKArc::clone(&thread))?;
        self.run_queue.insert(thread);
        Ok(())
    }

    pub fn schedule_next(&mut self) -> ThreadKArc<T> {
        if let Some(current) = self.current {
            let thread = self.threads.get(&current).unwrap();
            thread.set_state(ThreadState::Ready);
            self.run_queue.insert(thread.clone());
            self.current = None;
        }

        let Some(thread) = self.run_queue.pop() else {
            panic!("No thread available to schedule");
        };

        let now = self.timer.now();

        thread.set_state(ThreadState::Running);
        self.current = Some(thread.tid());

        // New thread = new 5 ms quantum.
        self.quantum = Some(now + QUANTUM);

        self.program_next_deadline(now);

        thread
    }

    pub fn sleep_current(&mut self, duration: Duration) -> ThreadKArc<T> {
        static SEQ: AtomicU64 = AtomicU64::new(0);

        let Some(tid) = self.current else {
            panic!("why must you hurt me in this way???")
        };

        let thread = self
            .threads
            .get(&tid)
            .cloned()
            .expect("Thread should be in the threads map, since it's calling sleep()");

        thread.set_state(ThreadState::Sleeping);
        self.current = None;

        let now = self.timer.now();
        let deadline = now + duration;
        let se = SleepEntry::<T> {
            deadline,
            thread: thread,
            seq: SEQ.fetch_add(1, Ordering::Relaxed),
        };

        self.sleep_queue.push(se);

        let next = self.schedule_next();

        next
    }

    pub fn on_timer(&mut self) -> ThreadKArc<T> {
        let now = self.timer.now();

        // Wake every thread whose sleep deadline has arrived.
        for entry in self.sleep_queue.pop_expired(now) {
            entry.thread.set_state(ThreadState::Ready);
            self.run_queue.push_front(entry.thread);
        }

        // If the current quantum has expired, rotate to the next thread.
        if self.quantum.is_some_and(|deadline| deadline <= now) {
            return self.schedule_next();
        }

        // Only a sleep deadline fired, so the current thread keeps
        // whatever remains of its existing quantum.
        self.program_next_deadline(now);

        let tid = self.current.expect("timer fired with no current thread");
        self.threads
            .get(&tid)
            .cloned()
            .expect("current thread must exist in thread registry")
    }

    pub fn curr_tid(&self) -> Option<&Tid> {
        self.current.as_ref()
    }

    pub fn current(&self) -> Option<ThreadKArc<T>> {
        let tid = self.current?;
        self.threads.get(&tid).cloned()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{
        queue::{RunQueue, SleepQueue},
        threads::{SleepEntry, Thread, ThreadState, Tid},
    };
    use core::{
        alloc::Layout,
        cell::{Cell, RefCell},
        ptr::NonNull,
        sync::atomic::{AtomicUsize, Ordering},
    };
    use kprimitives::alloc::{
        Allocation, AllocationError, Allocator, CreatableKernelObject, KernelObject, RefCounted,
        boxed::KBox,
    };

    struct TestAllocator;

    unsafe impl Allocator for TestAllocator {
        fn allocate(layout: Layout) -> Result<Allocation, AllocationError> {
            let ptr = unsafe { std::alloc::alloc(layout) };
            NonNull::new(ptr)
                .map(Allocation::from_ptr)
                .ok_or(AllocationError::OutOfMemory)
        }

        fn deallocate(allocation: Allocation, layout: Layout) {
            unsafe { std::alloc::dealloc(allocation.as_ptr().as_ptr(), layout) };
        }
    }

    #[derive(Debug)]
    struct TestThread {
        tid: Tid,
        state: Cell<ThreadState>,
        refs: AtomicUsize,
    }

    impl TestThread {
        fn new(tid: u64) -> Self {
            Self {
                tid: Tid::new(tid),
                state: Cell::new(ThreadState::Ready),
                refs: AtomicUsize::new(1),
            }
        }
    }

    impl KernelObject for TestThread {}
    impl CreatableKernelObject for TestThread {
        type Allocator = TestAllocator;
    }
    unsafe impl RefCounted for TestThread {
        fn ref_count(&self) -> usize {
            self.refs.load(Ordering::Relaxed)
        }

        fn inc_refcount(&self, ordering: Ordering) -> usize {
            self.refs.fetch_add(1, ordering) + 1
        }

        fn dec_refcount(&self, ordering: Ordering) -> usize {
            self.refs.fetch_sub(1, ordering) - 1
        }
    }
    impl Thread for TestThread {
        fn tid(&self) -> Tid {
            self.tid
        }

        fn state(&self) -> ThreadState {
            self.state.get()
        }

        fn set_state(&self, state: ThreadState) {
            self.state.set(state);
        }

        #[cfg(debug_assertions)]
        fn owner(&self) -> Option<&'static str> {
            None
        }
    }

    #[derive(Default)]
    struct FakeTimer {
        now: Cell<Instant>,
        programs: RefCell<Vec<Duration>>,
    }

    impl FakeTimer {
        fn at_ms(&self, milliseconds: u128) {
            self.now.set(at_ms(milliseconds));
        }

        fn last_program(&self) -> Duration {
            *self.programs.borrow().last().unwrap()
        }
    }

    impl timing::Timer for FakeTimer {
        fn now(&self) -> Instant {
            self.now.get()
        }

        fn program_next(&self, duration: Duration) {
            self.programs.borrow_mut().push(duration);
        }
    }

    fn at_ms(milliseconds: u128) -> Instant {
        Instant::new(milliseconds * 1_000_000)
    }

    fn thread(tid: u64) -> ThreadKArc<TestThread> {
        KBox::new(TestThread::new(tid)).unwrap().into_arc()
    }

    fn sleep_entry(
        thread: ThreadKArc<TestThread>,
        deadline: u128,
        seq: u64,
    ) -> SleepEntry<TestThread> {
        SleepEntry {
            deadline: at_ms(deadline),
            thread,
            seq,
        }
    }

    #[test]
    fn registration_rejects_duplicate_and_returns_incoming_thread() {
        let mut scheduler = Scheduler::new(FakeTimer::default());
        let registered = thread(1);
        scheduler.insert(registered).unwrap();
        let incoming = thread(1);

        let returned = scheduler.insert(incoming).unwrap_err();

        assert_eq!(returned.tid(), Tid::new(1));
        assert_eq!(returned.ref_count(), 1);
    }

    #[test]
    fn new_thread_registers_and_enqueues() {
        let mut scheduler = Scheduler::new(FakeTimer::default());
        scheduler.new_thread(thread(1)).unwrap();

        let next = scheduler.schedule_next();
        assert_eq!(next.tid(), Tid::new(1));
        assert_eq!(next.state(), ThreadState::Running);
    }

    #[test]
    fn run_queue_is_fifo_and_push_front_has_priority() {
        let mut queue = RunQueue::new();
        queue.insert(thread(1));
        queue.insert(thread(2));
        queue.push_front(thread(3));

        assert_eq!(queue.pop().unwrap().tid(), Tid::new(3));
        assert_eq!(queue.pop().unwrap().tid(), Tid::new(1));
        assert_eq!(queue.pop().unwrap().tid(), Tid::new(2));
        assert!(queue.pop().is_none());
    }

    #[test]
    fn sleep_queue_orders_deadlines_and_equal_deadlines_by_sequence() {
        let mut queue = SleepQueue::new();
        queue.push(sleep_entry(thread(1), 4, 1));
        queue.push(sleep_entry(thread(2), 2, 2));
        queue.push(sleep_entry(thread(3), 2, 1));

        assert_eq!(queue.pop().unwrap().thread.tid(), Tid::new(3));
        assert_eq!(queue.pop().unwrap().thread.tid(), Tid::new(2));
        assert_eq!(queue.pop().unwrap().thread.tid(), Tid::new(1));
    }

    #[test]
    fn sleep_queue_pop_expired_keeps_future_entries() {
        let mut queue = SleepQueue::new();
        queue.push(sleep_entry(thread(1), 2, 0));
        queue.push(sleep_entry(thread(2), 4, 1));

        let expired: Vec<_> = queue.pop_expired(at_ms(2)).collect();
        assert_eq!(expired.len(), 1);
        assert_eq!(expired[0].thread.tid(), Tid::new(1));
        assert_eq!(queue.peek().unwrap().thread.tid(), Tid::new(2));
    }

    #[test]
    fn scheduling_requeues_current_and_starts_a_fresh_quantum() {
        let mut scheduler = Scheduler::new(FakeTimer::default());
        scheduler.new_thread(thread(1)).unwrap();
        scheduler.new_thread(thread(2)).unwrap();

        assert_eq!(scheduler.schedule_next().tid(), Tid::new(1));
        scheduler.timer.at_ms(1);
        assert_eq!(scheduler.schedule_next().tid(), Tid::new(2));

        assert_eq!(scheduler.threads[&Tid::new(1)].state(), ThreadState::Ready);
        assert_eq!(scheduler.quantum, Some(at_ms(6)));
        assert_eq!(scheduler.timer.last_program(), Duration::from_millis(5));
    }

    #[test]
    fn sleeping_current_enqueues_deadline_and_programs_earlier_timer() {
        let mut scheduler = Scheduler::new(FakeTimer::default());
        scheduler.new_thread(thread(1)).unwrap();
        scheduler.new_thread(thread(2)).unwrap();
        scheduler.schedule_next();

        let next = scheduler.sleep_current(Duration::from_millis(2));

        assert_eq!(next.tid(), Tid::new(2));
        assert_eq!(
            scheduler.threads[&Tid::new(1)].state(),
            ThreadState::Sleeping
        );
        assert_eq!(scheduler.sleep_queue.peek().unwrap().deadline, at_ms(2));
        assert_eq!(scheduler.timer.last_program(), Duration::from_millis(2));
    }

    #[test]
    fn early_timer_wake_preserves_quantum_and_reprograms_it() {
        let mut scheduler = Scheduler::new(FakeTimer::default());
        scheduler.new_thread(thread(1)).unwrap();
        scheduler.new_thread(thread(2)).unwrap();
        scheduler.schedule_next();
        scheduler.sleep_current(Duration::from_millis(2));
        scheduler.timer.at_ms(2);

        let current = scheduler.on_timer();

        assert_eq!(current.tid(), Tid::new(2));
        assert_eq!(scheduler.quantum, Some(at_ms(5)));
        assert_eq!(scheduler.threads[&Tid::new(1)].state(), ThreadState::Ready);
        assert_eq!(scheduler.timer.last_program(), Duration::from_millis(3));
    }

    #[test]
    fn quantum_expiry_rotates_to_the_next_ready_thread() {
        let mut scheduler = Scheduler::new(FakeTimer::default());
        scheduler.new_thread(thread(1)).unwrap();
        scheduler.new_thread(thread(2)).unwrap();
        scheduler.schedule_next();
        scheduler.timer.at_ms(5);

        let next = scheduler.on_timer();

        assert_eq!(next.tid(), Tid::new(2));
        assert_eq!(scheduler.threads[&Tid::new(1)].state(), ThreadState::Ready);
        assert_eq!(scheduler.quantum, Some(at_ms(10)));
    }

    #[test]
    fn timer_wakes_multiple_expired_sleepers() {
        let mut scheduler = Scheduler::new(FakeTimer::default());
        scheduler.new_thread(thread(1)).unwrap();
        scheduler.insert(thread(2)).unwrap();
        scheduler.insert(thread(3)).unwrap();
        scheduler.schedule_next();
        scheduler
            .sleep_queue
            .push(sleep_entry(scheduler.threads[&Tid::new(2)].clone(), 2, 0));
        scheduler
            .sleep_queue
            .push(sleep_entry(scheduler.threads[&Tid::new(3)].clone(), 2, 1));
        scheduler.timer.at_ms(2);

        assert_eq!(scheduler.on_timer().tid(), Tid::new(1));
        assert_eq!(scheduler.threads[&Tid::new(2)].state(), ThreadState::Ready);
        assert_eq!(scheduler.threads[&Tid::new(3)].state(), ThreadState::Ready);
        assert_eq!(scheduler.run_queue.pop().unwrap().tid(), Tid::new(3));
        assert_eq!(scheduler.run_queue.pop().unwrap().tid(), Tid::new(2));
    }

    #[test]
    fn collision_wakes_thread_before_older_ready_thread() {
        let mut scheduler = Scheduler::new(FakeTimer::default());
        scheduler.new_thread(thread(1)).unwrap();
        scheduler.new_thread(thread(2)).unwrap();
        scheduler.insert(thread(3)).unwrap();
        scheduler.schedule_next();
        scheduler
            .sleep_queue
            .push(sleep_entry(scheduler.threads[&Tid::new(3)].clone(), 5, 0));
        scheduler.timer.at_ms(5);

        let next = scheduler.on_timer();

        assert_eq!(next.tid(), Tid::new(3));
        assert_eq!(scheduler.run_queue.pop().unwrap().tid(), Tid::new(2));
        assert_eq!(scheduler.run_queue.pop().unwrap().tid(), Tid::new(1));
    }

    #[test]
    fn spurious_timer_keeps_current_thread_and_remaining_quantum() {
        let mut scheduler = Scheduler::new(FakeTimer::default());
        scheduler.new_thread(thread(1)).unwrap();
        scheduler.schedule_next();
        scheduler.timer.at_ms(1);

        assert_eq!(scheduler.on_timer().tid(), Tid::new(1));
        assert_eq!(scheduler.quantum, Some(at_ms(5)));
        assert_eq!(scheduler.timer.last_program(), Duration::from_millis(4));
    }

    #[test]
    #[should_panic(expected = "No thread available to schedule")]
    fn sleeping_the_only_thread_panics() {
        let mut scheduler = Scheduler::new(FakeTimer::default());
        scheduler.new_thread(thread(1)).unwrap();
        scheduler.schedule_next();
        scheduler.sleep_current(Duration::from_millis(1));
    }

    #[test]
    fn repeated_wakes_integrate_with_future_sleepers() {
        let mut scheduler = Scheduler::new(FakeTimer::default());
        scheduler.new_thread(thread(1)).unwrap();
        scheduler.insert(thread(2)).unwrap();
        scheduler.insert(thread(3)).unwrap();
        scheduler.schedule_next();
        scheduler
            .sleep_queue
            .push(sleep_entry(scheduler.threads[&Tid::new(2)].clone(), 1, 0));
        scheduler
            .sleep_queue
            .push(sleep_entry(scheduler.threads[&Tid::new(3)].clone(), 3, 1));

        scheduler.timer.at_ms(1);
        assert_eq!(scheduler.on_timer().tid(), Tid::new(1));
        assert_eq!(scheduler.timer.last_program(), Duration::from_millis(2));
        scheduler.timer.at_ms(3);
        assert_eq!(scheduler.on_timer().tid(), Tid::new(1));
        assert_eq!(scheduler.threads[&Tid::new(2)].state(), ThreadState::Ready);
        assert_eq!(scheduler.threads[&Tid::new(3)].state(), ThreadState::Ready);
    }
}
