//! zenos kernel library.
//!
//! This is the main library crate for the zenos kernel. It provides core functionality
//! and interfaces for the operating system, including framebuffer handling for display output,
//! the core logging framework, memory management, and hardware-related functionality.
//!
//! ## Features
//!
//! - **Serial output** - The kernel supports serial output via the `serial` module.
//! - **Framebuffer output** - The kernel supports framebuffer output via the `framebuffer` module.
//! - **Logging** - The kernel supports logging via the `log` crate.
//! - **Memory management** - The kernel supports memory management via the `memory` module.
//! - **Hardware support** - The kernel supports hardware-related functionality via the `hardware` module.
//! - **Interrupts** - The kernel supports interrupts via the `interrupts` module.
//!
//! The crate runs in a `no_std` environment, as it's designed to operate without the standard library
//! on bare metal.
//!
//! ## Note about pronunciation
//! the name "zenos" is pronounced as one word, like in zeno's paradox, but with more emphasis on the s.
//! The name is not pronounced as "zen os" (like "zen operating system").\
//!

#![no_std]
#![feature(abi_x86_interrupt)]
#![feature(cold_path)]
#![feature(never_type)]
#![allow(unsafe_op_in_unsafe_fn)] // rustc 2024 doesn't allow unsafe ops in unsafe functions, so we enable it manually
#![deny(static_mut_refs)] // to be replaced later with deny... for now only.
#![warn(clippy::missing_safety_doc)]

extern crate alloc;

pub use crate::framebuffer::helpers::*;
use crate::testing::Testable;
use ::acpi::InterruptModel;
use bootloader_api::info::MemoryRegionKind;
use bootloader_api::{BootInfo, info::MemoryRegion};
use core::hint::cold_path;
use embedded_graphics::{draw_target::DrawTarget, pixelcolor::Rgb888};
use heapless::Vec;
use log::{debug, error, info, trace, warn};
use x86_64::VirtAddr;

/// ACPI Module for handling ACPI-related functionality. (currently only hosts acpi handler for acpi crate)
pub mod acpi;
/// Architecture-specific code(only does inb, outb etc., as os is designed for x86_64)
mod arch;
/// Framebuffer module for display output
pub(crate) mod framebuffer;
pub mod fs;
/// Hardware module for handling hardware-related functionality
pub mod hardware;
/// Interrupts module for handling interrupts
pub mod interrupts;
/// Memory module for handling memory-related functionality
pub mod memory;
mod pci;
pub mod process;
/// Serial module for logging output
pub mod serial;
/// Syscalls module for handling system calls
pub mod syscall;
/// Testing module for testing functionality
pub mod testing;
/// Time module for handling time-related functionality
pub mod time;

#[cfg(not(target_arch = "x86_64"))]
compile_error!("zenos only supports x86_64");

pub static TESTS: &[&[&(dyn Testable + Sync)]] = {
    if cfg!(test) || cfg!(debug_assertions) {
        &[framebuffer::TESTS, memory::TESTS, testing::TESTS]
    } else {
        &[]
    }
};

