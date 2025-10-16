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

use alloc::boxed::Box;
use bootloader_api::{BootInfo, BootloaderConfig, config::Mapping, entry_point};
use core::arch::asm;
use fatfs::{Read, Write};
use zenos_kernel::{kinit, kprintln, serial_println};

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
#[cfg_attr(not(test), panic_handler)]
fn _panic(info: &core::panic::PanicInfo) -> ! {
    use log::error;
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
    #[cfg(debug_assertions)]
    if RUN_TESTSUITE {
        serial_println!("running {} test suites", zenos_kernel::TESTS.len());
        for i in zenos_kernel::TESTS.iter() {
            serial_println!("running {} tests", i.len());
            for j in i.iter() {
                j.run()
                    .expect("Test failed... FIX THE FUCKING TEST WILL YOU?"); // unnecessary to print here, since printing alr handled in the run impl
            }
        }
    } else {
        serial_println!("running tests disabled");
    }

    // Initialize kernel subsystems
    kinit(boot_info);
    serial_println!("allocating boxes");

    let the_box = Box::new(100u8);

    serial_println!("{:?}", the_box);
    drop(the_box);

    let vec = alloc::vec![1u8; 1000];
    serial_println!("{:?}", vec.len());

    drop(vec);

    let fs = zenos_kernel::fs::FS.lock();
    let mut binding = [0; 13];
    let buf = binding.as_mut_slice();
    fs.root_dir()
        .open_file("chksum.txt")
        .expect("chksum.txt exists")
        .read(buf)
        .expect("read failed");
    let buf = str::from_utf8(buf).unwrap();
    kprintln!("contents of chksum.txt: {buf}");

    kprintln!("writing \"fuck\" to chksum.txt");
    fs.root_dir()
        .open_file("chksum.txt")
        .expect("chksum.txt exists")
        .write(b"fuck")
        .expect("write failed");
    let mut binding = [0; 13];
    let buf = binding.as_mut_slice();
    fs.root_dir()
        .open_file("chksum.txt")
        .expect("chksum.txt exists")
        .read(buf)
        .expect("read failed");
    let buf = str::from_utf8(buf).unwrap();
    kprintln!("new contents of chksum.txt: {buf}");

    let buf = b"hello world\n";
    let len = buf.len();
    let ret: isize;
    unsafe {
        asm!(
        "syscall",
        in("rax") 1usize,            // syscall number: write
        in("rdi") 1usize,            // fd = 1 (stdout)
        in("rsi") buf.as_ptr(),      // buffer pointer
        in("rdx") len,               // buffer length
        lateout("rax") ret,          // syscall return -> rax
        out("rcx") _,                // syscall clobbers rcx
        out("r11") _,                // syscall clobbers r11
        options(nostack),            // we don't touch the stack here
        );
    }
    // Enter the main kernel loop
    // TODO: Implement proper scheduling and process management
    loop {
        x86_64::instructions::hlt(); // Halt the CPU until the next interrupt
    }
}

/// A constant that determines whether to run the test suite based on the current build configuration.
///
/// This constant evaluates to `true` when:
/// - The code is being compiled in a test context (`cfg!(test)`).
/// - OR the code is being compiled with debug assertions enabled (`cfg!(debug_assertions)`).
/// - AND the target architecture is `x86_64` (`cfg!(target_arch = "x86_64")`).
///
/// Otherwise, it evaluates to `false`.
///
/// This can be useful for conditionally enabling test-related functionality
/// or debugging logic only in compatible environments.
static RUN_TESTSUITE: bool = (cfg!(test) || cfg!(debug_assertions)) && cfg!(target_arch = "x86_64");
