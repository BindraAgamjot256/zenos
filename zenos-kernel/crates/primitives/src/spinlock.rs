use core::cell::UnsafeCell;
use core::fmt::Debug;
use core::hint::spin_loop;
#[cfg(feature = "disable_int_on_lock")]
use core::marker::PhantomData;
use core::ops::{Deref, DerefMut};
use core::sync::atomic::{AtomicBool, Ordering};

/// A simple spinlock mutex for protecting shared data.
///
/// This mutex uses an atomic boolean flag and busy-waits until the lock is
/// acquired.
///
/// When `disable_int_on_lock` is enabled, interrupts are disabled before
/// attempting to acquire the lock and restored when the guard is dropped.
#[cfg(feature = "disable_int_on_lock")]
#[derive(Default)]
pub struct SpinLock<T, D: Disabler> {
    data: UnsafeCell<T>,
    lock: AtomicBool,
    disabler: PhantomData<D>,
}

/// A simple spinlock mutex for protecting shared data.
#[cfg(not(feature = "disable_int_on_lock"))]
#[derive(Default)]
pub struct SpinLock<T> {
    data: UnsafeCell<T>,
    lock: AtomicBool,
}

#[cfg(feature = "disable_int_on_lock")]
pub trait Disabler {
    fn disable_int() -> usize;
    fn enable_int(flags: usize);
}

#[cfg(feature = "disable_int_on_lock")]
unsafe impl<T: Send, D: Disabler> Sync for SpinLock<T, D> {}

#[cfg(not(feature = "disable_int_on_lock"))]
unsafe impl<T: Send> Sync for SpinLock<T> {}

/// A guard that grants temporary access to the protected data.
#[cfg(feature = "disable_int_on_lock")]
pub struct MutexGuard<'a, T, D: Disabler> {
    mutex: &'a SpinLock<T, D>,
    flags: usize,
}

/// A guard that grants temporary access to the protected data.
#[cfg(not(feature = "disable_int_on_lock"))]
pub struct MutexGuard<'a, T> {
    mutex: &'a SpinLock<T>,
}

//
// Constructors
//

#[cfg(feature = "disable_int_on_lock")]
impl<T, D: Disabler> SpinLock<T, D> {
    /// Creates a new mutex containing the provided value.
    pub const fn new(data: T) -> Self {
        Self {
            data: UnsafeCell::new(data),
            lock: AtomicBool::new(false),
            disabler: PhantomData,
        }
    }
}

#[cfg(not(feature = "disable_int_on_lock"))]
impl<T> SpinLock<T> {
    /// Creates a new mutex containing the provided value.
    pub const fn new(data: T) -> Self {
        Self {
            data: UnsafeCell::new(data),
            lock: AtomicBool::new(false),
        }
    }
}

//
// Lock implementation with interrupt disabling
//

#[cfg(feature = "disable_int_on_lock")]
impl<T, D: Disabler> SpinLock<T, D> {
    /// Acquires the mutex, spinning until it becomes available.
    pub fn lock(&self) -> MutexGuard<'_, T, D> {
        // Disable interrupts BEFORE acquiring the lock.
        //
        // Otherwise an interrupt could occur after we acquire the lock but
        // before interrupts are disabled. If the interrupt handler attempts
        // to acquire this same lock, it would spin forever.
        let flags = D::disable_int();

        while self
            .lock
            .compare_exchange_weak(false, true, Ordering::Acquire, Ordering::Relaxed)
            .is_err()
        {
            spin_loop();
        }

        MutexGuard { mutex: self, flags }
    }

    /// Unlocks the mutex and restores the previous interrupt state.
    ///
    /// Do not call this directly. Use `MutexGuard` instead.
    #[allow(dead_code)]
    pub fn unlock(&self, flags: usize) {
        self.lock.store(false, Ordering::Release);
        D::enable_int(flags);
    }

    /// Attempts to acquire the mutex without blocking.
    pub fn try_lock(&self) -> Option<MutexGuard<'_, T, D>> {
        let flags = D::disable_int();

        match self
            .lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        {
            Ok(_) => Some(MutexGuard { mutex: self, flags }),

            Err(_) => {
                // We failed to acquire the lock, so restore interrupts.
                D::enable_int(flags);
                None
            }
        }
    }

    /// Returns whether the mutex is currently locked.
    ///
    /// This is only a best-effort check and may race with another CPU.
    pub fn is_locked(&self) -> bool {
        self.lock.load(Ordering::Relaxed)
    }

    /// Returns a mutable reference to the inner data without locking.
    ///
    /// This is safe because `&mut self` guarantees exclusive access to
    /// the SpinLock.
    pub fn _get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.data.get() }
    }
}

