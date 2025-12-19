//! Production-quality memory subsystem in Rust
//!
//! Features:
//! - Thread-safe (using spinlock)
//! - O(1) page allocation/deallocation
//! - Supports 4 KiB and 2 MiB pages
//! - Uses a linked list of bitmaps stored at a fixed higher-half virtual address

pub mod alloc;

pub use constants::*;
use core::cmp::PartialEq;
use core::{
    ptr::NonNull,
    slice,
    sync::atomic::{AtomicU64, Ordering},
};
use log::{error, trace, warn};
use spin::Mutex;
use x86_64::{
    registers::control::Cr3, structures::paging::mapper::UnmapError,
    structures::paging::Translate,
    structures::paging::{
        FrameAllocator, FrameDeallocator, Mapper, OffsetPageTable, Page, PageTable, PageTableFlags,
        PhysFrame, Size2MiB, Size4KiB,
    },
    PhysAddr,
    VirtAddr,
};

/// Page sizes supported
#[derive(Copy, Clone, Debug, Ord, PartialOrd, Eq, PartialEq)]
#[repr(usize)]
pub enum PageSize {
    Size4KiB = PAGE_4K,
    Size2MiB = PAGE_2M,
}

#[derive(Debug, Copy, Clone)]
/// Types of mappings
pub enum PageType {
    /// MMIO page (no physical backing, just a mapping)
    Mmio,
    /// Identity-mapped (virtual == physical)
    Identity,
    /// Recursive mapping (virtual == physical + offset)
    Recursive,
    /// Huge page (2MiB, recursive style)
    Huge,
    /// An arbitrary page (4KiB, recursive style, but will be mapped to the last free physical address)
    Arbitrary,
    /// An arbitrary page(4KiB recursive style, mapped to a specific physical address)
    ArbitraryPhys(PhysAddr),
}

/// Possible page allocation/mapping errors
#[derive(Debug)]
pub enum MapErr {
    /// No free pages available
    OutOfMemory,
    /// Page not previously mapped
    NotMapped,
    /// Initialization not performed
    Uninitialized,
    /// Invalid alignment for huge pages
    InvalidAlignment,
}

/// Node in the linked list of bitmaps
#[repr(C)]
struct BitmapNode {
    next: Option<NonNull<BitmapNode>>,
    /// Dynamic bitmap where 1 = free, 0 = allocated
    map: NonNull<[AtomicU64]>,
    /// Base physical address this node manages
    base_phys: u64,
    /// Size of this region in bytes
    region_size: usize,
}

pub mod constants {
    // Constants
    pub const HIGHER_HALF_BASE: u64 = 0xFFFF_8000_0000_0000;
    pub const PAGE_4K: usize = 4096;
    pub const PAGE_2M: usize = 2 * 1024 * 1024;
    pub const KERNEL_BASE: u64 = 0xFFFF_8000_5000_0000;
    pub const KERNEL_STACK_BASE: u64 = 0xFFFF_8001_0000_0000;
    pub const KERNEL_CR3_SCRATCH: u64 = 0xFFFF_FFFF_0000_0000;
}

pub(crate) struct PageAllocator {
    head: Option<NonNull<BitmapNode>>,
    lock: Mutex<()>,
}

impl PageAllocator {
    unsafe fn initialize(memory_regions: impl Iterator<Item = (u64, usize)>) -> Self {
        let mut head = None;
        let mut storage_offset = 0usize;

        for (start_phys, size) in memory_regions {
            let aligned_start = (start_phys + PAGE_4K as u64 - 1) & !(PAGE_4K as u64 - 1);
            let end_phys = start_phys + size as u64;
            let aligned_size = ((end_phys - aligned_start) / PAGE_4K as u64) as usize * PAGE_4K;

            if aligned_size < PAGE_4K {
                continue;
            }

            let pages_in_region = aligned_size / PAGE_4K;
            let bitmap_words = pages_in_region.div_ceil(64);

            let node_virt = HIGHER_HALF_BASE + storage_offset as u64;
            let node_ptr = node_virt as *mut BitmapNode;
            let node = NonNull::new_unchecked(node_ptr);

            let bitmap_virt = node_virt + size_of::<BitmapNode>() as u64;
            let bitmap_ptr = bitmap_virt as *mut AtomicU64;
            let bitmap_slice = slice::from_raw_parts_mut(bitmap_ptr, bitmap_words);

            (*node_ptr).next = head;
            (*node_ptr).base_phys = aligned_start;
            (*node_ptr).region_size = aligned_size;
            (*node_ptr).map = NonNull::new_unchecked(bitmap_slice);

            let bitmap = (*node_ptr).map.as_ref();
            for word in bitmap {
                word.store(u64::MAX, Ordering::Relaxed);
            }

            let total_bits = pages_in_region;
            let last_word_bits = total_bits % 64;
            if last_word_bits != 0 {
                let last_word_idx = bitmap_words - 1;
                let mask = (1u64 << last_word_bits) - 1;
                bitmap[last_word_idx].store(mask, Ordering::Relaxed);
            }

            head = Some(node);
            storage_offset += size_of::<BitmapNode>() + bitmap_words * size_of::<AtomicU64>();
        }

        PageAllocator {
            head,
            lock: Mutex::new(()),
        }
    }

