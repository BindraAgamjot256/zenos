use crate::{
    ThreadKArc,
    threads::{SleepEntry, Thread},
};
use alloc::{
    collections::{BinaryHeap, VecDeque},
    vec::Vec,
};
use core::fmt::Debug;
use kprimitives::{alloc::sync::KArc, time::Instant};

/// Priority queue of sleeping threads ordered by wakeup deadline.
///
/// Uses a `BinaryHeap` with earliest-deadline-first ordering. Threads with equal
/// deadlines are ordered by sequence number (FIFO).
#[derive(Debug)]
pub(crate) struct SleepQueue<T: Thread> {
    queue: BinaryHeap<SleepEntry<T>>,
}

/// FIFO queue of ready-to-run threads.
///
/// Uses a `VecDeque` for O(1) push/pop at both ends. Supports priority wakeups
/// via `push_front` for interrupt handlers.
pub(crate) struct RunQueue<T: Thread> {
    queue: VecDeque<KArc<T, T::Allocator>>,
}

#[derive(Debug)]
pub struct BlockedQueue<T: Thread> {
    queue: VecDeque<KArc<T, T::Allocator>>,
}

impl<T: Thread> SleepQueue<T> {
    /// Creates an empty sleep queue.
    #[inline]
    pub const fn new() -> Self {
        Self {
            queue: BinaryHeap::new(),
        }
    }

    /// Adds a thread to the sleep queue.
    #[inline]
    pub fn push(&mut self, entry: SleepEntry<T>) {
        self.queue.push(entry);
    }

    /// Removes and returns the thread with the earliest deadline.
    #[inline]
    pub fn pop(&mut self) -> Option<SleepEntry<T>> {
        self.queue.pop()
    }

    /// Returns a reference to the thread with the earliest deadline without removing it.
    #[inline]
    pub fn peek(&self) -> Option<&SleepEntry<T>> {
        self.queue.peek()
    }

    /// Returns an iterator over all expired sleep entries.
    ///
    /// The iterator yields entries whose `deadline <= now`, in deadline order.
    /// The original queue is mutated as entries are consumed.
    #[inline]
    pub fn pop_expired(&mut self, now: Instant) -> ExpiredIterator<'_, T> {
        ExpiredIterator {
            queue: &mut self.queue,
            now,
        }
    }
}

impl<T: Thread> RunQueue<T> {
    /// Creates an empty run queue.
    #[inline]
    pub const fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    /// Adds a thread to the back of the queue (normal scheduling).
    #[inline]
    pub fn insert(&mut self, thread: KArc<T, T::Allocator>) {
        self.queue.push_back(thread);
    }

    /// Removes and returns the thread at the front of the queue, if any.
    #[inline]
    pub fn pop(&mut self) -> Option<KArc<T, T::Allocator>> {
        self.queue.pop_front()
    }

    /// Returns a reference to the front thread without removing it.
    #[inline]
    pub fn peek(&self) -> Option<&KArc<T, T::Allocator>> {
        self.queue.front()
    }

    /// Adds a thread to the front of the queue (priority wakeup).
    ///
    /// Used by interrupt handlers to preempt the current thread.
    #[inline]
    pub fn push_front(&mut self, thread: KArc<T, T::Allocator>) {
        self.queue.push_front(thread);
    }

    /// Adds multiple threads to the front of the queue.
    ///
    /// Threads are inserted in reverse order so the first in the iterator
    /// ends up at the front.
    #[inline]
    pub fn extend(&mut self, other: impl IntoIterator<Item = KArc<T, T::Allocator>>) {
        for thread in other {
            self.push_front(thread);
        }
    }
}

impl<T: Thread> BlockedQueue<T> {
    pub const fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    pub fn wake_one(&mut self) -> Option<ThreadKArc<T>> {
        self.queue.pop_front()
    }

    pub fn insert(&mut self, thread: ThreadKArc<T>) {
        self.queue.push_back(thread);
    }
}

impl<T: Debug + Thread> Debug for RunQueue<T> {
    #[inline]
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RunQueue")
            .field("queue", &self.queue)
            .finish()
    }
}

#[derive(Debug)]
pub(crate) struct ExpiredIterator<'a, T: Thread> {
    queue: &'a mut BinaryHeap<SleepEntry<T>>,
    now: Instant,
}

impl<'a, T: Thread> Iterator for ExpiredIterator<'a, T> {
    type Item = SleepEntry<T>;

    #[inline]
    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.queue.peek()?;
        if entry.deadline <= self.now {
            self.queue.pop()
        } else {
            None
        }
    }
}
