//! Memory management for x86_64 architecture.
//!
//! This module provides the core memory management subsystem for the kernel, including:
//! -
//! - `paging`: Virtual memory management through x86_64 paging structures
//!

pub(super) mod frame_allocator;
pub(super) mod paging;

pub use crate::arch::x86_64::mem::frame_allocator::{memmap_addr, memmap_len};
use crate::vmm::VmmArena;
use crate::{
    arch::{
        VirtAddr,
        common::mem_types::{MappingError, MemoryType},
        mem::paging::{FrameAllocator, get_current_page_tables},
        x86_64::{addr::PhysAddr, mem::paging::PageTableFlags},
    },
    mm::BUDDY_ALLOCATOR,
};
use core::sync::atomic::AtomicUsize;
use kprimitives::mutex::Mutex;

pub const PAGE_SIZE: usize = 4096;
static PHYS_OFFSET: AtomicUsize = AtomicUsize::new(0);
static IO_VMM_ARENA: Mutex<Option<VmmArena>> = Mutex::new(None);

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
    let memmap_len = memmap_len();
    let memmap_end = memmap_addr::<u8>() as usize + memmap_len;
    let iovmm = VmmArena::new(
        VirtAddr::new(memmap_end as u64),
        VirtAddr::new(memmap_end as u64 + 0x10000),
    );
    IO_VMM_ARENA.lock().replace(iovmm);
}

pub fn get_phys_offset() -> usize {
    PHYS_OFFSET.load(core::sync::atomic::Ordering::SeqCst)
}

pub fn ioremap(addr: PhysAddr, size: usize) -> Result<VirtAddr, MappingError> {
    let size = (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    let mut va_arena = IO_VMM_ARENA.lock();
    let va = va_arena.as_mut().ok_or(MappingError::Uninit)?;
    let vaddr = va
        .allocate_region(addr, size)
        .ok_or(MappingError::OutOfMem)?;
    let mut pt = unsafe { get_current_page_tables() };
    let fa = &BUDDY_ALLOCATOR;
    for i in (0..size).step_by(PAGE_SIZE) {
        unsafe {
            pt.map_to_with_table_flags_4k(
                VirtAddr::new(vaddr.as_u64() + i as u64),
                PhysAddr::new(addr.as_u64() + i as u64),
                fa,
                PageTableFlags::PRESENT
                    | PageTableFlags::WRITABLE
                    | PageTableFlags::NO_CACHE
                    | PageTableFlags::NO_EXECUTE,
                PageTableFlags::PRESENT | PageTableFlags::WRITABLE | PageTableFlags::NO_EXECUTE,
            )
            .map_err(|e| todo!("error: {e:?}"))?
            .flush();
        }
    }
    Ok(vaddr)
}

pub fn iounmap(vaddr: VirtAddr, size: usize) -> Result<(), MappingError> {
    let size = (size + PAGE_SIZE - 1) & !(PAGE_SIZE - 1);
    let mut pt = unsafe { get_current_page_tables() };
    for i in (0..size).step_by(PAGE_SIZE) {
        unsafe {
            pt.unmap(VirtAddr::new(vaddr.as_u64() + i as u64))
                .map_err(|e| todo!("error: {e:?}"))?
                .flush();
        }
    }
    IO_VMM_ARENA
        .lock()
        .as_mut()
        .map(|v| v.free_region(vaddr))
        .flatten()
        .ok_or(MappingError::Uninit)
}

// this impl only exists for x86_64
impl FrameAllocator for crate::mm::buddy::BuddyAllocator {
    fn alloc_frame(&self) -> Option<PhysAddr> {
        let mapping = self.alloc(0).ok()?;
        let start = (mapping.as_slice().as_ptr() as usize) - get_phys_offset();
        let phys_addr = PhysAddr::new(start as u64);
        Some(phys_addr)
    }
}
