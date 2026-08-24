//! Main entry point for the zenos kernel.
//!
//! This file contains the entry point for the zenos kernel and basic error handling.
//!

#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]
extern crate alloc;
mod arch;
mod log;
mod mm;

use crate::mm::BUDDY_ALLOCATOR;
use alloc::vec::Vec;
use bootloader_api::{config::*, *};
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
#[cfg_attr(not(any(test, doctest)), panic_handler)]
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
    for i in 0..11 {
        let mut frame = BUDDY_ALLOCATOR.alloc(i).unwrap();
        log::info!("Allocated frame: {:?}", frame);
        let slice = frame.as_mut_slice();
        log::info!("Slice length: {}", slice.len());
        slice.fill(i);
        BUDDY_ALLOCATOR.free(frame).unwrap();
    }
    let mut vec = Vec::new();
    for i in 0..10u8 {
        vec.push(i);
        log::info!("pushed: {}", i)
    }
    log::info!("created vec: {:?}", vec);
    drop(vec);

    let tbox = KBox::new(test_alloc_macro::Test { data: [0u16; 510] })
        .unwrap_or_else(|_| panic!("alloc failed."));

    log::info!("tbox: {:?}", tbox);
    log::info!(
        "layout of Test: {:?}",
        core::alloc::Layout::new::<test_alloc_macro::Test>()
    );

    let dyn_kbox: KBox<dyn KernelObject, _> = tbox;
    log::info!("dyn_kbox: {:?}", dyn_kbox.raw_ptr());

    loop {}
}

fn kinit(boot_info: &'static mut BootInfo) {
    log::init();
    log::info!("Hello, zenos!");
    arch::init(boot_info);
}

mod test_alloc_macro {
    use kernel_macros::*;
    use kprimitives::alloc::{CreatableKernelObject, KernelObject};

    #[derive(Debug)]
    pub struct Test {
        pub data: [u16; 510],
    }
    impl KernelObject for Test {}
    impl CreatableKernelObject for Test {
        type Allocator = TestAllocator;
    }

    #[allocator(type = Test)]
    pub struct TestAllocator;
}
