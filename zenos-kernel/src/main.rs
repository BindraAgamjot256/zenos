//! Main entry point for the zenos kernel.
//!
//! This file contains the entry point for the zenos kernel and basic error handling.
//!
//! # Note about pronunciation
//! the name "zenos" is pronounced as one word, like in zeno's paradox, but with more emphasis on the s.
//! The name is not pronounced as "zen os" (like "zen operating system").\
//! IPA pronunciation: /ˈziː.nɒsss/

#![no_std]
#![no_main]

use bootloader_api::*;

static CONFIG: BootloaderConfig = {
    let config = BootloaderConfig::new_default();
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
fn _panic(_info: &core::panic::PanicInfo) -> ! {
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
fn kmain(_boot_info: &'static mut BootInfo) -> ! {
    loop {}
}
