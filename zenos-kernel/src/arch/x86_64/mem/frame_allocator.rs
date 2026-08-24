use core::{
    mem::MaybeUninit,
    ops::Range,
    sync::atomic::{AtomicUsize, Ordering},
};

use crate::{
    arch::x86_64::mem::paging::{FrameAllocator, Page, PageTableFlags, get_current_page_tables},
    mm::{Page as s_Page, PageFlags},
};

const PAGE_SIZE: usize = 4096;
const MEMMAP_START: usize = 0xffffea0000000000;
const MEMMAP_HEADROOM_PERCENT: usize = 10; // 10% extra memory is taken, to be used by the BootFrameAllocator, when it maps page tables.

pub(super) fn init(mem_map: impl Iterator<Item = (usize, usize, bool)> + Clone) {
    log::info!("Initializing frame allocator...");

    mem_map
        .clone()
        .for_each(|i| assert_eq!((i.0) % 4096, 0, "iterator has field not aligned to page"));

    /*
     * The struct-page array is indexed by physical page number.
     *
     * Therefore we need an entry for EVERY physical page from 0 up to
     * the highest physical address in the memory map.
     *
     * Example:
     *
     *     usable:  0x00000000..0x20000000
     *     hole:    0x20000000..0xB0000000
     *     reserved 0xB0000000..0xC0000000
     *
     * The page array still needs entries for the hole, because page N
     * corresponds directly to physical address N * PAGE_SIZE.
     */
    let highest_physical_end = mem_map
        .clone()
        .map(|(region_start, region_size, _)| {
            region_start
                .checked_add(region_size)
                .expect("Physical memory region address overflow")
        })
        .max()
        .expect("Memory map is empty");

    let total_pages = highest_physical_end.div_ceil(PAGE_SIZE);

    log::info!("Highest physical address: {:#x}", highest_physical_end);

    log::info!(
        "Total number of struct pages: {:#x} ({})",
        total_pages,
        total_pages
    );

    let memmap_size = total_pages
        .checked_mul(core::mem::size_of::<s_Page>())
        .expect("Struct page array size overflow");

    log::info!("Total size of struct pages: {:#x}", memmap_size);

    /*
     * Pages actually required for the struct page array.
     */
    let memmap_pages = memmap_size.div_ceil(PAGE_SIZE);

    /*
     * Pages reserved for the boot allocator, including headroom.
     */
    let boot_allocator_pages = (memmap_pages * (100 + MEMMAP_HEADROOM_PERCENT)).div_ceil(100);

    let memmap_bytes = memmap_pages * PAGE_SIZE;
    let reserved_bytes = boot_allocator_pages * PAGE_SIZE;

    log::info!(
        "Struct page array requires {} pages ({:#x} bytes)",
        memmap_pages,
        memmap_bytes
    );

    log::info!(
        "Boot allocator reservation: {} pages ({:#x} bytes)",
        boot_allocator_pages,
        reserved_bytes
    );

    /*
     * Pick the largest usable region that can fit the boot allocator.
     *
     * The reservation is taken from the END of this region.
     */
    let backing_region = mem_map
        .clone()
        .filter(|&(_, _, usable)| usable)
        .filter(|&(_, size, _)| size >= reserved_bytes)
        .max_by_key(|&(_, size, _)| size)
        .expect("No usable memory region large enough for memmap");

    let backing_region_start = backing_region.0;
    let backing_region_size = backing_region.1;

    assert_eq!(
        backing_region_start % PAGE_SIZE,
        0,
        "Backing region is not page aligned"
    );

    assert_eq!(
        backing_region_size % PAGE_SIZE,
        0,
        "Backing region size is not page aligned"
    );

    /*
     * Reserve memory from the END of the selected usable region.
     *
     * Example:
     *
     *     usable region
     *     |------------------------------------------|
     *     ^                                          ^
     *     start                                      end
     *
     *                                  |-------------|
     *                                  boot allocator
     *                                  reservation
     */
    let backing_start = backing_region_start
        .checked_add(backing_region_size)
        .and_then(|end| end.checked_sub(reserved_bytes))
        .expect("Boot allocator backing region overflow");

    let boot_frame_allocator = BootFrameAllocator {
        start: backing_start,
        next: AtomicUsize::new(0),
        count: boot_allocator_pages,
    };

    let mut page_tables = unsafe { get_current_page_tables() };

    /*
     * Map only the pages actually occupied by the struct-page array.
     */
    for page_index in 0..memmap_pages {
        let frame = boot_frame_allocator
            .alloc_frame()
            .expect("Boot frame allocator exhausted");

        unsafe {
            page_tables
                .map_to_4kib(
                    Page::containing_address(MEMMAP_START + page_index * PAGE_SIZE),
                    frame,
                    &boot_frame_allocator,
                    PageTableFlags::GLOBAL | PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
                )
                .expect("Failed to map struct page array")
                .flush();
        }
    }

    log::info!("Mapped struct page array.");

    let reserved_frame_range = boot_frame_allocator.allocated_range();

    log::info!(
        "Reserved physical memory for memmap: {:#x?}",
        reserved_frame_range
    );

    /*
     * Create the struct-page array.
     *
     * There is one entry for EVERY physical page up to the highest
     * physical address, including holes and reserved regions.
     */
    let slice = unsafe {
        core::slice::from_raw_parts_mut(MEMMAP_START as *mut MaybeUninit<s_Page>, total_pages)
    };

    log::info!("Struct page slice created.");

    /*
     * Initialize EVERY page as RESERVED.
     *
     * This is the important bit for holes.
     *
     * If there is a physical hole such as:
     *
     *     0x20000000..0xB0000000
     *
     * then the corresponding struct pages already exist and are marked
     * RESERVED. We don't need a special "hole" state.
     *
     * The only pages we later turn into USABLE are pages explicitly
     * reported as usable by the firmware memory map.
     */
    for page in slice.iter_mut() {
        unsafe {
            core::ptr::write(page.as_mut_ptr(), s_Page::null());

            (*page.as_mut_ptr()).flags = PageFlags::RESERVED;
        }
    }

    let slice = unsafe { slice.assume_init_mut() };

    log::info!("Struct page array initialized as RESERVED.");

    /*
     * Mark only pages belonging to usable memory regions as usable.
     *
     * Everything else remains RESERVED:
     *
     * - holes
     * - MMIO
     * - ACPI
     * - bootloader memory
     * - firmware reservations
     * - etc.
     */
    for (region_start, region_size, usable) in mem_map.clone() {
        if !usable {
            continue;
        }

        let region_end = region_start
            .checked_add(region_size)
            .expect("Memory region address overflow");

        /*
         * Only complete pages are usable.
         *
         * If a region happened to start/end in the middle of a page,
         * don't accidentally expose that partial page as normal RAM.
         */
        let first_page = region_start.div_ceil(PAGE_SIZE);
        let last_page = region_end / PAGE_SIZE;

        if first_page >= last_page {
            continue;
        }

        log::info!(
            "Usable region {:#x}..{:#x} => pages {}..{}",
            region_start,
            region_end,
            first_page,
            last_page,
        );

        for (page_index, i) in slice
            .iter_mut()
            .enumerate()
            .take(last_page)
            .skip(first_page)
        {
            let phys = page_index * PAGE_SIZE;

            /*
             * The backing memory used by the boot allocator belongs to
             * a usable region, but must remain RESERVED.
             */
            if reserved_frame_range.contains(&phys) {
                continue;
            }

            i.mark_usable();
        }
    }

    /*
     * Physical page 0 must never be allocated.
     */
    slice[0].flags = PageFlags::RESERVED;

    log::info!("Physical page 0 reserved.");

    /*
     * At this point:
     *
     *     usable RAM  -> PageFlags::KERNEL
     *     holes       -> PageFlags::RESERVED
     *     MMIO        -> PageFlags::RESERVED
     *     firmware    -> PageFlags::RESERVED
     *     boot memory -> PageFlags::RESERVED
     *
     * The struct-page array therefore has a direct physical-page
     * correspondence:
     *
     *     slice[N] <=> physical address N * PAGE_SIZE
     */

    log::info!(
        "Frame allocator metadata initialized for {:#x} physical pages.",
        total_pages
    );

    let usable_pfn_iter = UsablePfnIter {
        ptr: slice.as_ptr(),
        len: slice.len(),
        cursor: 0,
    };

    crate::mm::init(slice, usable_pfn_iter);
}

