//! Zenos slab slab_allocator v0.0.sqrt(-1)-don't_you_dare_test_it_on_hardware. Yes... That's its full version. Don't judge

use crate::memory::{LARGE_ALLOC_BASE_ADDR, PageType, SLAB_BASE_ADDR, kalloc_page};
use core::cell::UnsafeCell;
use core::sync::atomic::{AtomicUsize, Ordering};
use core::{
    alloc::{GlobalAlloc, Layout},
    mem::size_of,
    ptr::NonNull,
};
use heapless::Vec;
use linked_list_allocator::LockedHeap;
use log::{error, trace, warn};
use spin::Mutex;
use x86_64::VirtAddr;

// ====== Macros ======
/// Convert *mut BlockMeta to *mut u8 (user data)
macro_rules! user_ptr_from_meta {
    ($meta_ptr:expr) => {
        unsafe { ($meta_ptr as *mut u8).add(core::mem::size_of::<BlockMeta>()) }
    };
}

/// Convert *mut u8 (user data) to *mut BlockMeta
macro_rules! meta_ptr_from_user {
    ($user_ptr:expr) => {
        unsafe { ($user_ptr as *mut BlockMeta).offset(-1) }
    };
}

// ====== Config ======
const PAGE_SIZE: usize = super::constants::PAGE_4K;
const MAX_SLAB_PAGES: usize = 10; // 40 KiB per slab.

const SLAB_SIZE_CLASSES: [usize; 9] = [8, 16, 32, 64, 128, 256, 512, 1024, 2048];

// Metadata for each block; forms a doubly linked free list
#[repr(C, align(16))]
#[derive(Debug, Copy, Clone, Default)]
pub struct BlockMeta {
    next: Option<NonNull<BlockMeta>>,
    prev: Option<NonNull<BlockMeta>>,
    used: bool,
}

impl BlockMeta {
    pub const fn new() -> Self {
        BlockMeta {
            next: None,
            prev: None,
            used: false,
        }
    }
}

// Metadata for the slab as a whole
#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
pub struct SlabMeta {
    size: usize,
    used: usize,
    total: usize,
    magic_number: u64,
}

impl SlabMeta {
    pub const fn new(size: usize, total: usize) -> Self {
        // We'll update this with the actual count later.
        SlabMeta {
            size,
            used: 0,
            total,
            magic_number: 0x1604_2010 << 1 | 1,
        }
    }

    pub fn is_full(&self) -> bool {
        self.used >= self.total
    }

    pub fn allocate(&mut self) -> Option<usize> {
        if self.is_full() {
            None
        } else {
            let idx = self.used;
            self.used += 1;
            Some(idx)
        }
    }

    pub fn deallocate(&mut self) {
        if self.used > 0 {
            self.used -= 1;
        }
        debug_assert!(
            self.used <= self.total,
            "Used blocks exceed total blocks in slab"
        );
    }
}
// The slab slab_allocator with a free list
#[derive(Debug, Clone)]
pub struct Slab {
    base_addr: usize,
    slab_size: usize,
    head: Option<NonNull<BlockMeta>>,
}

