use crate::threads::{SleepEntry, Thread};
use alloc::{
    collections::{BinaryHeap, VecDeque},
    vec::Vec,
};
use core::fmt::Debug;
use kprimitives::{alloc::sync::KArc, time::Instant};

#[derive(Debug)]
pub(crate) struct SleepQueue<T: Thread> {
    queue: BinaryHeap<SleepEntry<T>>,
}

pub(crate) struct RunQueue<T: Thread> {
    queue: VecDeque<KArc<T, T::Allocator>>,
}

impl<T: Thread> SleepQueue<T> {
    pub const fn new() -> Self {
        Self {
            queue: BinaryHeap::new(),
        }
    }

    pub fn push(&mut self, entry: SleepEntry<T>) {
        self.queue.push(entry);
    }

    pub fn pop(&mut self) -> Option<SleepEntry<T>> {
        self.queue.pop()
    }

    pub fn peek(&self) -> Option<&SleepEntry<T>> {
        self.queue.peek()
    }

    pub fn pop_expired(&mut self, now: Instant) -> ExpiredIterator<'_, T> {
        ExpiredIterator {
            queue: &mut self.queue,
            now,
        }
    }
}

impl<T: Thread> RunQueue<T> {
    pub const fn new() -> Self {
        Self {
            queue: VecDeque::new(),
        }
    }

    pub fn insert(&mut self, thread: KArc<T, T::Allocator>) {
        self.queue.push_back(thread);
    }

    pub fn pop(&mut self) -> Option<KArc<T, T::Allocator>> {
        self.queue.pop_front()
    }

    pub fn peek(&self) -> Option<&KArc<T, T::Allocator>> {
        self.queue.front()
    }

    pub fn push_front(&mut self, thread: KArc<T, T::Allocator>) {
        self.queue.push_front(thread);
    }

    pub fn extend(&mut self, other: impl IntoIterator<Item = KArc<T, T::Allocator>>) {
        for thread in other {
            self.push_front(thread);
        }
    }
}

impl<T: Debug + Thread> Debug for RunQueue<T> {
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

    fn next(&mut self) -> Option<Self::Item> {
        let entry = self.queue.peek()?;
        if entry.deadline <= self.now {
            self.queue.pop()
        } else {
            None
        }
    }
}