//
// Lock implementation without interrupt disabling
//

#[cfg(not(feature = "disable_int_on_lock"))]
impl<T> SpinLock<T> {
    /// Acquires the mutex, spinning until it becomes available.
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

    /// Unlocks the mutex.
    ///
    /// Do not call this directly. Use `MutexGuard` instead.
    #[allow(dead_code)]
    pub fn unlock(&self) {
        self.lock.store(false, Ordering::Release);
    }

    /// Attempts to acquire the mutex without blocking.
    pub fn try_lock(&self) -> Option<MutexGuard<'_, T>> {
        self.lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
            .ok()
            .map(|_| MutexGuard { mutex: self })
    }

    /// Returns whether the mutex is currently locked.
    ///
    /// This is only a best-effort check and may race with another CPU.
    pub fn is_locked(&self) -> bool {
        self.lock.load(Ordering::Relaxed)
    }

    /// Returns a mutable reference to the inner data without locking.
    ///
    /// This is safe because `&mut self` guarantees exclusive access to
    /// the SpinLock.
    pub fn _get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.data.get() }
    }
}

//
// Guard implementation with interrupt disabling
//

#[cfg(feature = "disable_int_on_lock")]
impl<T, D: Disabler> Drop for MutexGuard<'_, T, D> {
    fn drop(&mut self) {
        self.mutex.unlock(self.flags);
    }
}

#[cfg(feature = "disable_int_on_lock")]
impl<T, D: Disabler> Deref for MutexGuard<'_, T, D> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

#[cfg(feature = "disable_int_on_lock")]
impl<T, D: Disabler> DerefMut for MutexGuard<'_, T, D> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }
}

#[cfg(feature = "disable_int_on_lock")]
impl<T, D: Disabler> MutexGuard<'_, T, D> {
    pub fn flags(&self) -> usize {
        self.flags
    }
}

//
// Guard implementation without interrupt disabling
//

#[cfg(not(feature = "disable_int_on_lock"))]
impl<T> Drop for MutexGuard<'_, T> {
    fn drop(&mut self) {
        self.mutex.unlock();
    }
}

#[cfg(not(feature = "disable_int_on_lock"))]
impl<T> Deref for MutexGuard<'_, T> {
    type Target = T;

    fn deref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

#[cfg(not(feature = "disable_int_on_lock"))]
impl<T> DerefMut for MutexGuard<'_, T> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }
}

#[cfg(not(feature = "disable_int_on_lock"))]
impl<T> MutexGuard<'_, T> {
    pub fn flags(&self) -> usize {
        u64::from_ne_bytes(*b"FUCK YOU") as usize
    }
}

//
// Debug
//

#[cfg(feature = "disable_int_on_lock")]
impl<T: Debug, D: Disabler> Debug for MutexGuard<'_, T, D> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MutexGuard")
            .field("data", self.deref())
            .finish()
    }
}

#[cfg(not(feature = "disable_int_on_lock"))]
impl<T: Debug> Debug for MutexGuard<'_, T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MutexGuard")
            .field("data", self.deref())
            .finish()
    }
}

#[cfg(feature = "disable_int_on_lock")]
impl<T: Debug, D: Disabler> Debug for SpinLock<T, D> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Use try_lock to avoid deadlock
        if let Some(guard) = self.try_lock() {
            f.debug_struct("SpinLock")
                .field("data", &*guard)
                .field("lock", &true)
                .finish()
        } else {
            f.debug_struct("SpinLock")
                .field("data", &"<locked>")
                .field("lock", &true)
                .finish()
        }
    }
}

#[cfg(not(feature = "disable_int_on_lock"))]
impl<T: Debug> Debug for SpinLock<T> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        // Use try_lock to avoid deadlock
        if let Some(guard) = self.try_lock() {
            f.debug_struct("SpinLock")
                .field("data", &*guard)
                .field("lock", &true)
                .finish()
        } else {
            f.debug_struct("SpinLock")
                .field("data", &"<locked>")
                .field("lock", &true)
                .finish()
        }
    }
}