/// Initializes the kernel with essential parts.
///
/// This function is called early in the boot process to set up critical kernel
/// subsystems, including the framebuffer for display output, the logger and serial output, the
/// memory subsystem for memory management, etc.
///
/// # Parameters
///
/// - `boot_info` - Boot information provided by the bootloader, containing
///   details about system memory, framebuffer, and other interrupts configurations
///
/// # Note
///
/// This function expects to be called only once during system initialization.
#[track_caller]
pub fn kinit(boot_info: &'static mut BootInfo) {
    log::set_logger(&serial::LOGGER).expect("PANIK");

    #[cfg(debug_assertions)]
    {
        log::set_max_level(log::LevelFilter::Debug); // do not use trace unless you have half an hour to spare....
    }

    #[cfg(not(debug_assertions))]
    {
        log::set_max_level(log::LevelFilter::Warn);
    }

    info!("Kernel initialization started");

    // Initialize the framebuffer if available
    trace!("Initializing framebuffer");
    let framebuffer = boot_info.framebuffer.as_mut();
    if let Some(framebuffer) = framebuffer {
        let info = framebuffer.info();
        let buffer = framebuffer.buffer_mut();
        let mut fb_writer = framebuffer::FrameBufferWriter::new(buffer, info);
        debug!(
            "Framebuffer initialized, size: {}x{}",
            info.width, info.height
        );
        fb_writer.clear(Rgb888::new(0, 0, 0)).unwrap();
        framebuffer::FRAMEBUFFER.lock().replace(fb_writer);
    } else {
        cold_path(); // unlikely branch, fallback to serial-only
        warn!("Framebuffer is absent, falling back to serial logging");
    }

    info!("starting memory initialization");

    let merged_regions = merge_contiguous_regions(&mut boot_info.memory_regions);

    let merged_regions_iter = merged_regions
        .iter()
        .filter(|region| region.kind == MemoryRegionKind::Usable) // temporary
        .map(|region| (region.start, (region.end - region.start) as usize));

    trace!("merged regions: {merged_regions_iter:#?}");
    memory::init(
        VirtAddr::new(0),
        VirtAddr::new(memory::constants::HIGHER_HALF_BASE),
        0usize,
        merged_regions_iter,
    );

    // Initialize the slab allocator after page allocator is ready
    info!("Initializing slab allocator");
    memory::alloc::init();
    info!("Slab allocator initialized");

    info!("Initialising interrupts");
    interrupts::init_idt();
    interrupts::gdt::init_gdt();
    trace!("gdt: {:#?}", interrupts::gdt::GDT);
    info!("Interrupts initialized");

    info!("Initializing Hardware interrupts");
    trace!("reading ACPI tables for APIC base");

    let rsdp = *boot_info.rsdp_addr.as_mut().unwrap();
    let acpi_tables =
        unsafe { ::acpi::AcpiTables::from_rsdp(acpi::AcpiHandler, rsdp as usize) }.unwrap();
    let platform_info = acpi_tables.platform_info().unwrap();
    let interrupt_model = platform_info.interrupt_model.clone();
    trace!("interrupt model: {interrupt_model:#?}");

    let mut isr_overrides = None;

    trace!("initializing APIC");
    let mut apic_info = match interrupt_model {
        InterruptModel::Apic(a) => {
            let base = a.local_apic_address;
            trace!("APIC base: {base:#x}");

            let mut ios = Vec::<u64, { hardware::MAX_IOAPICS }>::new();

            let slice = a.io_apics;
            for (i, ioapic) in slice.iter().enumerate() {
                trace!("IOAPIC {i}: {ioapic:#?}");
                let _ = ios.push(ioapic.address as u64);
            }
            isr_overrides.replace(Vec::<(u64, u64), 128>::new());

            trace!("ISR Overrides: {:#?}", a.interrupt_source_overrides);
            for i in a.interrupt_source_overrides.iter() {
                let _ = isr_overrides
                    .as_mut()
                    .unwrap()
                    .push((i.isa_source as u64, i.global_system_interrupt as u64)); // 128 isr overrides is unlikely if i do say so myself...
                trace!(
                    "ISR Override: {} -> {}",
                    i.isa_source, i.global_system_interrupt
                )
            }

            (base, ios)
        }
        _ => (0xFEE00000u64, Vec::new()),
    };
    hardware::init(
        apic_info.0,
        apic_info.1.as_mut_slice(),
        isr_overrides.unwrap_or(Vec::new()).as_mut_slice(),
    );

    debug!("Enabling syscalls");
    syscall::init();

    debug!("Kernel initialization complete");
    debug!("everything initialized, enabling interrupts now");
    x86_64::instructions::interrupts::enable();
    debug!("Interrupts enabled");
}

fn merge_contiguous_regions(regions: &mut [MemoryRegion]) -> &mut [MemoryRegion] {
    // todo
    // for now, just return the regions as is
    regions
}

/// Prints the current stack trace by walking frame pointers
pub fn print_stack_trace() {
    use x86_64::VirtAddr;

    error!("Stack trace:");

    unsafe {
        let mut rbp: u64;
        core::arch::asm!("mov {}, rbp", out(reg) rbp);

        let mut frame_num = 0;
        error!("  #{}: <entry>", frame_num);
        while rbp != 0 && frame_num < 64 {
            let return_addr_ptr = (rbp + 8) as *const u64;
            if return_addr_ptr.is_null() {
                error!("  #{}: <null>", frame_num);
                break;
            }

            let return_addr = *return_addr_ptr;
            if return_addr == 0 {
                error!("  #{}: <null>", frame_num);
                break;
            }

            // Stop if we’ve crossed into userland
            // Typical split: user < 0x0000800000000000 (or 0x00007fffffffffff)
            if return_addr < 0xffff800000000000 {
                error!(
                    "  #{}: {:#018x} <userland, stopping>",
                    frame_num, return_addr
                );
                break;
            }

            error!("  #{}: {:#018x}", frame_num, return_addr);

            let next_rbp_ptr = rbp as *const u64;
            let next_rbp = *next_rbp_ptr;

            if next_rbp <= rbp {
                error!("  #{}: <recursion>", frame_num);
                break;
            }

            rbp = next_rbp;
            frame_num += 1;
        }
    }
}
