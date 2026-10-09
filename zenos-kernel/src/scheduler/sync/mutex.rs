use crate::{
    arch::SpinLock,
    scheduler::{SCHEDULER, thread::Thread},
};
#[cfg(feature = "__test_mutex")]
use core::sync::atomic::AtomicUsize;
use core::{
    cell::UnsafeCell,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering},
};

use scheduler::queue::BlockedQueue;

pub struct Mutex<T> {
    wait_queue: SpinLock<BlockedQueue<Thread>>,
    lock: AtomicBool,
    data: UnsafeCell<T>,

    #[cfg(feature = "__test_mutex")]
    test_queued: AtomicUsize,
}

impl<T> Mutex<T> {
    pub const fn new(data: T) -> Self {
        Self {
            wait_queue: SpinLock::new(BlockedQueue::new()),
            lock: AtomicBool::new(false),
            data: UnsafeCell::new(data),

            #[cfg(feature = "__test_mutex")]
            test_queued: AtomicUsize::new(0),
        }
    }

    #[cfg(feature = "__test_mutex")]
    pub fn test_queued_count(&self) -> usize {
        self.test_queued.load(Ordering::Acquire)
    }

    // ---------------------------------------------------------
    // Internal acquisition
    // ---------------------------------------------------------

    fn acquire(&self) {
        if self.try_acquire() {
            return;
        }

        loop {
            let mut queue = self.wait_queue.lock();

            // Recheck while holding the queue lock.
            if self.try_acquire() {
                return;
            }

            SCHEDULER.block_current_after(|curr| {
                queue.insert(curr);

                #[cfg(feature = "__test_mutex")]
                self.test_queued.fetch_add(1, Ordering::Release);

                drop(queue);
            });
        }
    }

    fn try_acquire(&self) -> bool {
        self.lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_ok()
    }

    fn release(&self) {
        let next_thread = {
            let mut queue = self.wait_queue.lock();

            self.lock.store(false, Ordering::Release);

            queue.wake_one()
        };

        if let Some(thread) = next_thread {
            SCHEDULER.wake(thread);
        }
    }

    // ---------------------------------------------------------
    // Normal RAII API
    // ---------------------------------------------------------

    pub fn lock(&self) -> MutexGuard<'_, T> {
        self.acquire();

        MutexGuard { lock: self }
    }

    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        if self.try_acquire() {
            Some(MutexGuard { lock: self })
        } else {
            None
        }
    }

    // ---------------------------------------------------------
    // Manual API for external interfaces such as uACPI
    //
    // Caller must pair every successful acquisition
    // with exactly one unlock.
    // ---------------------------------------------------------

    /// Acquire without returning a guard.
    ///
    /// # Safety
    /// The caller must ensure:
    /// - No recursive acquisition.
    /// - The mutex remains alive while held.
    /// - Only its owner releases it.
    /// - Exactly one release follows acquisition.
    /// - Protected data is accessed only while owned.
    pub unsafe fn lock_manual(&self) {
        self.acquire();
    }

    /// Attempt acquisition without blocking.
    ///
    /// # Safety
    /// If this returns true, the caller assumes all
    /// obligations documented for lock_manual().
    pub unsafe fn try_lock_manual(&self) -> bool {
        self.try_acquire()
    }

    /// Release a manually acquired mutex.
    ///
    /// # Safety
    /// The current thread must own this mutex through
    /// a successful manual acquisition, and that
    /// acquisition must not already have been released.
    pub unsafe fn unlock_manual(&self) {
        self.release();
    }
}

unsafe impl<T: Send> Sync for Mutex<T> {}
unsafe impl<T: Send> Send for Mutex<T> {}

pub struct MutexGuard<'a, T> {
    lock: &'a Mutex<T>,
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &Self::Target {
        unsafe { &*self.lock.data.get() }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        unsafe { &mut *self.lock.data.get() }
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.lock.release();
    }
}