    fn alloc(&self, size: PageSize, specific_addr: Option<PhysAddr>) -> Option<PhysAddr> {
        // Locking the allocator because multithreading is a thing, and race conditions suck
        let _guard = self.lock.lock();

        // Decide the step size based on the page size
        let step = match size {
            PageSize::Size4KiB => 1,                 // 4KiB pages use 1-bit steps
            PageSize::Size2MiB => PAGE_2M / PAGE_4K, // 2MiB pages = 512 x 4KiB pages
        };

        // If we're being fancy and someone asked for a specific address
        if let Some(phys_addr) = specific_addr {
            let phys = phys_addr.as_u64();
            match size {
                PageSize::Size4KiB => {
                    if phys % PAGE_4K as u64 != 0 {
                        log::debug!("Requested 4KiB address {phys:#x} not aligned to 4KiB");
                        return None;
                    }

                    let mut node_opt = self.head;

                    while let Some(node_ptr) = node_opt {
                        let node = unsafe { node_ptr.as_ref() };

                        if phys >= node.base_phys && phys < node.base_phys + node.region_size as u64
                        {
                            let offset = phys - node.base_phys;
                            let page_num = (offset / PAGE_4K as u64) as usize;
                            let word_idx = page_num / 64;
                            let bit_pos = page_num % 64;
                            let bitmap = unsafe { node.map.as_ref() };

                            if word_idx < bitmap.len() {
                                let word = &bitmap[word_idx];
                                let mask = 1u64 << bit_pos;
                                let val = word.load(Ordering::Acquire);

                                if val & mask == 0 {
                                    log::debug!("4KiB page at {phys:#x} is already allocated");
                                    return None;
                                }

                                // Try to flip the bit from 1 to 0 (allocated!)
                                loop {
                                    match word.compare_exchange(
                                        val,
                                        val & !mask,
                                        Ordering::AcqRel,
                                        Ordering::Acquire,
                                    ) {
                                        Ok(_) => {
                                            // log::debug!("Allocated 4KiB page at {phys:#x}");
                                            return Some(phys_addr);
                                        }
                                        Err(new_val) => {
                                            if new_val & mask == 0 {
                                                log::debug!(
                                                    "Race detected: page at {phys:#x} already allocated",
                                                );
                                                return None;
                                            }
                                        }
                                    }
                                }
                            }
                            log::debug!("4KiB page at {phys:#x} is out of bitmap bounds");
                            return None;
                        }

                        node_opt = node.next;
                    }

                    log::debug!("4KiB page address {phys:#x} not in any known region");
                    None
                }

                PageSize::Size2MiB => {
                    if phys % PAGE_2M as u64 != 0 {
                        log::debug!("Requested 2MiB address {phys:#x} not aligned to 2MiB");
                        return None;
                    }

                    let mut node_opt = self.head;

                    while let Some(node_ptr) = node_opt {
                        let node = unsafe { node_ptr.as_ref() };

                        if phys >= node.base_phys && phys < node.base_phys + node.region_size as u64
                        {
                            let offset = phys - node.base_phys;
                            let page_num = (offset / PAGE_4K as u64) as usize;

                            if !page_num.is_multiple_of(step) {
                                log::debug!("2MiB page at {phys:#x} not aligned to step {step}",);
                                return None;
                            }

                            let word_idx = page_num / 64;
                            let bitmap = unsafe { node.map.as_ref() };
                            let words_needed = step / 64;

                            if word_idx + words_needed > bitmap.len() {
                                log::debug!(
                                    "2MiB page at {phys:#x} exceeds bitmap size; cannot allocate",
                                );
                                return None;
                            }

                            // Check that all relevant bits are free (set to 1)
                            for i in 0..words_needed {
                                let w = &bitmap[word_idx + i];
                                if w.load(Ordering::Acquire) != u64::MAX {
                                    log::debug!(
                                        "Cannot allocate 2MiB pages at {:#x}, word {} not fully free",
                                        phys,
                                        word_idx + i
                                    );
                                    return None;
                                }
                            }

                            // Allocate by clearing all relevant bits
                            for i in 0..words_needed {
                                let w = &bitmap[word_idx + i];
                                w.store(0, Ordering::Release);
                            }

                            log::debug!("Allocated 2MiB pages at {phys:#x}");
                            return Some(phys_addr);
                        }

                        node_opt = node.next;
                    }

                    log::debug!("2MiB page address {phys:#x} not in any known region");
                    None
                }
            }
        } else {
            // No specific address requested — time to go hunting through the free list
            let mut node_opt = self.head;

            while let Some(node_ptr) = node_opt {
                let node = unsafe { node_ptr.as_ref() };
                let bitmap = unsafe { node.map.as_ref() };

                for (word_idx, word) in bitmap.iter().enumerate() {
                    let mut val = word.load(Ordering::Acquire);

                    if val != 0 {
                        let mut bit_pos = 0;

                        while bit_pos <= 64 - step
                            && word_idx * 64 + bit_pos < node.region_size / PAGE_4K
                        {
                            // Generate the appropriate mask
                            let mask = if step == 1 {
                                1u64 << bit_pos
                            } else {
                                let global_bit = word_idx * 64 + bit_pos;
                                let aligned_bit = (global_bit + step - 1) & !(step - 1);

                                if aligned_bit >= node.region_size / PAGE_4K {
                                    break;
                                }

                                let aligned_word = aligned_bit / 64;
                                let local_bit = aligned_bit % 64;

                                if aligned_word != word_idx || local_bit + step > 64 {
                                    break;
                                }

                                ((1u64 << step) - 1) << local_bit
                            };

                            if (val & mask) == mask {
                                // It's free! Try to claim it
                                match word.compare_exchange_weak(
                                    val,
                                    val & !mask,
                                    Ordering::AcqRel,
                                    Ordering::Acquire,
                                ) {
                                    Ok(_) => {
                                        let page_num = if step == 1 {
                                            word_idx * 64 + bit_pos
                                        } else {
                                            let global_bit = word_idx * 64 + bit_pos;
                                            (global_bit + step - 1) & !(step - 1)
                                        };

                                        let phys_addr =
                                            node.base_phys + (page_num * PAGE_4K) as u64;

                                        return Some(PhysAddr::new(phys_addr));
                                    }
                                    Err(new_val) => {
                                        val = new_val;
                                        continue;
                                    }
                                }
                            }

                            bit_pos += if step == 1 { 1 } else { step };
                        }
                    }
                }

                node_opt = node.next;
            }

            log::debug!("No free page found for {size:?} allocation");
            None
        }
    }

