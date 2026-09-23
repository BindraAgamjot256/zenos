use crate::mm::GlobalAllocator;
use core::sync::atomic::AtomicUsize;
use kprimitives::alloc::{CreatableKernelObject, KernelObject};

pub struct UacpiEvent {
    counter: AtomicUsize,
}

impl KernelObject for UacpiEvent {}
impl CreatableKernelObject for UacpiEvent {
    type Allocator = GlobalAllocator;
}

impl UacpiEvent {
    pub fn new() -> Self {
        Self {
            counter: AtomicUsize::new(0),
        }
    }

    pub fn increment(&self) {
        self.counter
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    }

    pub fn decrement(&self) {
        self.counter
            .fetch_sub(1, core::sync::atomic::Ordering::Relaxed);
    }
}
