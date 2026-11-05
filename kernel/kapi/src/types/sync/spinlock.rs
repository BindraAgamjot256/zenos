use core::{
    cell::UnsafeCell,
    fmt::Debug,
    hint::spin_loop,
    marker::PhantomData,
    ops::{Deref, DerefMut},
    sync::atomic::{AtomicBool, Ordering},
};

/// Controls interrupt state while a spinlock is held.
pub trait Disabler {
    fn disable_int() -> usize;
    fn enable_int(flags: usize);
}

/// A disabler that leaves interrupt state unchanged.
pub struct NoopDisabler;

impl Disabler for NoopDisabler {
    fn disable_int() -> usize {
        0
    }

    fn enable_int(_: usize) {}
}

/// A disabler that disables interrupts while a spinlock is held.
pub struct InterruptDisabler;

impl Disabler for InterruptDisabler {
    fn disable_int() -> usize {
        crate::ffi::kernel_disable_interrupts()
    }

    fn enable_int(flags: usize) {
        crate::ffi::kernel_enable_interrupts(flags)
    }
}

/// A simple spinlock mutex for protecting shared data.
///
/// The selected disabler runs before acquisition and restores the previous
/// interrupt state when the guard is dropped.
#[derive(Default)]
pub struct SpinLock<T, D: Disabler = NoopDisabler> {
    data: UnsafeCell<T>,
    lock: AtomicBool,
    disabler: PhantomData<D>,
}

unsafe impl<T: Send, D: Disabler> Sync for SpinLock<T, D> {}

pub struct MutexGuard<'a, T, D: Disabler = NoopDisabler> {
    mutex: &'a SpinLock<T, D>,
    flags: usize,
}

impl<T, D: Disabler> SpinLock<T, D> {
    pub const fn new(data: T) -> Self {
        Self {
            data: UnsafeCell::new(data),
            lock: AtomicBool::new(false),
            disabler: PhantomData,
        }
    }

    pub fn lock(&self) -> MutexGuard<'_, T, D> {
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

    pub fn unlock(&self, flags: usize) {
        self.lock.store(false, Ordering::Release);
        D::enable_int(flags);
    }

    pub fn try_lock(&self) -> Option<MutexGuard<'_, T, D>> {
        let flags = D::disable_int();
        match self
            .lock
            .compare_exchange(false, true, Ordering::Acquire, Ordering::Relaxed)
        {
            Ok(_) => Some(MutexGuard { mutex: self, flags }),
            Err(_) => {
                D::enable_int(flags);
                None
            }
        }
    }

    pub fn is_locked(&self) -> bool {
        self.lock.load(Ordering::Relaxed)
    }

    pub fn _get_mut(&mut self) -> &mut T {
        unsafe { &mut *self.data.get() }
    }
}

impl<T, D: Disabler> Drop for MutexGuard<'_, T, D> {
    fn drop(&mut self) {
        self.mutex.unlock(self.flags);
    }
}

impl<T, D: Disabler> Deref for MutexGuard<'_, T, D> {
    type Target = T;
    fn deref(&self) -> &T {
        unsafe { &*self.mutex.data.get() }
    }
}

impl<T, D: Disabler> DerefMut for MutexGuard<'_, T, D> {
    fn deref_mut(&mut self) -> &mut T {
        unsafe { &mut *self.mutex.data.get() }
    }
}

impl<T, D: Disabler> MutexGuard<'_, T, D> {
    pub fn flags(&self) -> usize {
        self.flags
    }
}

impl<T: Debug, D: Disabler> Debug for MutexGuard<'_, T, D> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("MutexGuard")
            .field("data", self.deref())
            .finish()
    }
}

impl<T: Debug, D: Disabler> Debug for SpinLock<T, D> {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
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