    fn map_last_free_page(&self, size: PageSize) -> Option<PhysAddr> {
        if size == PageSize::Size2MiB {
            error!("map_last_free_page only supports 4KiB pages");
            return None;
        }

        let mut node_opt = self.head;
        while let Some(node_ptr) = node_opt {
            let node = unsafe { node_ptr.as_ref() };
            let map = unsafe { node.map.as_ref() };

            for word_idx in (0..map.len()).rev() {
                let val = map[word_idx].load(Ordering::Acquire);

                if val != 0 {
                    // There is at least one free bit
                    let bit = 63 - val.leading_zeros() as usize;
                    // set bit to 0
                    let mask = 1u64 << bit;
                    map[word_idx].fetch_and(!mask, Ordering::AcqRel);

                    let page_index = word_idx * 64 + bit;
                    let addr = node.base_phys + (page_index * PAGE_4K) as u64;
                    trace!("map_last_free_page: allocated page at {addr:#x}");

                    return Some(PhysAddr::new(addr));
                }
            }

            node_opt = node.next;
        }

        None
    }

    fn dealloc(&self, phys_addr: PhysAddr, size: PageSize) -> Result<(), MapErr> {
        let _guard = self.lock.lock();
        let phys = phys_addr.as_u64();
        let step = match size {
            PageSize::Size4KiB => 1,
            PageSize::Size2MiB => PAGE_2M / PAGE_4K,
        };

        let mut node_opt = self.head;
        while let Some(node_ptr) = node_opt {
            let node = unsafe { node_ptr.as_ref() };
            if phys >= node.base_phys && phys < node.base_phys + node.region_size as u64 {
                let offset = phys - node.base_phys;
                let page_num = (offset / PAGE_4K as u64) as usize;
                let aligned_page = page_num & !(step - 1);
                let word_idx = aligned_page / 64;
                let bit_start = aligned_page % 64;
                let bitmap = unsafe { node.map.as_ref() };
                if word_idx < bitmap.len() {
                    let mask = if step == 1 {
                        1u64 << bit_start
                    } else {
                        ((1u64 << step) - 1) << bit_start
                    };
                    bitmap[word_idx].fetch_or(mask, Ordering::AcqRel);
                    return Ok(());
                }
                return Err(MapErr::NotMapped);
            }
            node_opt = node.next;
        }
        Err(MapErr::NotMapped)
    }
    pub(crate) fn get_first_free_phys(&self) -> Result<PhysAddr, MapErr> {
        let _guard = self.lock.lock();

        let mut node_opt = self.head;
        while let Some(node_ptr) = node_opt {
            let node = unsafe { node_ptr.as_ref() };
            let bitmap = unsafe { node.map.as_ref() };

            for (word_idx, word) in bitmap.iter().enumerate() {
                let val = word.load(Ordering::Acquire);

                if val != 0 {
                    // Find the first set bit (1 = free page)
                    let bit = val.trailing_zeros() as usize;

                    // Safety check — trailing_zeros might point past the end of a region if last word is partial
                    let page_index = word_idx * 64 + bit;
                    if page_index < node.region_size / PAGE_4K {
                        let addr = node.base_phys + (page_index * PAGE_4K) as u64;
                        return Ok(PhysAddr::new(addr));
                    }
                }
            }

            node_opt = node.next;
        }

        Err(MapErr::OutOfMemory)
    }
}

