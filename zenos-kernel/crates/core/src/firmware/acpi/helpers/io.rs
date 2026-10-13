use crate::mm::GlobalAllocator;
use kprimitives::alloc::{CreatableKernelObject, KernelObject};

pub struct IoPort {
    pub base: u64,
    pub size: u64,
}
impl IoPort {
    pub fn new(base: u64, size: u64) -> Self {
        Self { base, size }
    }
}

impl KernelObject for IoPort {}
impl CreatableKernelObject for IoPort {
    type Allocator = GlobalAllocator;
}
