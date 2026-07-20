//! Memory management for x86_64 architecture.
//!
//! This module provides the core memory management subsystem for the kernel, including:
//! -
//! - `paging`: Virtual memory management through x86_64 paging structures
//!

#![allow(unused)] // we have a bunch of unsed stuff that will be used in the future, but we want to avoid warnings for now

pub(super) mod frame_allocator;
pub(super) mod paging;

use core::sync::atomic::AtomicUsize;

use log::{debug, error, info, trace, warn};

use bitflags::bitflags;

use crate::arch::{
    MemMapErr,
    x86_64::addr::{PhysAddr, VirtAddr},
    x86_64::mem::paging::{
        Frame, Page, PageTableFlags, get_current_page_tables,
        page::{self, PageSize, Size1G, Size2M, Size4K},
    },
};

pub const PAGE_SIZE: usize = 4096;
static PHYS_OFFSET: AtomicUsize = AtomicUsize::new(0);

bitflags! {
    /// Permissions and memory attributes for mapped pages.
    #[derive(Debug)]
    pub struct MemoryType: u32 {
        /// The page is readable.
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
    paging::init(phys_offset);
    frame_allocator::init(mem_map.clone());
}

fn map_to_mem_map_err(err: paging::MapToError) -> MemMapErr {
    match err {
        paging::MapToError::PageAlreadyMapped => MemMapErr::AlreadyMapped,
        paging::MapToError::ParentEntryHugePage => MemMapErr::ParentHugePage,
        paging::MapToError::FrameAllocationFailed => MemMapErr::Uninit, //todo: fix.
    }
}

pub fn map_mem_region(
    virt: VirtAddr,
    phys: Option<PhysAddr>,
    len: usize,
    mem_type: MemoryType,
) -> Result<PhysAddr, MemMapErr> {
    todo!();
}

pub fn free_mem_region(virt: VirtAddr, len: usize) -> Result<(), MemMapErr> {
    todo!();
}

pub fn get_phys_offset() -> usize {
    PHYS_OFFSET.load(core::sync::atomic::Ordering::SeqCst)
}