pub const fn memmap_addr<T>() -> *mut T {
    MEMMAP_START as *mut T
}

pub struct BootFrameAllocator {
    /// Physical byte address of the first frame.
    start: usize,

    /// Number of frames already allocated.
    next: AtomicUsize,

    /// Total number of frames available.
    count: usize,
}

impl FrameAllocator for BootFrameAllocator {
    fn alloc_frame(&self) -> Option<super::paging::Frame<super::paging::page::Size4K>> {
        let idx = self.next.fetch_add(1, Ordering::SeqCst);

        if idx >= self.count {
            return None;
        }

        Some(super::paging::Frame::containing_address(
            self.start + idx * PAGE_SIZE,
        ))
    }
}

impl BootFrameAllocator {
    pub fn allocated_frames(&self) -> usize {
        self.next.load(Ordering::Acquire)
    }

    pub fn allocated_range(&self) -> Range<usize> {
        let used = self.allocated_frames();

        self.start..(self.start + used * PAGE_SIZE)
    }
}

pub struct UsablePfnIter {
    ptr: *const s_Page,
    len: usize,
    cursor: usize,
}

impl Iterator for UsablePfnIter {
    type Item = (usize, usize);

    fn next(&mut self) -> Option<Self::Item> {
        unsafe {
            while self.cursor < self.len && !(*self.ptr.add(self.cursor)).is_usable() {
                self.cursor += 1;
            }
            if self.cursor >= self.len {
                return None;
            }
            let start = self.cursor;
            while self.cursor < self.len && (*self.ptr.add(self.cursor)).is_usable() {
                self.cursor += 1;
            }
            Some((start, self.cursor))
        }
    }
}
