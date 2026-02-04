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

extern crate alloc;

use bootloader_api::{BootInfo, BootloaderConfig, config::Mapping, entry_point};
use core::arch::asm;
use zenos_kernel::memory::PAGE_4K;
use zenos_kernel::{kinit, serial_println};

static CONFIG: BootloaderConfig = {
    let mut config = BootloaderConfig::new_default();
    config.mappings.physical_memory = Some(Mapping::FixedAddress(
        zenos_kernel::memory::constants::HIGHER_HALF_BASE,
    ));
    config.mappings.boot_info = Mapping::Dynamic;
    config.mappings.kernel_base =
        Mapping::FixedAddress(zenos_kernel::memory::constants::KERNEL_BASE); // higher-half base + 0x5000_0000
    config.mappings.kernel_stack =
        Mapping::FixedAddress(zenos_kernel::memory::constants::KERNEL_STACK_BASE); // higher-half base + 0x1_0000_0000
    config.mappings.framebuffer =
        Mapping::FixedAddress(zenos_kernel::memory::constants::KERNEL_FB_MAPPINGS);
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
    use log::error;
    zenos_kernel::print_stack_trace();
    error!("Kernel Panic: {info}");
    // Halt the CPU
    unsafe {
        asm!(
            "
    4:
        cli; hlt
        jmp 4b",
            options(nomem, preserves_flags, nostack, noreturn)
        )
    }
}

// Defines the kernel main function as the entry point and adds metadata for the bootloader.
#[cfg(not(feature = "run-kunittest"))]
entry_point!(kmain, config = &CONFIG);
#[cfg(feature = "run-kunittest")]
entry_point!(ktest_main, config = &CONFIG);

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
    // Initialize kernel subsystems
    kinit(boot_info);

    let (free, total) = zenos_kernel::memory::get_stats().unwrap();
    let used = total - free;
    serial_println!("Pages used: {} / {}", used, total);
    serial_println!("Memory used: {} KiB", used * PAGE_4K / 1024);
    #[cfg(feature = "test_stub")]
    {
        compile_error!("Test stubs are unsupported, testing done through kunittest feature, and through stress testing.");
    }

    let buf = zenos_kernel::process::init_process();

    let (entry, stack, pid) = {
        let mut processes = zenos_kernel::process::PROCESSES.lock();
        let pinit = &mut processes[0];
        pinit.load(buf);
        let (e, s) = pinit.prepare_run().unwrap();
        (e, s, pinit.pid)
    };

    // Tell the scheduler which process is currently running
    {
        let mut sched = zenos_kernel::process::SCHEDULER.lock();
        sched.set_current(pid);
    }

    zenos_kernel::process::enter_user_mode(entry, stack);
}

#[cfg(feature = "run-kunittest")]
fn ktest_main(bi: &'static mut BootInfo) -> ! {
    use crate::testing_stuff::{QemuExitCode, exit_qemu};
    use log::LevelFilter;
    use zenos_kernel::serial_println;
    use zenos_kernel::testing::Testable;
    kinit(bi); // idk if i should do this... seems fine i guess...
    log::set_max_level(LevelFilter::Off);
    serial_println!(
        "running {} tests",
        zenos_kernel::TESTS.iter().filter(|t| t.is_some()).count()
    );
    let mut failed = false;
    for i in zenos_kernel::TESTS.iter() {
        if i.is_some() {
            if let Err(()) = i.unwrap().run() {
                failed = true;
            }
        }
    }
    if failed {
        exit_qemu(QemuExitCode::Failed);
    }
    exit_qemu(QemuExitCode::Success);
    loop {}
}

#[cfg(feature = "run-kunittest")]
mod testing_stuff {
    use zenos_kernel::serial_println;

    #[derive(Debug, Clone, Copy, PartialEq, Eq)]
    #[repr(u32)]
    pub enum QemuExitCode {
        Success = 0x10,
        Failed = 0x11,
    }

    pub fn exit_qemu(exit_code: QemuExitCode) {
        use x86_64::instructions::port::Port;

        unsafe {
            let mut port = Port::new(0xf4);
            port.write(exit_code as u32);
        }
    }

    #[panic_handler]
    fn panic(info: &core::panic::PanicInfo) -> ! {
        use crate::testing_stuff::{QemuExitCode, exit_qemu};
        serial_println!("KERNEL PANIC DURING UNIT TESTS: {}", info);
        exit_qemu(QemuExitCode::Failed);
        loop {}
    }
}
