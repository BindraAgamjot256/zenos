use kprimitives::alloc::{CreatableKernelObject, KernelObject};

pub struct UacpiMutex {
    inner: kprimitives::mutex::Mutex<()>,
    _pad: usize,
}

impl UacpiMutex {
    pub fn new() -> Self {
        Self {
            inner: kprimitives::mutex::Mutex::new(()),
            _pad: 0,
        }
    }

    pub fn lock(&self) {
        if self.inner.is_locked() {
            return;
        }
        self.inner.lock();
    }

    pub fn try_lock(&self) -> bool {
        if self.inner.is_locked() {
            return true;
        }

        self.inner.try_lock().is_some()
    }

    pub fn unlock(&self) {
        self.inner.unlock();
    }
}

impl KernelObject for UacpiMutex {}
impl CreatableKernelObject for UacpiMutex {
    type Allocator = Allocator;
}

#[kernel_macros::allocator(type = UacpiMutex)]
pub struct Allocator;
