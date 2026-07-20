use core::cell::UnsafeCell;
use core::hint::spin_loop;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

/// A simple spinlock mutex for protecting shared data.
///
/// This mutex uses an atomic boolean flag and busy-waits until the lock is acquired.
/// It is suitable for low-level kernel contexts where blocking is not available.s
#[derive(Debug, Default)]
pub struct Mutex<T> {
    data: UnsafeCell<T>,
    lock: AtomicBool,
}

// Safety:
// - We only give out &T / &mut T when the lock is held.
// - T must be Send to safely move the mutex across threads.
unsafe impl<T: Send> Sync for Mutex<T> {}

/// A guard that grants temporary access to the protected data.
///
/// The lock is released when this guard is dropped.
pub struct MutexGuard<'a, T> {
    mutex: &'a Mutex<T>,
}

impl<T> Mutex<T> {
    /// Creates a new mutex containing the provided value.
    pub const fn new(data: T) -> Self {
        Self {
            data: UnsafeCell::new(data),
            lock: AtomicBool::new(false),
        }
    }

    /// Acquires the mutex, spinning until it becomes available.
    ///
    /// Returns a guard that dereferences to the protected value.
    pub fn lock(&self) -> MutexGuard<'_, T> {
        while self
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            spin_loop();
        }

        MutexGuard { mutex: self }
    }

    /// Attempts to acquire the mutex without blocking.
    ///
    /// Returns `Some(MutexGuard)` if the lock was acquired, or `None` if it was already held.
    pub fn _try_lock(&self) -> Option<MutexGuard<'_, T>> {
        self.lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| MutexGuard { mutex: self })
    }

    /// Returns whether the mutex is currently locked.
    ///
    /// This is a best-effort check and may race with other threads.
    pub fn _is_locked(&self) -> bool {
        self.lock.load(Ordering::Relaxed)
    }

    /// Returns a mutable reference to the inner data without locking.
    ///
    /// This is only safe when no other references to the mutex exist.
    pub fn _get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.data.get() }
    }
}

impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        // Release ordering ensures all writes to `T` happen-before unlock
        self.mutex.lock.store(false, Ordering::Release);
    }
}

impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }
}
