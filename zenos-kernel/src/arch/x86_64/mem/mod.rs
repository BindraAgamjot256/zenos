//! Memory management for x86_64 architecture.
//!
//! This module provides the core memory management subsystem for the kernel, including:
//! - `FrameAllocator`: A bitmap-based physical frame allocator for managing physical memory
//! - `paging`: Virtual memory management through x86_64 paging structures
//!
//! The frame allocator uses a bitmap approach where each bit represents a 4 KiB frame.
//! Multiple 64-bit bitmaps are used to cover the full address space up to 2^48.

#![allow(unused)] // we have a bunch of unsed stuff that will be used in the future, but we want to avoid warnings for now

mod frame_allocator;
pub(super) mod paging;

use frame_allocator::FrameAllocator;
use log::{info, error, warn, trace, debug};

use bitflags::bitflags;

use crate::arch::{
    MemMapErr,
    x86_64::addr::{PhysAddr, VirtAddr},
    x86_64::mem::{frame_allocator::FrameAllocError, paging::{Frame, Page, PageTableFlags, get_current_page_tables, page::{self, PageSize, Size1G, Size2M, Size4K}}},
};

bitflags! {
    /// Permissions and memory attributes for mapped pages.
    #[derive(Debug)]
    pub struct MemoryType: u32 {
        /// The page is readable.
        /// x86_64 does not expose a separate read-disable bit; readable is implied by presence.
        const READABLE = 1 << 0;

        /// The page is writable.
        const WRITABLE = 1 << 1;

        /// The page is executable.
        const EXECUTABLE = 1 << 2;

        /// The page is accessible from user mode.
        const USER_ACCESSIBLE = 1 << 3;

        /// The page is global and should not be flushed from the TLB when CR3 changes.
        const GLOBAL = 1 << 4;

        /// The page is not cached.
        const NO_CACHE = 1 << 5;

        /// The page uses write-through caching.
        const WRITE_THROUGH = 1 << 6;
    }
}

impl MemoryType {
    #[inline]
    pub fn to_page_table_flags(self) -> PageTableFlags {
        let mut flags = PageTableFlags::empty();

        if self.contains(MemoryType::WRITABLE) {
            flags |= PageTableFlags::WRITABLE;
        }

        if !self.contains(MemoryType::EXECUTABLE) {
            flags |= PageTableFlags::NO_EXECUTE;
        }

        if self.contains(MemoryType::USER_ACCESSIBLE) {
            flags |= PageTableFlags::USER_ACCESSIBLE;
        }

        if self.contains(MemoryType::GLOBAL) {
            flags |= PageTableFlags::GLOBAL;
        }

        if self.contains(MemoryType::NO_CACHE) {
            flags |= PageTableFlags::NO_CACHE;
        }

        if self.contains(MemoryType::WRITE_THROUGH) {
            flags |= PageTableFlags::WRITE_THROUGH;
        }

        flags
    }
}

static FRAME_ALLOCATOR: FrameAllocator = FrameAllocator::new_uninit();

/// Initializes the global frame allocator.
///
/// This function initializes the global `FRAME_ALLOCATOR` with the memory map from
/// the bootloader. It must be called exactly once during early kernel initialization.
///
/// # Arguments
/// * `mem_map` - Memory regions from the bootloader with (start, len, usable) tuples
/// * `phys_offset` - Virtual offset for physical memory access
///
/// # Panics
/// Panics if the allocator fails to initialize (should only happen if no suitable
/// memory region is available for bitmap storage).
pub fn init(
    mem_map: impl DoubleEndedIterator<Item = (usize, usize, bool)> + Clone,
    phys_offset: usize,
) {
    info!("global frame allocator init");

    unsafe {
        FRAME_ALLOCATOR
            .init(mem_map, phys_offset)
            .expect("failed to initialize frame allocator");
    }
    paging::init(phys_offset);

}

fn frame_alloc_err_to_mem_map_err(err: FrameAllocError) -> MemMapErr {
    match err {
        FrameAllocError::Uninitialized => MemMapErr::Uninit,
        FrameAllocError::OutOfMemory => MemMapErr::OutOfMem,
        FrameAllocError::TooLong => MemMapErr::TooLong,
        FrameAllocError::EmptyRange => MemMapErr::InvalidLength,
        FrameAllocError::AddressOverflow => MemMapErr::AddressOverflow,
        FrameAllocError::UnmanagedFrame => MemMapErr::UnmanagedFrame,
        FrameAllocError::BitmapPlacementFailed => MemMapErr::OutOfMem,
    }
}

fn map_to_mem_map_err(err: paging::MapToError) -> MemMapErr {
    match err {
        paging::MapToError::FrameAllocationFailed(frame_err) => frame_alloc_err_to_mem_map_err(frame_err),
        paging::MapToError::PageAlreadyMapped => MemMapErr::AlreadyMapped,
        paging::MapToError::ParentEntryHugePage => MemMapErr::ParentHugePage,
    }
}

