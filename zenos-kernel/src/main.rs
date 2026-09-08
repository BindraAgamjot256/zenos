//! Main entry point for the zenos kernel.
//!
//! This file contains the entry point for the zenos kernel and basic error handling.

#![no_std]
#![no_main]
#![deny(unsafe_op_in_unsafe_fn)]

extern crate alloc;
mod arch;
mod firmware;
mod log;
mod mm;
mod vmm;

use crate::arch::common::timers::Instant;
use crate::arch::{InterruptContext, register_interrupt_handler};
use bootloader_api::{config::*, *};
use core::sync::atomic::{AtomicBool, Ordering};
use core::time::Duration;

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

    #[cfg(feature = "__test_timer")]
    test_timer();

    let now = arch::CLOCKSOURCE.read().as_ref().unwrap().now();
    log::info!(
        "good day everyone, it is {:?} (according to the clock)",
        now
    );

    test_timer_irq();
    loop {}
}

#[cfg(feature = "__test_timer")]
fn test_timer() {
    log::info!("getting clock");
    let clock = arch::CLOCKSOURCE.read();
    let clock = clock.as_ref().unwrap();

    for _ in 0..10 {
        let then = clock.now();
        log::qmp_pause_barrier();
        let delta = clock.delta_now(then);
        log::qmp_pause_complete();
        log::info!("delta sleep: {:?}", delta);
    }

    let then = clock.now();
    let delta = clock.delta_now(then);
    log::info!("delta_instant: {:?}", delta);
}

fn test_timer_irq() {
    log::info!("Testing timer interrupt...");

    let clock = arch::CLOCKSOURCE.read();
    let clock = clock.as_ref().unwrap();

    // Get the interrupt controller and timer
    let ic = crate::arch::INTERRUPT_CONTROLLER.read();
    let ic = ic.as_ref().unwrap();

    let timer = ic.timer();

    const TIMER_VECTOR: u8 = 0x30;

    timer.set_vector(TIMER_VECTOR);

    let then = clock.now();

    // Flag to indicate the interrupt has fired
    static IRQ_FIRED: AtomicBool = AtomicBool::new(false);

    // Store the start time in a static for the handler to access
    static mut START_TIME: Option<Instant> = None;

    #[allow(static_mut_refs)]
    fn timer_handler(_ctx: &mut InterruptContext) {
        unsafe {
            if let Some(start) = START_TIME.take() {
                let clock = &*core::ptr::addr_of!(arch::CLOCKSOURCE);
                let clock = clock.read();
                let clock = clock.as_ref().unwrap();
                let delta = clock.delta_now(start);
                log::info!("Timer interrupt fired! delta_now = {:?}", delta);
            }
            IRQ_FIRED.store(true, Ordering::SeqCst);
            let ic = crate::arch::INTERRUPT_CONTROLLER.read();
            ic.as_ref().unwrap().send_eoi();
        }
    }

    let _guard = register_interrupt_handler(TIMER_VECTOR, timer_handler);

    unsafe {
        START_TIME = Some(then);
        core::arch::asm!("sti");
    }

    timer.toggle();
    timer.next_tick(Duration::from_millis(1000));

    log::info!("Waiting for timer interrupt...");
    while !IRQ_FIRED.load(Ordering::SeqCst) {
        core::hint::spin_loop();
    }

    log::info!("Timer interrupt test complete");
}

fn kinit(boot_info: &'static mut BootInfo) {
    log::init();
    log::info!("Hello, zenos!");
    arch::init(boot_info);
}
