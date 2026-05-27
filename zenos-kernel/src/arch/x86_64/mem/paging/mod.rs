//! x86_64 paging structures and management.
//!
//! This module provides the core data structures for x86_64 paging:
//! - `PageTableFlags`: Flags controlling page table entry behavior
//! - `PageTableEntry`: A single entry in a page table
//! - `PageTableIndex`: Calculated index into a specific paging level
//! - `PageTable`: A single 4KB page table with 512 entries
//!
//! These types work together to implement multi-level paging translation
//! from virtual addresses to physical addresses.

#![allow(unused)]
#![allow(unsafe_op_in_unsafe_fn)]

pub mod entry;
pub mod flags;
pub mod index;
pub mod page;
pub mod table;

pub use entry::PageTableEntry;
pub use flags::PageTableFlags;
pub use index::PageTableIndex;
pub use page::{Frame, Page};
pub use table::PageTable;

use crate::arch::{
    PhysAddr, VirtAddr,
    x86_64::mem::frame_allocator::{FrameAllocError, FrameAllocator},
};

use core::{arch::asm, sync::atomic::AtomicUsize};

use log::info;
use page::PageSize;

/// Errors returned by mapping operations.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum MapToError {
    /// Frame allocator returned no free frames.
    FrameAllocationFailed(FrameAllocError),

    /// Attempted to map an already mapped page.
    PageAlreadyMapped,

    /// Encountered a huge page while walking page tables.
    ParentEntryHugePage,
}

/// Returned by mapping operations.
///
/// This type is intentionally `#[must_use]` so callers are forced to
/// explicitly decide whether to invalidate the TLB.
#[must_use = "TLB flushes must be handled explicitly via .flush() or .ignore()"]
pub struct MapperFlush<S: PageSize> {
    page: Page<S>,
}

impl<S: PageSize> MapperFlush<S> {
    #[inline]
    pub fn new(page: Page<S>) -> Self {
        Self { page }
    }

    /// Flushes the mapped page from the TLB using `invlpg`.
    #[inline]
    pub fn flush(self) {
        unsafe {
            asm!(
                "invlpg [{}]",
                in(reg) self.page.start_address(),
                options(nostack, preserves_flags)
            );
        }
    }

    /// Explicitly ignores the flush.
    ///
    /// Useful during early boot before paging is fully active,
    /// or before a later CR3 reload.
    #[inline]
    pub fn ignore(self) {}
}

pub struct OffsetPageTable<'a> {
    l4_table: &'a mut PageTable,
    physical_memory_offset: VirtAddr,
}

impl<'a> OffsetPageTable<'a> {
    /// Creates a new `OffsetPageTable`.
    ///
    /// # Safety
    ///
    /// Caller must guarantee:
    /// - `l4_table` is the active level 4 table
    /// - `physical_memory_offset` correctly maps all physical memory
    pub unsafe fn new(l4_table: &'a mut PageTable, physical_memory_offset: VirtAddr) -> Self {
        Self {
            l4_table,
            physical_memory_offset,
        }
    }

    /// Zeroes the entire page table.
    ///
    /// # Safety
    ///
    /// Must only be used on inactive page tables.
    /// Calling this on the active table will invalidate currently
    /// active mappings and almost certainly crash the kernel.
    pub unsafe fn init(&mut self) {
        self.l4_table.zero();

        info!(
            "Initialized page table at physical address {:#x}",
            self.l4_table as *const _ as u64 - self.physical_memory_offset.as_u64()
        );
    }

    /// Converts a physical address into a higher-half virtual address.
    #[inline]
    fn phys_to_virt<T>(phys: PhysAddr, physical_memory_offset: VirtAddr) -> *mut T {
        (phys.as_u64() + physical_memory_offset.as_u64()) as *mut T
    }