unsafe impl Sync for PageAllocator {}
unsafe impl Send for PageAllocator {}

unsafe impl FrameAllocator<Size4KiB> for PageAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size4KiB>> {
        let phys_addr = self.alloc(PageSize::Size4KiB, None);
        phys_addr.map(PhysFrame::containing_address)
    }
}

unsafe impl FrameAllocator<Size2MiB> for PageAllocator {
    fn allocate_frame(&mut self) -> Option<PhysFrame<Size2MiB>> {
        let phys_addr = self.alloc(PageSize::Size2MiB, None);
        phys_addr.map(PhysFrame::containing_address)
    }
}

impl FrameDeallocator<Size4KiB> for PageAllocator {
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame<Size4KiB>) {
        let phys_addr = frame.start_address();
        if let Err(e) = self.dealloc(phys_addr, PageSize::Size4KiB) {
            error!("Error unmapping frame: {e:#?}")
        }
    }
}

impl FrameDeallocator<Size2MiB> for PageAllocator {
    unsafe fn deallocate_frame(&mut self, frame: PhysFrame<Size2MiB>) {
        let phys_addr = frame.start_address();
        if let Err(e) = self.dealloc(phys_addr, PageSize::Size2MiB) {
            error!("Error unmapping frame: {e:#?}")
        }
    }
}

/// Global allocator holder
pub(crate) static ALLOCATOR: Mutex<Option<PageAllocator>> = Mutex::new(None);
pub static MAPPER: Mutex<Option<OffsetPageTable>> = Mutex::new(None);