impl Slab {
    /// Initialize a new slab at base_addr, with blocks of slab_size bytes and meta info.
    pub fn new(base_addr: usize, slab_size: usize, mut meta: SlabMeta) -> Self {
        // Write slab metadata at start
        let meta_ptr = base_addr as *mut SlabMeta;
        unsafe {
            *meta_ptr = meta;
            trace!(
                "Wrote slab metadata at 0x{:x}, total blocks: {}",
                base_addr, meta.total
            );
        }

        // Calculate usable space (after accounting for SlabMeta)
        let usable_space = MAX_SLAB_PAGES * PAGE_SIZE - size_of::<SlabMeta>();
        let actual_blocks = usable_space / (size_of::<BlockMeta>() + slab_size); // Include BlockMeta size
        trace!(
            "Calculated {actual_blocks} usable blocks of size {slab_size} bytes each for slab at 0x{base_addr:x}",
        );

        if actual_blocks < meta.total {
            trace!(
                "Adjusting block count from {} to {} due to space constraints",
                meta.total, actual_blocks
            );
            meta.total = actual_blocks;
            unsafe {
                *meta_ptr = meta; // Update metadata
            }
        }

        // Build free list for all blocks
        let mut head: Option<NonNull<BlockMeta>> = None;
        let mut prev: Option<NonNull<BlockMeta>> = None;
        unsafe {
            for i in 0..actual_blocks {
                let block_addr =
                    base_addr + size_of::<SlabMeta>() + i * (size_of::<BlockMeta>() + slab_size);
                /*
                trace!(
                    "Processing block {}/{} at 0x{:x}",
                    i + 1,
                    actual_blocks,
                    block_addr
                ); // only uncomment this if you need extreme debugging...
                */

                let bm = block_addr as *mut BlockMeta;
                *bm = BlockMeta::new();

                let mut nn = NonNull::new_unchecked(bm);
                if let Some(mut p) = prev {
                    p.as_mut().next = Some(nn);
                    nn.as_mut().prev = Some(p);
                } else {
                    head = Some(nn);
                }
                prev = Some(nn);
            }
        }

        Self {
            base_addr,
            slab_size,
            head,
        }
    }

    /// Pop a block from the free list and return its user-space pointer
    pub fn alloc(&mut self) -> *mut u8 {
        let slab_meta = unsafe { &mut *(self.base_addr as *mut SlabMeta) };

        if slab_meta.magic_number != 0x1604_2010 << 1 | 1 {
            warn!("Slab magic number mismatch, alloc aborted");
            return core::ptr::null_mut(); // nothing to do
        }
        if slab_meta.is_full() {
            warn!("Slab is full, alloc aborted");
            return core::ptr::null_mut(); // nothing to do
        }

        // take first free block
        let bm_ptr = match self.head {
            None => return core::ptr::null_mut(),
            Some(nn) => {
                // update head
                let next = unsafe { nn.as_ref().next };
                self.head = next;
                if let Some(mut nxt) = next {
                    unsafe {
                        nxt.as_mut().prev = None;
                    }
                }
                let err = slab_meta.allocate();
                if err.is_none() {
                    warn!("Error mapping block");
                }
                trace!("Allocated block, used count now: {}", slab_meta.used);
                nn
            }
        };

        unsafe {
            (*bm_ptr.as_ptr()).used = true;
        }

        // user pointer is after metadata
        user_ptr_from_meta!(bm_ptr.as_ptr())
    }

    /// Return a block to the free list given its user pointer
    pub fn dealloc(&mut self, ptr: *mut u8) {
        let slab_meta = unsafe { &mut *(self.base_addr as *mut SlabMeta) };
        if slab_meta.magic_number != 0x1604_2010 << 1 | 1 {
            warn!("Slab magic number mismatch, dealloc aborted");
            return; // nothing to do
        }
        if slab_meta.used == 0 {
            warn!("Slab is already empty, dealloc aborted");
            return; // nothing to do
        }

        if ptr.is_null() {
            warn!("Dealloc called with null pointer, ignoring");
            return; // nothing to do
        }

        let bm = meta_ptr_from_user!(ptr);
        let mut nn = NonNull::new(bm).unwrap();

        // insert at head
        unsafe {
            nn.as_mut().next = self.head;
        }
        unsafe {
            nn.as_mut().prev = None;
        }
        unsafe {
            nn.as_mut().used = false;
        }
        if let Some(mut old_head) = self.head {
            unsafe {
                old_head.as_mut().prev = Some(nn);
            }
        }
        slab_meta.deallocate();
        self.head = Some(nn);
    }

    #[cfg(debug_assertions)]
    /// Dump detailed metadata about this slab using serial printing.
    /// Reduced to a no-op to avoid excessive logging.
    pub fn dump_metadata(&self) {}
}

