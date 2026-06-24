//! Main entry point for the zenos kernel.
//!
//! This file contains the entry point for the zenos kernel and basic error handling.
//!
//! # Note about pronunciation
//! the name "zenos" is pronounced as one word, like in zeno's paradox, but with more emphasis on the s.
//! The name is not pronounced as "zen os" (like "zen operating system").\

#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

mod arch;
mod log;
mod primitives;

use arch::map_mem_region;
use bootloader_api::{config::*, *};

use crate::arch::{MemoryType, PhysAddr, VirtAddr};

static CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::Dynamic);
    config.mappings.dynamic_range_start = Some(0xFFFF_8000_0000_0000);
    config
};

/// Panic handler for the kernel.
///
/// This function is called when a panic occurs in the kernel code.
/// It outputs panic information to the serial port for debugging,
/// then enters an infinite loop, halting the system.
///
/// # Parameters
///
/// * `info` - Information about the panic, including location and message
///
/// # Returns
///
/// This function never returns (marked by `!` return type)
#[cfg_attr(not(any(test, feature = "run-kunittest")), panic_handler)]
fn _panic(info: &core::panic::PanicInfo) -> ! {
    log::error!("FUCK");
    log::error!("PANIC");
    log::error!("PANIC INFO: {}", info);
    loop {}
}

// Defines the kernel main function as the entry point and adds metadata for the bootloader.
entry_point!(kmain, config = &CONFIG);

/// Kernel main function - the entry point for the OS.
///
/// This function is called by the bootloader after basic hardware initialization.
/// It initializes the kernel and then enters an idle loop.
///
/// # Parameters
///
/// * `boot_info` - Information provided by the bootloader about system configuration
///
/// # Returns
///
/// This function never returns (marked by `!` return type)
fn kmain(boot_info: &'static mut BootInfo) -> ! {
    kinit(boot_info);
    map_mem_region(
        VirtAddr::new(0x8000),
        Some(PhysAddr::new(0x8000)),
        0x1000,
        MemoryType::READABLE | MemoryType::WRITABLE,
    )
    .unwrap();
    let ptr = 0x8000 as *mut ();
    let slice = unsafe { core::slice::from_raw_parts_mut(ptr as *mut u8, 0x1000 - 1) };
    slice.fill(1u8);
    loop {}
}

fn kinit(boot_info: &'static mut BootInfo) {
    log::init();
    log::info!("Hello, zenos!");
    arch::init(boot_info);
}