/// Initialize the allocator
pub fn init(
    _kernel_base: VirtAddr, // offset addr where kernel will be remapped... will remain unused since i decided to fork the bootloader and do it there
    offset: VirtAddr,       // address where bootloader remaps physical memory
    _last: usize,           // last free memory region same as above, unused
    memory_regions_iter: impl Iterator<Item = (u64, usize)>, // an iterator of memory regions as (start, size)
) {
    let l4_table = unsafe { active_level_4_table(offset) };
    let mut mapper = unsafe { OffsetPageTable::new(l4_table, offset) };

    let entry_recursive = &mut mapper.level_4_table_mut()[511];

    let frame = Cr3::read().0;
    entry_recursive.set_frame(frame, PageTableFlags::PRESENT | PageTableFlags::WRITABLE);

    unsafe {
        ALLOCATOR
            .lock()
            .replace(PageAllocator::initialize(memory_regions_iter));
        MAPPER.lock().replace(mapper);
    }
}

/// # Safety
/// The caller must ensure the physical memory offset is a valid pointer.
pub unsafe fn active_level_4_table(physical_memory_offset: VirtAddr) -> &'static mut PageTable {
    use x86_64::registers::control::Cr3;

    let (level_4_table_frame, _) = Cr3::read();

    let phys = level_4_table_frame.start_address();
    let virt = physical_memory_offset + phys.as_u64();
    let page_table_ptr: *mut PageTable = virt.as_mut_ptr();

    &mut *page_table_ptr
}

pub fn virt_to_phys(virt: VirtAddr) -> Option<PhysAddr> {
    let mut mapper = MAPPER.lock();
    let mapper = mapper.as_mut().unwrap();
    mapper.translate_addr(virt)
}

pub fn kalloc_page(virtaddr: VirtAddr, ptype: PageType) -> Result<PhysAddr, MapErr> {
    let mut alloc_guard = ALLOCATOR.lock();
    let alloc = alloc_guard.as_mut().ok_or(MapErr::Uninitialized)?;

    // Validate alignment for huge pages (because misaligned huge pages are a crime)
    if matches!(ptype, PageType::Huge) && !virtaddr.as_u64().is_multiple_of(PAGE_2M as u64) {
        return Err(MapErr::InvalidAlignment);
    }

    // Allocate physical memory
    let frame = allocate_frame(alloc, &virtaddr, &ptype)?;

    // Setup flags
    let mut flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE;
    match ptype {
        PageType::Mmio => flags |= PageTableFlags::NO_CACHE,
        PageType::Huge => flags |= PageTableFlags::HUGE_PAGE,
        _ => flags |= PageTableFlags::GLOBAL,
    }

    // Actually map the page
    map_page(&virtaddr, &frame, flags, alloc, &ptype)?;

    trace!(
        "Map successful, {:#x} -> {:#x}",
        virtaddr.as_u64(),
        frame.0.as_u64()
    );

    Ok(frame.0)
}

pub fn ualloc_page(virtaddr: VirtAddr, ptype: PageType) -> Result<PhysAddr, MapErr> {
    let mut alloc_guard = ALLOCATOR.lock();
    let alloc = alloc_guard.as_mut().ok_or(MapErr::Uninitialized)?;

    // Validate alignment for huge pages (because misaligned huge pages are a crime)
    if matches!(ptype, PageType::Huge) && !virtaddr.as_u64().is_multiple_of(PAGE_2M as u64) {
        return Err(MapErr::InvalidAlignment);
    }

    // Allocate physical memory
    let frame = allocate_frame(alloc, &virtaddr, &ptype)?;

    // Setup flags
    let mut flags = PageTableFlags::PRESENT | PageTableFlags::WRITABLE;
    match ptype {
        PageType::Mmio => flags |= PageTableFlags::NO_CACHE,
        PageType::Huge => flags |= PageTableFlags::HUGE_PAGE,
        _ => {}
    }

    flags |= PageTableFlags::USER_ACCESSIBLE;

    // Actually map the page
    map_page(&virtaddr, &frame, flags, alloc, &ptype)?;

    trace!(
        "Map successful, {:#x} -> {:#x}",
        virtaddr.as_u64(),
        frame.0.as_u64()
    );

    Ok(frame.0)
}

