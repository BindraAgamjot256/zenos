use crate::mm::GlobalAllocator;
use core::sync::atomic::AtomicUsize;
use kprimitives::alloc::{CreatableKernelObject, KernelObject};

#[derive(Debug)]
pub struct UacpiMutex {
    inner: crate::arch::common::SpinLock<()>,
    flags: AtomicUsize,
}

// todo: actual mutex impl.
impl UacpiMutex {
    pub fn new() -> Self {
        Self {
            inner: crate::arch::common::SpinLock::new(()),
            flags: AtomicUsize::new(0),
        }
    }

    pub fn try_lock(&self) -> Option<usize> {
        let Some(guard) = self.inner.try_lock() else {
            return None;
        };

        let flags = guard.flags();
        self.flags
            .store(flags, core::sync::atomic::Ordering::Relaxed);

        core::mem::forget(guard);

        Some(flags)
    }

    pub fn unlock(&self) {
        #[cfg(feature = "disable_int_on_lock")]
        self.inner
            .unlock(self.flags.load(core::sync::atomic::Ordering::Relaxed));

        #[cfg(not(feature = "disable_int_on_lock"))]
        self.inner.unlock();
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

    pub fn unlock(&self, flags: usize) {
        #[cfg(feature = "disable_int_on_lock")]
        self.inner.unlock(flags);

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
