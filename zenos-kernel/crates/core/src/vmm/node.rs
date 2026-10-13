use kernel_macros::allocator;
use kprimitives::alloc::{CreatableKernelObject, KernelObject, boxed::KBox};

use crate::arch::{PhysAddr, VirtAddr};

#[derive(Debug)]
pub(super) struct VmmNode {
    pub(super) virtaddr: VirtAddr,
    pub(super) physaddr: PhysAddr,
    pub(super) size: usize,
    pub(super) next: Option<KBox<VmmNode, VmmNodeAllocator>>,
}

impl KernelObject for VmmNode {}
impl CreatableKernelObject for VmmNode {
    type Allocator = VmmNodeAllocator;
}

#[allocator(type = VmmNode)]
pub(super) struct VmmNodeAllocator;