pub fn ualloc_page_flags(
    virtaddr: VirtAddr,
    ptype: PageType,
    flags: PageTableFlags,
) -> Result<PhysAddr, MapErr> {
    let mut alloc_guard = ALLOCATOR.lock();
    let alloc = alloc_guard.as_mut().ok_or(MapErr::Uninitialized)?;

    // Validate alignment for huge pages (because misaligned huge pages are a crime)
    if matches!(ptype, PageType::Huge) && !virtaddr.as_u64().is_multiple_of(PAGE_2M as u64) {
        return Err(MapErr::InvalidAlignment);
    }

    // Allocate physical memory
    let frame = allocate_frame(alloc, &virtaddr, &ptype)?;

    // Setup flags
    let mut flags = flags | PageTableFlags::PRESENT;
    match ptype {
        PageType::Mmio => flags |= PageTableFlags::NO_CACHE,
        PageType::Huge => flags |= PageTableFlags::HUGE_PAGE,
        _ => {}
    }

    flags |= PageTableFlags::USER_ACCESSIBLE;

    // Actually map the page
    map_page(&virtaddr, &frame, flags, alloc, &ptype)?;

    trace!(
        "Map successful, {:#x} -> {:#x}",
        virtaddr.as_u64(),
        frame.0.as_u64()
    );

    Ok(frame.0)
}

fn allocate_frame(
    alloc: &mut PageAllocator,
    virtaddr: &VirtAddr,
    ptype: &PageType,
) -> Result<(PhysAddr, PageSize), MapErr> {
    let addr = match ptype {
        PageType::Huge => alloc.alloc(PageSize::Size2MiB, Some(PhysAddr::new(virtaddr.as_u64()))),
        PageType::Mmio => Some(PhysAddr::new(virtaddr.as_u64())),
        PageType::Identity => {
            alloc.alloc(PageSize::Size4KiB, Some(PhysAddr::new(virtaddr.as_u64())))
        }

        PageType::Recursive => alloc.alloc(
            PageSize::Size4KiB,
            Some(PhysAddr::new(
                virtaddr.as_u64().wrapping_sub(HIGHER_HALF_BASE),
            )),
        ),
        PageType::Arbitrary => alloc.map_last_free_page(PageSize::Size4KiB),
        PageType::ArbitraryPhys(phhys) => Some(*phhys),
    };

    addr.map(|a| {
        let size = match ptype {
            PageType::Huge => PageSize::Size2MiB,
            _ => PageSize::Size4KiB,
        };
        (a, size)
    })
    .ok_or_else(|| {
        if matches!(ptype, PageType::Recursive) {
            error!("Failed to allocate recursive mapping");
        }
        MapErr::OutOfMemory
    })
}

fn map_page(
    virtaddr: &VirtAddr,
    frame: &(PhysAddr, PageSize),
    flags: PageTableFlags,
    alloc: &mut dyn FrameAllocator<Size4KiB>,
    ptype: &PageType,
) -> Result<(), MapErr> {
    let mut mapper_guard = MAPPER.lock();
    let mapper = mapper_guard.as_mut().ok_or(MapErr::Uninitialized)?;

    let (phys, size) = frame;

    // Logging because who doesn’t love unnecessary runtime noise?
    match ptype {
        PageType::Recursive | PageType::Arbitrary => {
            trace!(
                "Mapping {:?} page at {:#x}, with phys addr {:#x}",
                ptype,
                virtaddr.as_u64(),
                phys.as_u64()
            );
        }
        _ => {}
    }

    unsafe {
        match size {
            PageSize::Size2MiB => mapper
                .map_to(
                    Page::<Size2MiB>::containing_address(*virtaddr),
                    PhysFrame::containing_address(*phys),
                    flags,
                    alloc,
                )
                .map_err(|e| {
                    error!("err: {e:#?}");
                    MapErr::OutOfMemory
                })?
                .flush(),
            PageSize::Size4KiB => {
                let result = mapper.map_to(
                    Page::<Size4KiB>::containing_address(*virtaddr),
                    PhysFrame::containing_address(*phys),
                    flags,
                    alloc,
                );

                match result {
                    Ok(flush) => flush.flush(), // fwooosh
                    Err(x86_64::structures::paging::mapper::MapToError::ParentEntryHugePage) => {
                        // a huge page already covers this 4KiB page, ignore
                    }
                    Err(x86_64::structures::paging::mapper::MapToError::PageAlreadyMapped(e)) => {
                        // page is already mapped... ignore but warn
                        warn!("Page already mapped {e:#?}");
                    }
                    Err(e) => {
                        error!("err: {e:#?}");
                        return Err(MapErr::OutOfMemory);
                    }
                }
            }
        };
    }

    Ok(())
}

