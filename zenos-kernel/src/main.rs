//! Main entry point for the zenos kernel.
//!
//! This file contains the entry point for the zenos kernel and basic error handling.
//!

#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

extern crate alloc;
mod arch;
mod firmware;
mod log;
mod mm;
mod vmm;

use alloc::vec::Vec;
use bootloader_api::{config::*, *};
use core::hint::spin_loop;
use kprimitives::alloc::{KernelObject, boxed::KBox};

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
#[cfg_attr(target_os = "none", panic_handler)]
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

    log::info!("getting clock");
    let clock = arch::CLOCKSOURCE.read();
    let clock = clock.as_ref().unwrap();
    for _ in 0..10 {
        let then = clock.now();

        unsafe {
            log::set_max_level_racy(::log::LevelFilter::Off);
        }
        let mut vec = Vec::new();
        unsafe {
            log::set_max_level_racy(::log::LevelFilter::Debug);
        }
        for i in 0..10u8 {
            vec.push(i);
            //log::info!("pushed: {}", i)
        }
        //log::info!("created vec: {:?}", vec);

        let delta = clock.delta_now(then);
        log::info!("delta: {:?}", delta);
        drop(vec);
    }

    for _ in 0..10 {
        let then = clock.now();

        let mut x = 0u64;
        for i in 0..10_000 {
            x = core::hint::black_box(x.wrapping_add(i));
        }

        let delta = clock.delta_now(then);
        log::info!("delta: {:?}", delta);
    }

    let then = clock.now();
    let delta = clock.delta_now(then);
    log::info!("delta_instant: {:?}", delta);

    loop {
        spin_loop();
    }
}

fn kinit(boot_info: &'static mut BootInfo) {
    log::init();
    log::info!("Hello, zenos!");
    arch::init(boot_info);
}