    /// Returns a mutable reference to a page table from a physical address.
    ///
    /// # Safety
    ///
    /// Caller must ensure the physical address actually points to a valid
    /// page table mapped into the higher-half direct map.
    #[inline]
    unsafe fn table_from_phys(
        phys: PhysAddr,
        physical_memory_offset: VirtAddr,
    ) -> &'a mut PageTable {
        &mut *Self::phys_to_virt::<PageTable>(phys, physical_memory_offset)
    }

    /// Allocates a new page table.
    fn alloc_table(
        frame_alloc: &FrameAllocator,
        physical_memory_offset: VirtAddr,
    ) -> Result<PhysAddr, MapToError> {
        let frame = frame_alloc
            .alloc()
            .map_err(MapToError::FrameAllocationFailed)?;

        debug_assert_eq!(frame.as_u64() % 4096, 0);

        unsafe {
            Self::table_from_phys(frame, physical_memory_offset).zero();
        }

        Ok(frame)
    }

    /// Walks to the next paging level.
    ///
    /// Allocates a new table if necessary.
    fn next_table_create(
        current: &mut PageTable,
        index: PageTableIndex,
        frame_alloc: &FrameAllocator,
        table_flags: PageTableFlags,
        physical_memory_offset: VirtAddr,
    ) -> Result<&'a mut PageTable, MapToError> {
        let entry = &mut current[index];

        if entry.flags().contains(PageTableFlags::HUGE_PAGE) {
            return Err(MapToError::ParentEntryHugePage);
        }

        if !entry.is_present() {
            let new_table_phys = Self::alloc_table(frame_alloc, physical_memory_offset)?;

            entry.set_addr(new_table_phys);
            entry.add_flags(table_flags);
        }

        Ok(unsafe { Self::table_from_phys(entry.addr(), physical_memory_offset) })
    }

    /// Maps a page.
    pub unsafe fn map_to_with_table_flags_4k(
        &mut self,
        page: Page<page::Size4K>,
        frame: Frame<page::Size4K>,
        frame_alloc: &FrameAllocator,
        page_flags: PageTableFlags,
        table_flags: PageTableFlags,
    ) -> Result<MapperFlush<page::Size4K>, MapToError> {
        let virt_addr = VirtAddr::new(page.start_address() as u64);
        let phys_addr = PhysAddr::new(frame.start_address() as u64);

        let l4_index = PageTableIndex::new(virt_addr, 0);
        let l3_index = PageTableIndex::new(virt_addr, 1);
        let l2_index = PageTableIndex::new(virt_addr, 2);
        let l1_index = PageTableIndex::new(virt_addr, 3);

        let l4_table = &mut *self.l4_table;
    

        let l3_table = Self::next_table_create(
            l4_table,
            l4_index,
            frame_alloc,
            table_flags,
            self.physical_memory_offset,
        )?;
        
        let l2_table = Self::next_table_create(
            l3_table,
            l3_index,
            frame_alloc,
            table_flags,
            self.physical_memory_offset,
        )?;


        let l1_table = Self::next_table_create(
            l2_table,
            l2_index,
            frame_alloc,
            table_flags,
            self.physical_memory_offset,
        )?;


        let entry = &mut l1_table[l1_index];

        if entry.is_present() {
            return Err(MapToError::PageAlreadyMapped);
        }

        entry.set_addr(phys_addr);
        entry.add_flags(page_flags | PageTableFlags::PRESENT);

        Ok(MapperFlush::new(page))
    }

    /// Maps using default intermediate table flags.
    pub unsafe fn map_to_4kib(
        &mut self,
        page: Page<page::Size4K>,
        frame: Frame<page::Size4K>,
        frame_alloc: &FrameAllocator,
        page_flags: PageTableFlags,
    ) -> Result<MapperFlush<page::Size4K>, MapToError> {
        self.map_to_with_table_flags_4k(
            page,
            frame,
            frame_alloc,
            page_flags,
            PageTableFlags::GLOBAL
                | PageTableFlags::WRITABLE
                | PageTableFlags::PRESENT
                | PageTableFlags::NO_EXECUTE
                | if page_flags.contains(PageTableFlags::USER_ACCESSIBLE) {
                    PageTableFlags::USER_ACCESSIBLE
                } else {
                    PageTableFlags::empty()
                },
        )
    }

    /// Maps a 2MiB huge page.
    pub unsafe fn map_huge_2mib(
        &mut self,
        page: Page<page::Size2M>,
        frame: Frame<page::Size2M>,
        frame_alloc: &FrameAllocator,
        flags: PageTableFlags,
    ) -> Result<MapperFlush<page::Size2M>, MapToError> {
        let virt_addr = VirtAddr::new(page.start_address() as u64);
        let phys_addr = PhysAddr::new(frame.start_address() as u64);

        debug_assert_eq!(phys_addr.as_u64() % (2 * 1024 * 1024), 0);
        debug_assert_eq!(virt_addr.as_u64() % (2 * 1024 * 1024), 0);

        let l4_index = PageTableIndex::new(virt_addr, 0);
        let l3_index = PageTableIndex::new(virt_addr, 1);
        let l2_index = PageTableIndex::new(virt_addr, 2);

        let l3_table = Self::next_table_create(
            self.l4_table,
            l4_index,
            frame_alloc,
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
            self.physical_memory_offset,
        )?;

        let l2_table = Self::next_table_create(
            l3_table,
            l3_index,
            frame_alloc,
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
            self.physical_memory_offset,
        )?;

        let entry = &mut l2_table[l2_index];

        if entry.is_present() {
            return Err(MapToError::PageAlreadyMapped);
        }

        entry.set_addr(phys_addr);
        entry.add_flags(flags | PageTableFlags::PRESENT | PageTableFlags::HUGE_PAGE);

        Ok(MapperFlush::new(page))
    }

    /// Maps a 1GiB huge page.
    pub unsafe fn map_huge_1gib(
        &mut self,
        page: Page<page::Size1G>,
        frame: Frame<page::Size1G>,
        frame_alloc: &FrameAllocator,
        flags: PageTableFlags,
    ) -> Result<MapperFlush<page::Size1G>, MapToError> {
        let virt_addr = VirtAddr::new(page.start_address() as u64);
        let phys_addr = PhysAddr::new(frame.start_address() as u64);

        debug_assert_eq!(phys_addr.as_u64() % (1024 * 1024 * 1024), 0);
        debug_assert_eq!(virt_addr.as_u64() % (1024 * 1024 * 1024), 0);

        let l4_index = PageTableIndex::new(virt_addr, 0);
        let l3_index = PageTableIndex::new(virt_addr, 1);

        let l3_table = Self::next_table_create(
            &mut *self.l4_table,
            l4_index,
            frame_alloc,
            PageTableFlags::PRESENT | PageTableFlags::WRITABLE,
            self.physical_memory_offset,
        )?;

        let entry = &mut l3_table[l3_index];

        if entry.is_present() {
            return Err(MapToError::PageAlreadyMapped);
        }

        entry.set_addr(phys_addr);
        entry.add_flags(flags | PageTableFlags::PRESENT | PageTableFlags::HUGE_PAGE);

        Ok(MapperFlush::new(page))
    }
}

static PHYS_OFFSET: AtomicUsize = AtomicUsize::new(0);

#[inline(always)]
pub fn init(phys_offset: usize) {
    PHYS_OFFSET.store(phys_offset, core::sync::atomic::Ordering::SeqCst);
}

pub unsafe fn get_current_page_tables<'a>() -> OffsetPageTable<'a> {
    let physical_mem_offset = PHYS_OFFSET.load(core::sync::atomic::Ordering::SeqCst);

    let (l4_table, _) = crate::arch::registers::control::CR3::read_raw();
    let virt = l4_table.start_address() + physical_mem_offset;
    let page_table_ptr = virt as *mut PageTable;

    let l4_table = unsafe{&mut *page_table_ptr};


    OffsetPageTable::new(l4_table, VirtAddr::new(physical_mem_offset as u64))
}