/// Free a previously allocated page
pub fn kfree_page(virtaddr: VirtAddr, ptype: PageType) -> Result<(), MapErr> {
    let mut alloc = ALLOCATOR.lock();
    let alloc = alloc.as_mut().ok_or(MapErr::Uninitialized)?;
    let size = match ptype {
        PageType::Huge => PageSize::Size2MiB,
        _ => PageSize::Size4KiB,
    };

    // TODO: In a real implementation, you would:
    // 1. Look up the physical address from the page table
    // 2. Unmap the virtual address
    // 3. Then free the physical page
    // But this isn't a real implementation. so... 🖕

    // Future me here: I implemented something... hope it works:

    let mut ptable = MAPPER.lock();
    let ptable = ptable.as_mut().ok_or(MapErr::Uninitialized)?;

    let phys = ptable.translate_addr(virtaddr).ok_or(MapErr::NotMapped)?;

    match size {
        PageSize::Size2MiB => {
            let err = ptable.unmap(Page::<Size4KiB>::containing_address(virtaddr));

            alloc.dealloc(phys, PageSize::Size4KiB)?;

            match err {
                Ok((_, flush)) => flush.flush(), // fwoosh
                Err(e) => match e {
                    UnmapError::ParentEntryHugePage => {}
                    _ => {
                        error!("err: {e:#?}");
                        return Err(MapErr::NotMapped);
                    }
                },
            }
        }
        PageSize::Size4KiB => {
            let err = ptable.unmap(Page::<Size4KiB>::containing_address(virtaddr));

            alloc.dealloc(phys, PageSize::Size4KiB)?;

            match err {
                Ok((_, flush)) => flush.flush(), // fwoosh
                Err(e) => match e {
                    UnmapError::ParentEntryHugePage => {}
                    _ => {
                        error!("err: {e:#?}");
                        return Err(MapErr::NotMapped);
                    }
                },
            }
        }
    }
    // For now, we assume virtual address maps to physical via the kernel offset
    Ok(())
}
pub fn kleak_page(virtaddr: VirtAddr, ptype: PageType) -> Result<(), MapErr> {
    let mut alloc = ALLOCATOR.lock();
    alloc.as_mut().ok_or(MapErr::Uninitialized)?;
    let size = match ptype {
        PageType::Huge => PageSize::Size2MiB,
        _ => PageSize::Size4KiB,
    };

    let mut ptable = MAPPER.lock();
    let ptable = ptable.as_mut().ok_or(MapErr::Uninitialized)?;

    match size {
        PageSize::Size2MiB => {
            let err = ptable.unmap(Page::<Size4KiB>::containing_address(virtaddr));

            match err {
                Ok((_, flush)) => flush.flush(), // fwoosh
                Err(e) => match e {
                    UnmapError::ParentEntryHugePage => {}
                    _ => {
                        error!("err: {e:#?}");
                        return Err(MapErr::NotMapped);
                    }
                },
            }
        }
        PageSize::Size4KiB => {
            let err = ptable.unmap(Page::<Size4KiB>::containing_address(virtaddr));

            match err {
                Ok((_, flush)) => flush.flush(), // fwoosh
                Err(e) => match e {
                    UnmapError::ParentEntryHugePage => {}
                    _ => {
                        error!("err: {e:#?}");
                        return Err(MapErr::NotMapped);
                    }
                },
            }
        }
    }
    Ok(())
}
static DMA_BASE: AtomicU64 = AtomicU64::new(KERNEL_BASE + 0x200_000);

