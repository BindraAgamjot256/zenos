use crate::arch::PAGE_SIZE;
use crate::mm::{Page, PageFlags};
use core::ptr::NonNull;
use kmm::buddy::RawBuddyAllocator;
use kmm::buddy::{BuddyBackend, PageFrameNum};
use kprimitives::mutex::Mutex;

/// Buddy allocator managing free physical memory.
///
/// Memory is organized into power-of-two sized blocks ("orders").
///
/// Each order maintains its own singly-linked free list containing the
/// head page of every free buddy block of that size.
///
/// Allocation searches for the smallest suitable block. If only larger
/// blocks exist, they are repeatedly split until the requested order is
/// reached.
///
/// Freeing performs the reverse operation, repeatedly merging buddies
/// whenever both halves are free.
pub struct BuddyAllocator {
    raw: Mutex<RawBuddyAllocator<Backend>>,
}

struct Backend;

impl BuddyBackend for Backend {
    fn ptr_to_pfn(&self, ptr: *mut u8) -> kmm::buddy::PageFrameNum {
        let addr = ptr.addr() - crate::arch::get_phys_offset();
        PageFrameNum::new(addr >> PAGE_SIZE.ilog2())
    }

    fn pfn_to_ptr(&self, pfn: kmm::buddy::PageFrameNum) -> *mut u8 {
        let addr = (pfn.number() << PAGE_SIZE.ilog2()) + crate::arch::get_phys_offset();
        addr as *mut u8
    }

    fn is_free(&self, pfn: kmm::buddy::PageFrameNum) -> bool {
        let page = Page::from_pfn(pfn.number());
        unsafe { page.as_ref().is_usable() }
    }

    fn mark_free(&mut self, pfn: kmm::buddy::PageFrameNum) {
        let mut page = Page::from_pfn(pfn.number());
        unsafe {
            page.as_mut().mark_usable();
            page.as_mut().flags.insert(PageFlags::BUDDY_HEAD);
        }
    }

    fn mark_allocated(&mut self, pfn: kmm::buddy::PageFrameNum) {
        let mut page = Page::from_pfn(pfn.number());
        unsafe { page.as_mut().mark_allocated() }
    }

    fn get_order(&self, pfn: kmm::buddy::PageFrameNum) -> usize {
        let page = Page::from_pfn(pfn.number());
        unsafe { page.as_ref().buddy_order() as usize }
    }

    fn set_order(&mut self, pfn: kmm::buddy::PageFrameNum, order: usize) {
        let mut page = Page::from_pfn(pfn.number());
        unsafe { page.as_mut().set_buddy_order(order as u8) }
    }
}

impl BuddyAllocator {
    /// Creates an empty buddy allocator.
    ///
    /// All free lists begin empty. Memory is added later by inserting
    /// buddy blocks with [`insert_block`].
    pub const fn new() -> Self {
        Self {
            raw: Mutex::new(RawBuddyAllocator::new(Backend)),
        }
    }

    /// Inserts a free buddy block into the appropriate free list.
    ///
    /// The page must not already represent the head of a valid buddy block.
    /// This function does not construct the buddy block; it merely links
    /// it into the allocator's free list for its stored order.
    ///
    /// Returns `None` if:
    /// - the page is marked as a buddy head
    /// - the stored buddy order is invalid
    pub fn insert_block(&self, mut page: NonNull<Page>, order: u8) -> Option<()> {
        let mut raw = self.raw.lock();
        let page = unsafe { page.as_mut() };
        if page.flags.contains(PageFlags::BUDDY_HEAD) {
            return None;
        }
        page.flags.insert(PageFlags::BUDDY_HEAD);
        let pfn = PageFrameNum::new(page.pfn());
        raw.insert_block(pfn, order as usize);
        Some(())
    }

    /// Allocates one contiguous buddy block of the requested order.
    ///
    /// If no block of the exact order exists, progressively larger
    /// blocks are searched and repeatedly split until the requested
    /// order is reached.
    ///
    /// Returns `None` if no suitable block exists.
    pub fn alloc(&self, order: u8) -> Option<Mapping> {
        let mut raw = self.raw.lock();
        let pfn = raw.alloc(order as usize)?;
        let mapping = Mapping {
            start: pfn.number(),
            order: order as usize,
        };
        Some(mapping)
    }

    /// Frees a previously allocated buddy block.
    ///
    /// The block will eventually be returned to the buddy allocator and
    /// repeatedly merged with its free buddy whenever possible until no
    /// further merge is possible or the maximum order is reached.
    pub fn free(&self, mapping: Mapping) {
        let mut raw = self.raw.lock();
        let pfn = PageFrameNum::new(mapping.start);
        raw.free(pfn, mapping.order);
    }
}

/// Describes one contiguous physical allocation returned by the buddy
/// allocator.
///
/// The allocation spans `2^order` physical pages beginning at `start`.
#[derive(Debug, Clone)]
pub struct Mapping {
    start: usize,
    order: usize,
}

unsafe impl Send for BuddyAllocator {}
unsafe impl Sync for BuddyAllocator {}

/// Global kernel buddy allocator.
///
/// During early boot the allocator starts empty. Physical memory is
/// later organized into buddy blocks and inserted into the appropriate
/// free lists as memory initialization progresses.
pub static BUDDY_ALLOCATOR: BuddyAllocator = BuddyAllocator::new();
