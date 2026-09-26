use core::fmt::Debug;

use kprimitives::{
    alloc::{CreatableKernelObject, RefCounted, sync::KArc},
    time::Instant,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(transparent)]
pub struct Tid(u64);

impl Tid {
    pub const fn new(value: u64) -> Self {
        Self(value)
    }

    pub const fn get(self) -> u64 {
        self.0
    }
}

pub trait Thread: RefCounted + CreatableKernelObject {
    fn tid(&self) -> Tid;

    fn state(&self) -> ThreadState;
    fn set_state(&self, state: ThreadState);

    #[cfg(debug_assertions)]
    fn owner(&self) -> Option<&'static str> {
        None
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum ThreadState {
    Running = 0,
    Ready = 1,
    Sleeping = 2,
}

#[derive(Clone)]
pub(crate) struct SleepEntry<T: Thread> {
    pub deadline: Instant,
    pub thread: KArc<T, T::Allocator>,
    pub seq: u64,
}

impl<T: Thread> Eq for SleepEntry<T> {}
impl<T: Thread> PartialEq for SleepEntry<T> {
    fn eq(&self, other: &Self) -> bool {
        self.cmp(other).is_eq()
    }
}

impl<T: Thread> PartialOrd for SleepEntry<T> {
    fn partial_cmp(&self, other: &Self) -> Option<core::cmp::Ordering> {
        self.deadline
            .partial_cmp(&other.deadline)
            .and_then(|c| Some(c.reverse().then(self.seq.cmp(&other.seq).reverse())))
    }
}

impl<T: Thread> Ord for SleepEntry<T> {
    fn cmp(&self, other: &Self) -> core::cmp::Ordering {
        self.partial_cmp(other).unwrap()
    }
}

impl<T> Debug for SleepEntry<T>
where
    T: Thread + Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("SleepEntry")
            .field("deadline", &self.deadline)
            .field("thread", &self.thread)
            .finish()
    }
}
