pub use crate::mm::alloc::GlobalAllocator;
pub use crate::mm::alloc::SlabBackend;
pub use crate::mm::buddy::BUDDY_ALLOCATOR;
use bitflags::bitflags;
use core::{fmt::Debug, ptr::NonNull, sync::atomic::AtomicU32};
use kmm::buddy::MAX_ORDER;
use kmm::slab::Metadata as SlabMeta;

mod alloc;
pub mod buddy;

bitflags! {
    /// Flags for the state of a page.
    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    pub struct PageFlags: u16 {
        /// This page is currently free. No other flags should be set.
        const FREE = 1 << 15;

        /// This page is the head of a buddy allocation.
        const BUDDY_HEAD = 1 << 1;

        /// Reserved by firmware or the kernel.
        const RESERVED = 1 << 2;

        /// Managed by the slab allocator.
        const SLAB = 1 << 3;

        /// Kernel-owned page.
        const KERNEL = 1 << 4;
    }
}

#[repr(C)]
pub struct Page {
    /// Generic page state.
    pub flags: PageFlags,

    /// Number of active references to this page.
    pub refcount: AtomicU32,

    /// Metadata specific to the page's current use.
    pub meta: PageMeta,
}

impl Debug for Page {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Page")
            .field("flags", &self.flags)
            .field("refcount", &self.refcount)
            .field("meta", &"<PRIVATE>")
            .finish()
    }
}

impl Page {
    pub const fn null() -> Self {
        Self {
            flags: PageFlags::RESERVED,
            refcount: AtomicU32::new(0),
            meta: PageMeta {
                null: NullMeta { _null: () },
            },
        }
    }

    pub fn mark_usable(&mut self) {
        self.flags.insert(PageFlags::FREE);
    }

    pub fn is_usable(&self) -> bool {
        self.flags.contains(PageFlags::FREE)
    }

    pub fn pfn(&self) -> usize {
        unsafe { (self as *const Self).offset_from(crate::arch::mem::memmap_addr()) as usize }
    }

    pub fn from_pfn(pfn: usize) -> NonNull<Self> {
        let pfn = unsafe { crate::arch::mem::memmap_addr::<Self>().add(pfn) };
        NonNull::new(pfn).unwrap()
    }

    pub fn buddy_order(&self) -> u8 {
        unsafe { self.meta.buddy.order }
    }

    pub fn set_buddy_order(&mut self, order: u8) {
        self.meta.buddy.order = order;
    }

    pub fn clear_meta(&mut self) {
        self.meta.null = NullMeta { _null: () };
    }

    pub fn clear_buddy_state(&mut self) {
        self.flags.remove(PageFlags::BUDDY_HEAD);
        self.clear_meta();
    }

    pub fn mark_allocated(&mut self) {
        self.flags.remove(PageFlags::FREE);
        self.clear_buddy_state();
    }

    pub fn mark_slab(&mut self, meta: kmm::slab::Metadata) {
        self.flags.insert(PageFlags::SLAB);
        self.meta.slab = meta;
    }

    pub fn is_slab(&self) -> bool {
        self.flags.contains(PageFlags::SLAB)
    }
}

#[repr(C)]
pub union PageMeta {
    pub buddy: BuddyMeta,
    pub null: NullMeta,
    pub slab: SlabMeta,
}

#[repr(C)]
#[derive(Clone, Copy)]
/// Only valid if `BUDDY_HEAD` is set.
pub struct BuddyMeta {
    /// Order of the buddy block.
    pub order: u8,
}

#[derive(Clone, Copy)]
pub struct NullMeta {
    _null: (),
}

pub(crate) fn init(slice: &mut [Page], usable_iter: impl Iterator<Item = (usize, usize)>) {
    for (start, end) in usable_iter {
        let mut current = start;

        while current < end {
            let remaining = end - current;

            // Largest order that fits in the remaining range.
            let size_order = remaining.ilog2() as usize;

            // Largest order that is aligned at `current`.
            let alignment_order = if current == 0 {
                MAX_ORDER
            } else {
                current.trailing_zeros() as usize
            };

            // The block must satisfy both constraints.
            let order = size_order.min(alignment_order).min(MAX_ORDER);

            let block_size = 1usize << order;

            let raw = &raw mut slice[current];
            BUDDY_ALLOCATOR
                .insert_block(NonNull::new(raw).unwrap(), order as u8)
                .unwrap();

            current += block_size;
        }
    }
}
