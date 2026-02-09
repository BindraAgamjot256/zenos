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
macro_rules! user_ptr_from_meta {
    ($meta_ptr:expr) => {
        unsafe { ($meta_ptr as *mut u8).add(core::mem::size_of::<BlockMeta>()) }
    };
}

macro_rules! meta_ptr_from_user {
    ($user_ptr:expr) => {
        unsafe { ($user_ptr as *mut BlockMeta).offset(-1) }
    };
}

// ====== Config ======
const PAGE_SIZE: usize = super::constants::PAGE_4K;
const MAX_SLAB_PAGES: usize = 10; // 40 KiB per slab.
const SLAB_SIZE_CLASSES: [usize; 10] = [8, 16, 32, 64, 128, 256, 512, 1024, 2048, 4096];
const MAX_SLABS: usize = 32; // Limit total slabs to prevent heapless::Vec overflow

// Metadata for each block
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

// Metadata for the slab
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

// The slab allocator structure
#[derive(Debug, Clone)]
pub struct Slab {
    base_addr: usize,
    slab_size: usize,
    head: Option<NonNull<BlockMeta>>,
}

impl Slab {
    pub fn new(base_addr: usize, slab_size: usize, mut meta: SlabMeta) -> Self {
        for j in 0..MAX_SLAB_PAGES {
            let page_addr = base_addr + (j * PAGE_SIZE);
            trace!(
                "Allocating page {} at address 0x{:x} for slab",
                j + 1,
                page_addr,
            );

            match kalloc_page(VirtAddr::new(page_addr as u64), PageType::Arbitrary) {
                Ok(_) => {
                    trace!("Page allocated successfully at 0x{page_addr:x}");
                }
                Err(e) => {
                    error!("Failed to allocate page at 0x{page_addr:x}: {e:#?}");
                    panic!("Failed to allocate page at 0x{page_addr:x}: {e:#?}");
                }
            }
        }

        let meta_ptr = base_addr as *mut SlabMeta;
        unsafe {
            *meta_ptr = meta;
        }

        let usable_space = MAX_SLAB_PAGES * PAGE_SIZE - size_of::<SlabMeta>();
        let actual_blocks = usable_space / (size_of::<BlockMeta>() + slab_size);

        if actual_blocks < meta.total {
            meta.total = actual_blocks;
            unsafe {
                *meta_ptr = meta;
            }
        }

        let mut head: Option<NonNull<BlockMeta>> = None;
        let mut prev: Option<NonNull<BlockMeta>> = None;
        unsafe {
            for i in 0..actual_blocks {
                let block_addr =
                    base_addr + size_of::<SlabMeta>() + i * (size_of::<BlockMeta>() + slab_size);
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

    pub fn used_blocks(&self) -> usize {
        let slab_meta = unsafe { &*(self.base_addr as *const SlabMeta) };
        slab_meta.used
    }

    /// Free all blocks (physically), making the virtual address range available for reuse.
    pub fn free(&self) {
        let base = self.base_addr;
        for i in 0..MAX_SLAB_PAGES {
            let page_addr = base + (i * PAGE_SIZE);
            match crate::memory::kfree_page(VirtAddr::new(page_addr as u64), PageType::Arbitrary) {
                Ok(_) => {
                    trace!("Page freed successfully at 0x{page_addr:x}");
                }
                Err(e) => {
                    error!("Failed to free page at 0x{page_addr:x}: {e:#?}");
                    panic!("Failed to free slab pages");
                }
            }
        }
    }

    pub fn alloc(&mut self) -> *mut u8 {
        let slab_meta = unsafe { &mut *(self.base_addr as *mut SlabMeta) };

        if slab_meta.magic_number != 0x1604_2010 << 1 | 1 {
            return core::ptr::null_mut();
        }
        if slab_meta.is_full() {
            return core::ptr::null_mut();
        }

        let bm_ptr = match self.head {
            None => return core::ptr::null_mut(),
            Some(nn) => {
                let next = unsafe { nn.as_ref().next };
                self.head = next;
                if let Some(mut nxt) = next {
                    unsafe {
                        nxt.as_mut().prev = None;
                    }
                }
                let _ = slab_meta.allocate();
                nn
            }
        };

        unsafe {
            (*bm_ptr.as_ptr()).used = true;
        }
        user_ptr_from_meta!(bm_ptr.as_ptr())
    }

    pub fn dealloc(&mut self, ptr: *mut u8) {
        let slab_meta = unsafe { &mut *(self.base_addr as *mut SlabMeta) };
        if slab_meta.magic_number != 0x1604_2010 << 1 | 1 || slab_meta.used == 0 || ptr.is_null() {
            return;
        }

        let bm = meta_ptr_from_user!(ptr);
        let mut nn = NonNull::new(bm).unwrap();

        unsafe {
            nn.as_mut().next = self.head;
            nn.as_mut().prev = None;
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
    pub fn dump_metadata(&self) {}
}

// ====== Allocator ======

#[derive(Default)]
pub struct SlabAllocator {
    // Increased Vec size to MAX_SLABS to accommodate more active slabs
    slabs: Vec<Mutex<Slab>, MAX_SLABS>,
    base_addr: AtomicUsize,
    // Stack to store virtual addresses of freed slabs for reuse
    freed_slots: Vec<usize, MAX_SLABS>,
}

impl SlabAllocator {
    pub const fn new() -> Self {
        SlabAllocator {
            slabs: Vec::new(),
            base_addr: AtomicUsize::new(SLAB_BASE_ADDR as usize),
            freed_slots: Vec::new(),
        }
    }
}

impl SlabAllocator {
    unsafe fn alloc(&mut self, layout: Layout) -> *mut u8 {
        let size = layout.size();
        let align = layout.align();

        if size == 0 {
            return core::ptr::null_mut();
        }

        if size > *SLAB_SIZE_CLASSES.last().unwrap() {
            warn!("Alloc size {} too large for slab", size);
            return core::ptr::null_mut();
        }

        let slab_size = match SLAB_SIZE_CLASSES.iter().find(|&&s| s >= size && s >= align) {
            Some(&s) => s,
            None => return core::ptr::null_mut(),
        };

        // 1. Try to find an existing slab with space
        let slab = self.slabs.iter().find(|s| s.lock().slab_size == slab_size);
        if let Some(mtx) = slab {
            let mut slab = mtx.lock();
            let ptr = slab.alloc();
            if !ptr.is_null() {
                return ptr;
            }
        }

        // 2. Create a new slab if existing ones are full or don't exist

        // Check if we can reuse a freed virtual address
        let base_addr = if let Some(recycled_addr) = self.freed_slots.pop() {
            trace!("Reusing freed slab address 0x{:x}", recycled_addr);
            recycled_addr
        } else {
            // No recycled addresses, increment the heap pointer
            self.base_addr
                .fetch_add(MAX_SLAB_PAGES * PAGE_SIZE, Ordering::SeqCst)
        };

        let usable_space = MAX_SLAB_PAGES * PAGE_SIZE - size_of::<SlabMeta>();
        let total_blocks = usable_space / (size_of::<BlockMeta>() + slab_size);
        let slab_meta = SlabMeta::new(slab_size, total_blocks);

        // This will physically allocate the pages at the chosen base_addr
        let mut new_slab = Slab::new(base_addr, slab_size, slab_meta);
        let ptr = new_slab.alloc();

        if ptr.is_null() {
            warn!("New slab allocation failed");
            // If we failed, we should probably try to return the address to freed_slots or handle the leak,
            // but for now we return null.
            return core::ptr::null_mut();
        }

        if self.slabs.push(Mutex::new(new_slab)).is_err() {
            error!("Slab list full, cannot track new slab. Leaking memory to prevent corruption.");
            // Panic or failure strategy here. Since we can't track it, we can't free it later.
            // We return the pointer because the memory IS allocated, but this is a critical state.
        }

        ptr
    }

    unsafe fn dealloc(&mut self, ptr: *mut u8, layout: Layout) {
        if ptr.is_null() {
            return;
        }

        let size = layout.size();
        let slab_size = match SLAB_SIZE_CLASSES.iter().find(|&&s| s >= size) {
            Some(&s) => s,
            None => return,
        };

        // Find slab index
        let index = match self
            .slabs
            .iter()
            .position(|s| s.lock().slab_size == slab_size)
        {
            Some(i) => i,
            None => {
                error!("Pointer {:p} not found in any slab", ptr);
                return;
            }
        };

        let mut slab_lock = self.slabs[index].lock();
        slab_lock.dealloc(ptr);
        let empty = slab_lock.used_blocks() == 0;

        // Important: Get the base address before dropping the lock/slab if we are going to remove it
        let slab_addr = slab_lock.base_addr;

        drop(slab_lock);

        if empty {
            trace!(
                "Slab at 0x{:x} is empty, freeing and recycling address",
                slab_addr
            );
            // Remove from active list
            let slab_mutex = self.slabs.swap_remove(index);
            let slab = slab_mutex.into_inner();

            // Free physical pages
            slab.free();

            // Recycle the virtual address
            // If the recycle bin is full, we just drop the address (leak the reuse opportunity, but safe)
            if self.freed_slots.push(slab_addr).is_err() {
                warn!(
                    "Freed slots cache full, unable to recycle slab address 0x{:x}",
                    slab_addr
                );
            }
        }
    }
}

struct LockedAllocator {
    slab_allocator: UnsafeCell<SlabAllocator>,
    large_allocator: LockedHeap,
}

impl LockedAllocator {
    pub const fn new() -> Self {
        LockedAllocator {
            slab_allocator: UnsafeCell::new(SlabAllocator::new()),
            large_allocator: LockedHeap::empty(),
        }
    }
}

// NOTE: Ensure your kernel architecture guarantees that `alloc` and `dealloc`
// are not called concurrently on multiple cores without an external lock,
// or ensure `without_interrupts` is sufficient for your specific case (UP vs SMP).
unsafe impl GlobalAlloc for LockedAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        x86_64::instructions::interrupts::without_interrupts(|| {
            if layout.size() > *SLAB_SIZE_CLASSES.last().unwrap() {
                let ptr = self.large_allocator.lock().allocate_first_fit(layout);
                return ptr.map(|p| p.as_ptr()).unwrap_or(core::ptr::null_mut());
            }

            let allocator = &self.slab_allocator;
            let ptr = allocator.as_mut_unchecked().alloc(layout);

            if ptr.is_null() {
                // Fallback to large allocator if slab fails
                let ptr = self.large_allocator.lock().allocate_first_fit(layout);
                return ptr.map(|p| p.as_ptr()).unwrap_or(core::ptr::null_mut());
            }
            ptr
        })
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
        x86_64::instructions::interrupts::without_interrupts(|| {
            if layout.size() > *SLAB_SIZE_CLASSES.last().unwrap() {
                if let Some(ptr) = NonNull::new(ptr) {
                    self.large_allocator.lock().deallocate(ptr, layout);
                }
                return;
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
    trace!("Initializing slab allocator");
    let mut addr = LARGE_ALLOC_BASE_ADDR;
    let pages = (super::PAGE_2M * 10) / super::PAGE_4K;
    for _ in 0..pages {
        kalloc_page(VirtAddr::new(addr), PageType::Arbitrary).unwrap();
        addr += super::PAGE_4K as u64;
    }
    unsafe {
        ALLOCATOR
            .large_allocator
            .lock()
            .init(LARGE_ALLOC_BASE_ADDR as *mut u8, super::PAGE_2M * 10);
    }
    trace!("Slab allocator initialized with base address 0x{SLAB_BASE_ADDR:x}");
}
