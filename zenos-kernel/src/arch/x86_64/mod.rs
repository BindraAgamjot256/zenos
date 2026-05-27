//! x86_64 architecture-specific implementation.
//!
//! This module provides the core x86_64-specific functionality including:
//! - Address types for physical and virtual addresses
//! - Memory management (frame allocation, paging)
//! - I/O port access (low-level read/write operations)
//! - Serial port communication for debugging
//!
//! The module initializes architecture-specific components during kernel boot.

use bootloader_api::info::{MemoryRegion as Region, MemoryRegionKind};

mod addr;
mod mem;
pub mod ports;
pub mod registers;
pub mod serial;

pub use addr::{PhysAddr, VirtAddr};
pub use mem::{map_mem_region, MemoryType};

pub fn init(boot_info: &'static mut crate::BootInfo) {
    log::info!("Initializing architecture-specific components...");

    let regions = &mut *boot_info.memory_regions;

    let write = merge_contiguous_regions(regions);

    let iter = regions[..write].iter().map(|r| {
        (
            r.start as usize,
            (r.end - r.start) as usize,
            r.kind == MemoryRegionKind::Usable,
        )
    });

    mem::init(
        iter,
        boot_info
            .physical_memory_offset
            .into_option()
            .unwrap_or_default() as usize,
    );

}

fn merge_contiguous_regions(regions: &mut [Region]) -> usize {
    // We will compact into the front of the same buffer
    let mut write = 0;

    for i in 0..regions.len() {
        let current = regions[i];

        // skip already processed entries
        if i == 0 {
            regions[write] = current;
            write += 1;
            continue;
        }

        let last = &mut regions[write - 1];

        let same_kind = last.kind == current.kind;
        let contiguous = last.end == current.start;

        if same_kind && contiguous {
            // merge into previous
            if current.end > last.end {
                last.end = current.end;
            }
        } else {
            // write new region
            regions[write] = current;
            write += 1;
        }
    }
    write
}