// ====== Allocator ======

#[derive(Default)]
pub struct SlabAllocator {
    slabs: Vec<Mutex<Slab>, 9>,
    base_addr: AtomicUsize,
}
impl SlabAllocator {
    pub const fn new() -> Self {
        SlabAllocator {
            slabs: Vec::new(),
            base_addr: AtomicUsize::new(SLAB_BASE_ADDR as usize),
        }
    }

    /// Initialize the slab allocator with a set of slabs
    pub fn init(&mut self) {
        for (i, &size) in SLAB_SIZE_CLASSES.iter().enumerate() {
            trace!(
                "Initializing slab {}/{} with size {} bytes",
                i + 1,
                SLAB_SIZE_CLASSES.len(),
                size
            );

            // Calculate new base address BEFORE allocating pages
            let current_base = self.base_addr.load(Ordering::Acquire);

            // Log transition between slabs clearly
            if i > 0 {
                trace!("Moving to new slab region at 0x{current_base:x}");
            }

            let slab_meta = SlabMeta::new(size, MAX_SLAB_PAGES * PAGE_SIZE / size);

            // Allocate pages for this slab with explicit flags
            for j in 0..MAX_SLAB_PAGES {
                let page_addr = current_base + (j * PAGE_SIZE);
                trace!(
                    "Allocating page {} at address 0x{:x} for slab {}",
                    j + 1,
                    page_addr,
                    i + 1
                );

                // Make sure your kalloc_page function sets proper write permissions
                match kalloc_page(VirtAddr::new(page_addr as u64), PageType::Arbitrary) {
                    Ok(_) => {
                        // Page allocated successfully - we'll verify it during slab creation
                        trace!("Page allocated successfully at 0x{page_addr:x}");
                    }
                    Err(e) => {
                        error!("Failed to allocate page at 0x{page_addr:x}: {e:#?}");
                        return;
                    }
                }
            }

            trace!("Creating slab at base address 0x{current_base:x}");
            let slab = Slab::new(current_base, size, slab_meta);

            match self.slabs.push(Mutex::new(slab)) {
                Ok(_) => trace!("Slab {} added to collection", i + 1),
                Err(_) => {
                    error!("Failed to add slab to collection");
                    return;
                }
            }

            // Update base_addr for next slab
            self.base_addr
                .store(current_base + MAX_SLAB_PAGES * PAGE_SIZE, Ordering::Release);
            trace!(
                "Slab {} initialized, next base address: 0x{:x}",
                i + 1,
                self.base_addr.load(Ordering::Relaxed)
            );
        }
    }
}

impl SlabAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();
        trace!("Allocating {size} bytes with alignment {align}");

        if size == 0 {
            return core::ptr::null_mut(); // nothing to do
        }

        if size > *SLAB_SIZE_CLASSES.last().unwrap() {
            warn!(
                "Slab allocator does not support allocations larger than {} bytes, alloc aborted",
                SLAB_SIZE_CLASSES.last().unwrap()
            );
            return core::ptr::null_mut(); // nothing to do
        }
        // Find the appropriate slab size class
        let slab_size = match SLAB_SIZE_CLASSES.iter().find(|&&s| s >= size && s >= align) {
            Some(&s) => s,
            None => {
                warn!("No suitable slab for size {size} and alignment {align}");
                return core::ptr::null_mut();
            }
        };

        // Find a slab that can accommodate this size
        trace!("Finding slab for size {slab_size} bytes");
        let slab = self.slabs.iter().find(|s| s.lock().slab_size == slab_size);
        if let Some(mtx) = slab {
            let mut slab = mtx.lock();
            // Allocate from the slab
            let ptr = slab.alloc();
            if ptr.is_null() {
                warn!("Slab allocation failed for size {size} bytes");
                return core::ptr::null_mut(); // nothing to do
            }
            trace!("Allocated {size} bytes at address {ptr:p}");
            trace!("Slab state after allocation: {slab:#?}");

            #[cfg(debug_assertions)]
            slab.dump_metadata(); // Dump metadata for debugging
            ptr
        } else {
            error!("No slab found for size {slab_size} bytes");
            core::ptr::null_mut() // nothing to do
        }
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            warn!("Dealloc called with null pointer, ignoring");
            return;
        }

        let size = layout.size();
        trace!("Deallocating {size} bytes at address {ptr:p}");

        // Find the appropriate slab size class
        let slab_size = match SLAB_SIZE_CLASSES.iter().find(|&&s| s >= size) {
            Some(&s) => s,
            None => {
                warn!("No suitable slab size class found for {size} bytes, dealloc aborted");
                return;
            }
        };

        // Find the slab with the right size
        if let Some(mtx) = self.slabs.iter().find(|s| s.lock().slab_size == slab_size) {
            let mut slab = mtx.lock();
            trace!("Deallocating pointer {ptr:p} in slab with block size {slab_size}",);
            slab.dealloc(ptr);

            #[cfg(debug_assertions)]
            slab.dump_metadata(); // Dump metadata for debugging
        } else {
            error!("No slab found for size {slab_size} bytes");
        }
    }
}

