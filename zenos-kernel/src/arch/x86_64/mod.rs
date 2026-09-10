//! x86_64 architecture-specific implementation.
//!
//! This module provides the core x86_64-specific functionality including:
//! - Address types for physical and virtual addresses
//! - Memory management (frame allocation, paging)
//! - I/O port access (low-level read/write operations)
//! - Serial port communication for debugging
//!
//! The module initializes architecture-specific components during kernel boot.

mod addr;
mod gdt;
mod interrupts;
pub mod mem;
pub mod ports;
pub mod registers;
pub mod serial;
mod timers;

use crate::firmware;
pub use addr::{PhysAddr, VirtAddr};
use bootloader_api::info::{MemoryRegion as Region, MemoryRegionKind};
pub use interrupts::{InterruptContext, InterruptGuard, register_interrupt_handler};
pub use timers::CLOCKSOURCE;

pub fn init(boot_info: &'static mut crate::BootInfo) {
    log::info!("Initializing architecture-specific components...");

    let regions = &mut *boot_info.memory_regions;
    let write = merge_contiguous_regions(regions);
    let iter = regions[..write].iter().map(|r| {
        (
            r.start as usize,                   // start
            (r.end - r.start) as usize,         // length
            r.kind == MemoryRegionKind::Usable, // usable
        )
    });

    mem::init(
        iter,
        boot_info
            .physical_memory_offset
            .into_option()
            .unwrap_or_default() as usize,
    );

    let bootdata = firmware::init(boot_info);
    log::info!("{:#?}", bootdata);
    log::info!(
        "Running on machine with OEM: {}",
        bootdata.platform.oem_id.clone().unwrap_or("unknown".into())
    );
    timers::init(&bootdata);
    gdt::init_gdt();
    interrupts::init(&bootdata);

    self::interrupts::with_handler(
        0x3,
        |ctx| {
            log::info!("breakpoint exception, context: {:?}", ctx);
        },
        || {
            unsafe { core::arch::asm!("int3") };
        },
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