pub fn map_mem_region(
    virt: VirtAddr,
    phys: Option<PhysAddr>,
    len: usize,
    mem_type: MemoryType,
) -> Result<PhysAddr, MemMapErr> {
    info!(
        "map_mem_region: virt={:#x}, phys={:?}, len={}, mem_type={:?}",
        virt.as_usize(),
        phys.map(|p| p.as_usize()),
        len,
        mem_type
    );

    let len = len
        .checked_add(Size4K::SIZE - 1)
        .ok_or_else(|| {
            error!("Length overflow while aligning len={}", len);
            MemMapErr::AddressOverflow
        })?
        & !(Size4K::SIZE - 1);

    debug!("Aligned mapping length to {} bytes", len);

    if len == 0 {
        error!("Rejected zero-length mapping");
        return Err(MemMapErr::InvalidLength);
    }

    if virt.as_usize() % Size4K::SIZE != 0 {
        error!(
            "Virtual address not 4KiB aligned: virt={:#x}",
            virt.as_usize()
        );
        return Err(MemMapErr::MisalignedAddress);
    }

    if let Some(phys_addr) = phys {
        if phys_addr.as_usize() % Size4K::SIZE != 0 {
            error!(
                "Physical address not 4KiB aligned: phys={:#x}",
                phys_addr.as_usize()
            );
            return Err(MemMapErr::MisalignedAddress);
        }
    }

    let frame_allocator = &FRAME_ALLOCATOR;

    let frame_addr = if let Some(phys) = phys {
        info!(
            "Reserving existing physical range: phys={:#x}, len={}",
            phys.as_usize(),
            len
        );

        frame_allocator
            .reserve_range(phys, len)
            .map_err(|e| {
                error!("Failed to reserve physical range: {:?}", e);
                frame_alloc_err_to_mem_map_err(e)
            })?;

        phys
    } else {
        let pages = len / Size4K::SIZE;

        info!("Allocating {} pages ({} bytes)", pages, len);

        frame_allocator.alloc_range(pages).map_err(|e| {
            error!("Failed to allocate frame range: {:?}", e);
            frame_alloc_err_to_mem_map_err(e)
        })?
    };

    debug!(
        "Using physical base address: {:#x}",
        frame_addr.as_usize()
    );

    let mut page_table = unsafe { get_current_page_tables() };
    let page_flags = mem_type.to_page_table_flags() | PageTableFlags::PRESENT;

    trace!("Page table flags: {:?}", page_flags);

    let mut current_virt = virt.as_usize();
    let mut current_phys = frame_addr.as_usize();
    let mut remaining = len;

    while remaining > 0 {
        if remaining >= Size1G::SIZE
            && current_virt % Size1G::SIZE == 0
            && current_phys % Size1G::SIZE == 0
        {
            debug!(
                "Mapping 1GiB page: virt={:#x} -> phys={:#x}",
                current_virt,
                current_phys
            );

            let page = Page::<Size1G>::containing_address(current_virt);
            let frame = Frame::<Size1G>::containing_address(current_phys);

            unsafe {
                page_table
                    .map_huge_1gib(page, frame, &frame_allocator, page_flags)
                    .map_err(|e| {
                        error!(
                            "Failed 1GiB mapping: virt={:#x}, phys={:#x}, err={:?}",
                            current_virt,
                            current_phys,
                            e
                        );
                        map_to_mem_map_err(e)
                    })?
                    .flush();
            }

            current_virt += Size1G::SIZE;
            current_phys += Size1G::SIZE;
            remaining -= Size1G::SIZE;

            continue;
        }

        if remaining >= Size2M::SIZE
            && current_virt % Size2M::SIZE == 0
            && current_phys % Size2M::SIZE == 0
        {
            debug!(
                "Mapping 2MiB page: virt={:#x} -> phys={:#x}",
                current_virt,
                current_phys
            );

            let page = Page::<Size2M>::containing_address(current_virt);
            let frame = Frame::<Size2M>::containing_address(current_phys);

            unsafe {
                page_table
                    .map_huge_2mib(page, frame, &frame_allocator, page_flags)
                    .map_err(|e| {
                        error!(
                            "Failed 2MiB mapping: virt={:#x}, phys={:#x}, err={:?}",
                            current_virt,
                            current_phys,
                            e
                        );
                        map_to_mem_map_err(e)
                    })?
                    .flush();
            }

            current_virt += Size2M::SIZE;
            current_phys += Size2M::SIZE;
            remaining -= Size2M::SIZE;

            continue;
        }

        trace!(
            "Mapping 4KiB page: virt={:#x} -> phys={:#x}",
            current_virt,
            current_phys
        );

        let page = Page::<Size4K>::containing_address(current_virt);
        let frame = Frame::<Size4K>::containing_address(current_phys);

        unsafe {
            page_table
                .map_to_4kib(page, frame, &frame_allocator, page_flags)
                .map_err(|e| {
                    error!(
                        "Failed 4KiB mapping: virt={:#x}, phys={:#x}, err={:?}",
                        current_virt,
                        current_phys,
                        e
                    );
                    map_to_mem_map_err(e)
                })?
                .flush();
        }

        current_virt += Size4K::SIZE;
        current_phys += Size4K::SIZE;
        remaining -= Size4K::SIZE;
    }

    info!(
        "Successfully mapped region: virt={:#x}, phys={:#x}, len={}",
        virt.as_usize(),
        frame_addr.as_usize(),
        len
    );

    Ok(frame_addr)
}

pub fn free_mem_region(
    virt: VirtAddr,
    len: usize,
) -> Result<(), MemMapErr>{
    todo!()
}