pub fn kalloc_dma_pages(len: usize) -> Result<&'static mut [u8], MapErr> {
    if len == 0 {
        return Err(MapErr::NotMapped); // or handle zero-size gracefully
    }

    let num_pages = len.div_ceil(PAGE_4K);
    let mut virt_base = DMA_BASE.load(Ordering::SeqCst);

    kalloc_page(VirtAddr::new(virt_base), PageType::Recursive)?;
    let first_virt = VirtAddr::new(virt_base);
    virt_base += PAGE_4K as u64;

    for _ in 1..num_pages {
        kalloc_page(VirtAddr::new(virt_base), PageType::Recursive)?;
        virt_base += PAGE_4K as u64;
    }

    DMA_BASE.store(virt_base, Ordering::SeqCst);
    let buf = unsafe { slice::from_raw_parts_mut(first_virt.as_mut_ptr(), len) };
    Ok(buf)
}
pub fn kfree_dma_pages(buf: &mut [u8]) -> Result<(), MapErr> {
    let len = buf.len();
    if len == 0 {
        return Err(MapErr::NotMapped);
    }

    let num_pages = len.div_ceil(PAGE_4K);
    let base_ptr = buf.as_ptr() as u64;

    for i in 0..num_pages {
        let page_ptr = base_ptr + (i as u64) * PAGE_4K as u64;
        kfree_page(VirtAddr::new(page_ptr), PageType::Recursive)?;
    }

    DMA_BASE.store(base_ptr, Ordering::SeqCst); // store the actual start
    Ok(())
}

/// Get allocator statistics
pub fn get_stats() -> Result<(usize, usize), MapErr> {
    let mut binding = ALLOCATOR.lock();
    let alloc = binding.as_mut().ok_or(MapErr::Uninitialized)?;
    let _guard = alloc.lock.lock();

    let mut total_pages = 0;
    let mut free_pages = 0;

    let mut node_opt = alloc.head;
    while let Some(node_ptr) = node_opt {
        let node = unsafe { node_ptr.as_ref() };
        let bitmap = unsafe { node.map.as_ref() };
        let pages_in_region = node.region_size / PAGE_4K;

        for (word_idx, word) in bitmap.iter().enumerate() {
            let val = word.load(Ordering::Acquire);
            let bits_in_this_word = if word_idx == bitmap.len() - 1 {
                // Last word might be partial
                let remaining_pages = pages_in_region - (word_idx * 64);
                core::cmp::min(64, remaining_pages)
            } else {
                64
            };

            total_pages += bits_in_this_word;
            free_pages += (val & ((1u64 << (bits_in_this_word - 1)) - 1)).count_ones() as usize;
        }

        node_opt = node.next;
    }

    Ok((free_pages, total_pages))
}

#[cfg(feature = "run-kunittest")]
pub(crate) mod tests {
    use super::*;
    use crate::{test_assert, test_assert_eq, Test};

    fn mock_regions() -> [(u64, usize); 2] {
        // Mock two memory regions: 16 KiB and 8 KiB
        [
            (0x1000, 16 * 1024), // 16 KiB
            (0x20000, 8 * 1024), // 8 KiB
        ]
    }

    fn make_allocator() -> PageAllocator {
        unsafe { PageAllocator::initialize(mock_regions().into_iter()) }
    }

    #[zenos_macros::test]
    pub fn test_allocator_initializes() -> Option<()> {
        let alloc = make_allocator();
        test_assert!(alloc.head.is_some());
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_allocate_and_free_4kib() -> Option<()> {
        let alloc = make_allocator();
        let phys = alloc.alloc(PageSize::Size4KiB, None);
        test_assert!(phys.is_some());

        let addr = phys.unwrap();
        // Free it back
        test_assert!(alloc.dealloc(addr, PageSize::Size4KiB).is_ok());
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_allocate_specific_address() -> Option<()> {
        let alloc = make_allocator();
        let specific = PhysAddr::new(0x20000); // aligned address
        let phys = alloc.alloc(PageSize::Size4KiB, Some(specific));
        test_assert_eq!(phys, Some(specific));
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_allocate_2mib_alignment_fail() -> Option<()> {
        let alloc = make_allocator();
        let misaligned = PhysAddr::new(0x21000); // not 2 MiB aligned
        let phys = alloc.alloc(PageSize::Size2MiB, Some(misaligned));
        test_assert!(phys.is_none());
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_out_of_memory() -> Option<()> {
        let alloc = make_allocator();

        // Try to allocate way too many pages
        let mut count = 0;
        while alloc.alloc(PageSize::Size4KiB, None).is_some() {
            count += 1;
            if count > 10_000 {
                break; // just in case
            }
        }

        let none_left = alloc.alloc(PageSize::Size4KiB, None);
        test_assert!(none_left.is_none());
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_get_first_free_phys() -> Option<()> {
        let alloc = make_allocator();
        let first = alloc.get_first_free_phys();
        test_assert!(first.is_ok());
        Some(())
    }
}
