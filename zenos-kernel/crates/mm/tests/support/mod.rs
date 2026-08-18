#![allow(dead_code)]

use core::cell::UnsafeCell;
use core::mem::size_of;
use core::ptr::NonNull;
use kmm::buddy::{BuddyBackend, PageFrameNum, RawBuddyAllocator};
use kmm::slab::{Backend as SlabBackend, Metadata, PAGE_SIZE};
use std::alloc::{Layout, alloc_zeroed, dealloc};
use std::cell::Cell;
use std::rc::Rc;
use std::vec::Vec;

const BUDDY_PAGE_WORDS: usize = 8;

pub struct HostBuddyBackend {
    memory: Vec<UnsafeCell<[usize; BUDDY_PAGE_WORDS]>>,
    free: Vec<bool>,
    orders: Vec<usize>,
}

impl HostBuddyBackend {
    pub fn new(page_count: usize) -> Self {
        let memory = (0..page_count)
            .map(|_| UnsafeCell::new([0; BUDDY_PAGE_WORDS]))
            .collect();

        Self {
            memory,
            free: vec![false; page_count],
            orders: vec![0; page_count],
        }
    }
}

impl BuddyBackend for HostBuddyBackend {
    fn ptr_to_pfn(&self, ptr: *mut u8) -> PageFrameNum {
        let base = self.memory.as_ptr().cast::<u8>();
        let page_size = size_of::<UnsafeCell<[usize; BUDDY_PAGE_WORDS]>>();
        let offset = unsafe { ptr.cast_const().offset_from(base) } as usize;

        PageFrameNum::new(offset / page_size)
    }

    fn pfn_to_ptr(&self, pfn: PageFrameNum) -> *mut u8 {
        self.memory[pfn.number()].get().cast::<u8>()
    }

    fn is_free(&self, pfn: PageFrameNum) -> bool {
        assert!(
            pfn.number() < self.free.len(),
            "{:?} > {}",
            pfn,
            self.free.len() - 1
        );
        self.free[pfn.number()]
    }

    fn mark_free(&mut self, pfn: PageFrameNum) {
        assert!(pfn.number() < self.free.len(), "{:?}", pfn);
        self.free[pfn.number()] = true;
    }

    fn mark_allocated(&mut self, pfn: PageFrameNum) {
        assert!(pfn.number() < self.free.len(), "{:?}", pfn);
        self.free[pfn.number()] = false;
    }

    fn get_order(&self, pfn: PageFrameNum) -> usize {
        assert!(pfn.number() < self.orders.len(), "{:?}", pfn);
        self.orders[pfn.number()]
    }

    fn set_order(&mut self, pfn: PageFrameNum, order: usize) {
        assert!(pfn.number() < self.orders.len(), "{:?}", pfn);
        self.orders[pfn.number()] = order;
    }
}

pub fn buddy_allocator(order: usize) -> RawBuddyAllocator<HostBuddyBackend> {
    let page_count = 1usize << order;
    // Keep metadata for the root block's out-of-range buddy. The allocator
    // probes that PFN before deciding that a non-MAX_ORDER root cannot merge.
    let mut allocator = RawBuddyAllocator::new(HostBuddyBackend::new(page_count * 2));
    allocator.insert_block(PageFrameNum::new(0), order);
    allocator
}

#[derive(Clone, Default)]
pub struct HostSlabStats {
    allocation_attempts: Rc<Cell<usize>>,
    page_allocations: Rc<Cell<usize>>,
    page_deallocations: Rc<Cell<usize>>,
    live_pages: Rc<Cell<usize>>,
}

impl HostSlabStats {
    pub fn allocation_attempts(&self) -> usize {
        self.allocation_attempts.get()
    }

    pub fn page_allocations(&self) -> usize {
        self.page_allocations.get()
    }

    pub fn page_deallocations(&self) -> usize {
        self.page_deallocations.get()
    }

    pub fn live_pages(&self) -> usize {
        self.live_pages.get()
    }
}

pub struct HostSlabBackend {
    pages: Vec<NonNull<u8>>,
    metadata: Vec<(NonNull<u8>, Metadata)>,
    max_live_pages: usize,
    stats: HostSlabStats,
}

impl HostSlabBackend {
    pub fn new(max_live_pages: usize) -> Self {
        Self::with_stats(max_live_pages, HostSlabStats::default())
    }

    pub fn with_stats(max_live_pages: usize, stats: HostSlabStats) -> Self {
        Self {
            pages: Vec::new(),
            metadata: Vec::new(),
            max_live_pages,
            stats,
        }
    }

    fn page_layout() -> Layout {
        Layout::from_size_align(PAGE_SIZE, PAGE_SIZE).expect("valid slab page layout")
    }
}

impl SlabBackend for HostSlabBackend {
    unsafe fn allocate(&mut self) -> Option<NonNull<u8>> {
        self.stats
            .allocation_attempts
            .set(self.stats.allocation_attempts.get() + 1);
        if self.pages.len() >= self.max_live_pages {
            return None;
        }

        let page = NonNull::new(unsafe { alloc_zeroed(Self::page_layout()) })?;
        self.pages.push(page);
        self.stats
            .page_allocations
            .set(self.stats.page_allocations.get() + 1);
        self.stats.live_pages.set(self.pages.len());
        Some(page)
    }

    unsafe fn deallocate(&mut self, page: NonNull<u8>) {
        let index = self
            .pages
            .iter()
            .position(|candidate| *candidate == page)
            .expect("slab backend received an unknown page");

        self.pages.swap_remove(index);
        self.metadata.retain(|(candidate, _)| *candidate != page);
        self.stats
            .page_deallocations
            .set(self.stats.page_deallocations.get() + 1);
        self.stats.live_pages.set(self.pages.len());
        unsafe { dealloc(page.as_ptr(), Self::page_layout()) };
    }

    fn set_metadata(&mut self, metadata: Metadata, page: NonNull<u8>) {
        if let Some((_, current)) = self
            .metadata
            .iter_mut()
            .find(|(candidate, _)| *candidate == page)
        {
            *current = metadata;
        } else {
            self.metadata.push((page, metadata));
        }
    }

    fn get_metadata(&self, page: NonNull<u8>) -> Option<Metadata> {
        self.metadata
            .iter()
            .find_map(|(candidate, metadata)| (*candidate == page).then_some(*metadata))
    }
}

impl Drop for HostSlabBackend {
    fn drop(&mut self) {
        for page in self.pages.drain(..) {
            unsafe { dealloc(page.as_ptr(), Self::page_layout()) };
        }
        self.stats.live_pages.set(0);
    }
}
