use crate::mm::GlobalAllocator;
use crate::scheduler::sync::Mutex;
use core::fmt;
use kprimitives::alloc::{CreatableKernelObject, KernelObject};

pub struct UacpiMutex {
    inner: Mutex<()>,
}

impl UacpiMutex {
    pub const fn new() -> Self {
        Self {
            inner: Mutex::new(()),
        }
    }

    /// Attempt to acquire the mutex without blocking.
    ///
    /// Returns true on success, false if already locked.
    pub fn try_lock(&self) -> bool {
        // SAFETY:
        // The uACPI integration is responsible for
        // balanced acquisition/release operations.
        unsafe { self.inner.try_lock_manual() }
    }

    /// Release the mutex.
    ///
    /// Must be called exactly once after a successful
    /// acquisition, by the owning thread.
    pub fn unlock(&self) {
        // SAFETY:
        // Requires the uACPI caller to follow the
        // mutex ownership contract.
        unsafe {
            self.inner.unlock_manual();
        }
    }
}

impl Default for UacpiMutex {
    fn default() -> Self {
        Self::new()
    }
}

impl fmt::Debug for UacpiMutex {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("UacpiMutex").finish_non_exhaustive()
    }
}

#[derive(Debug)]
pub struct UacpiSpinlock {
    inner: crate::arch::common::SpinLock<()>,
}

impl UacpiSpinlock {
    pub fn new() -> Self {
        Self {
            inner: kprimitives::spinlock::SpinLock::new(()),
        }
    }

    pub fn lock(&self) -> usize {
        let guard = self.inner.lock();
        let flags = guard.flags();
        core::mem::forget(guard);
        flags
    }

    pub fn unlock(&self, _flags: usize) {
        #[cfg(feature = "disable_int_on_lock")]
        self.inner.unlock(_flags);

        #[cfg(not(feature = "disable_int_on_lock"))]
        self.inner.unlock();
    }
}

impl KernelObject for UacpiMutex {}
impl CreatableKernelObject for UacpiMutex {
    type Allocator = GlobalAllocator;
}

impl KernelObject for UacpiSpinlock {}
impl CreatableKernelObject for UacpiSpinlock {
    type Allocator = GlobalAllocator;
}