struct LockedAllocator {
    slab_allocator: UnsafeCell<SlabAllocator>,
    large_allocator: LockedHeap,
}
impl LockedAllocator {
    pub const fn new() -> Self {
        {
            LockedAllocator {
                slab_allocator: UnsafeCell::new(SlabAllocator::new()),
                large_allocator: LockedHeap::empty(),
            }
        }
    }
    pub fn init(&self) {
        unsafe {
            self.slab_allocator.as_mut_unchecked().init();
        }
    }
}

unsafe impl GlobalAlloc for LockedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        x86_64::instructions::interrupts::without_interrupts(|| {
            if layout.size() > *SLAB_SIZE_CLASSES.last().unwrap() {
                // redirect the chunky bois
                let ptr = self.large_allocator.lock().allocate_first_fit(layout);
                if ptr.is_err() {
                    return core::ptr::null_mut();
                }
                return ptr.unwrap().as_ptr();
            }

            let allocator = &self.slab_allocator;
            let ptr = allocator.as_mut_unchecked().alloc(layout);
            if ptr.is_null() {
                let ptr = self.large_allocator.lock().allocate_first_fit(layout);
                if ptr.is_err() {
                    return core::ptr::null_mut();
                }
                return ptr.unwrap().as_ptr();
            }
            ptr
        })
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        x86_64::instructions::interrupts::without_interrupts(|| {
            if layout.size() > *SLAB_SIZE_CLASSES.last().unwrap() {
                let ptr = NonNull::new(ptr);
                if ptr.is_none() {
                    return;
                }
                return self.large_allocator.lock().deallocate(ptr.unwrap(), layout);
            }

            let allocator = &self.slab_allocator;
            allocator.as_mut_unchecked().dealloc(ptr, layout);
        })
    }
}

unsafe impl Sync for LockedAllocator {}
unsafe impl Send for LockedAllocator {}

#[global_allocator]
static ALLOCATOR: LockedAllocator = LockedAllocator::new();

pub fn init() {
    trace!("Initializing slab slab_allocator");
    ALLOCATOR.init();
    // map pages for large allocator.
    let mut addr = LARGE_ALLOC_BASE_ADDR;
    let pages = (super::PAGE_2M * 5) / super::PAGE_4K;
    for _ in 0..pages {
        kalloc_page(VirtAddr::new(addr), PageType::Arbitrary).unwrap();
        addr += super::PAGE_4K as u64;
    }
    unsafe {
        ALLOCATOR
            .large_allocator
            .lock()
            .init(LARGE_ALLOC_BASE_ADDR as *mut u8, super::PAGE_2M * 5);
    }
    trace!("Slab allocator initialized with base address 0x{SLAB_BASE_ADDR:x}",);
}
