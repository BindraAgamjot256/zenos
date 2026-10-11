use crate::mm::GlobalAllocator;
use core::sync::atomic::AtomicUsize;
use kprimitives::alloc::{CreatableKernelObject, KernelObject};

pub struct UacpiEvent {
    _counter: AtomicUsize,
}

impl KernelObject for UacpiEvent {}
impl CreatableKernelObject for UacpiEvent {
    type Allocator = GlobalAllocator;
}

impl UacpiEvent {
    pub fn new() -> Self {
        Self {
            _counter: AtomicUsize::new(0),
        }
    }

    pub fn _increment(&self) {
        self._counter
            .fetch_add(1, core::sync::atomic::Ordering::Relaxed);
    }

    pub fn _decrement(&self) {
        self._counter
            .fetch_sub(1, core::sync::atomic::Ordering::Relaxed);
    }
}
