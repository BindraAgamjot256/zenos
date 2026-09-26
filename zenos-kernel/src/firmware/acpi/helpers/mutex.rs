use kprimitives::alloc::{CreatableKernelObject, KernelObject};

use crate::mm::GlobalAllocator;

#[derive(Debug)]
pub struct UacpiMutex {
    inner: kprimitives::spinlock::SpinLock<()>,
}

// todo: use kernel::mutex
impl UacpiMutex {
    pub fn new() -> Self {
        Self {
            inner: kprimitives::spinlock::SpinLock::new(()),
        }
    }

    pub fn try_lock(&self) -> bool {
        let Some(guard) = self.inner.try_lock() else {
            return false;
        };

        core::mem::forget(guard);
        true
    }

    pub fn unlock(&self) {
        self.inner.unlock();
    }
}

#[derive(Debug)]
pub struct UacpiSpinlock {
    inner: kprimitives::spinlock::SpinLock<()>,
}

impl UacpiSpinlock {
    pub fn new() -> Self {
        Self {
            inner: kprimitives::spinlock::SpinLock::new(()),
        }
    }

    pub fn lock(&self) {
        core::mem::forget(self.inner.lock());
    }

    pub fn unlock(&self) {
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
