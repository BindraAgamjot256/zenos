use core::fmt::Debug;

use kprimitives::{
    alloc::{CreatableKernelObject, RefCounted, sync::KArc},
    time::Instant,
};

/// Unique thread identifier.
///
/// A simple wrapper around `u64` providing type safety for thread IDs.
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

/// Trait that all kernel thread types must implement.
///
/// This trait defines the interface between the scheduler and thread implementations.
/// Threads must be reference-counted ([RefCounted]) and creatable ([CreatableKernelObject])
/// to be managed by the scheduler.
pub trait Thread: RefCounted + CreatableKernelObject {
    /// Returns the thread's unique identifier.
    fn tid(&self) -> Tid;

    /// Returns the current scheduler state of this thread.
    fn state(&self) -> ThreadState;

    /// Sets the scheduler state of this thread.
    fn set_state(&self, state: ThreadState);

    /// Optional debug field: the subsystem that owns this thread.
    ///
    /// Only available in debug builds. Returns `None` by default.
    #[cfg(debug_assertions)]
    fn owner(&self) -> Option<&'static str> {
        None
    }
}

/// Execution state of a thread.
///
/// Used by the scheduler to track thread lifecycle.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
#[repr(u8)]
pub enum ThreadState {
    /// Currently executing on the CPU.
    Running = 0,
    /// Ready to run, waiting in the run queue.
    Ready = 1,
    /// Blocked on a timer deadline.
    Sleeping = 2,
    /// Blocked on a synchronization primitive (not yet used).
    Blocked = 3,
}

/// Internal entry for the sleep queue.
///
/// Wraps a thread with its wakeup deadline and a sequence number for FIFO ordering
/// of threads with equal deadlines.
#[derive(Clone)]
pub(crate) struct SleepEntry<T: Thread> {
    /// When this thread should be woken up.
    pub deadline: Instant,
    /// The sleeping thread.
    pub thread: KArc<T, T::Allocator>,
    /// Sequence number for tiebreaking equal deadlines.
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
