#![no_std]
#![no_main]
#![feature(format_args_nl)]

use bitflags::bitflags;
use core::fmt::{self, Write};

unsafe extern "C" {
    fn open(path: *const u8, flags: u64) -> isize;
    fn close(fd: u64) -> u64;
    fn read(fd: u64, buf: *mut u8, count: usize) -> isize;
    fn write(fd: *const u8, buf: *const u8, count: usize) -> isize;
    fn exit(code: u64) -> !;
}

// Simple console writer
struct Console;

impl Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let _ = unsafe { write(1 as *const u8, s.as_ptr(), s.len()) };
        Ok(())
    }
}

macro_rules! print {
    ($($arg:tt)*) => ({
        let mut c = Console;
        c.write_fmt(format_args!($($arg)*)).unwrap();
    });
}

macro_rules! println {
    () => (print!("\n"));
    ($($arg:tt)*) => ({
        print!("{}", format_args_nl!($($arg)*));
    })
}

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    println!("panic: {}", info);
    loop {}
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct FileOpenOptions: u64 {
        // Access modes (mutually exclusive)
        const READ_ONLY  = 0; // O_RDONLY
        const WRITE_ONLY = 1; // O_WRONLY
        const READ_WRITE = 2; // O_RDWR

        // Flags
        const CREATE        = 0o100;      // O_CREAT
        const EXCLUSIVE     = 0o200;      // O_EXCL
        const NOCTTY        = 0o400;      // O_NOCTTY
        const TRUNCATE      = 0o1000;     // O_TRUNC
        const APPEND        = 0o2000;     // O_APPEND
        const NONBLOCK      = 0o4000;     // O_NONBLOCK
        const SYNC          = 0o10000;    // O_SYNC
        const CLOSE_ON_EXEC = 0o2000000;  // O_CLOEXEC
    }

}

#[unsafe(no_mangle)]
pub extern "C" fn main(argc: usize, argv: *const *const u8) -> ! {
    // Display command line arguments
    println!("=== Command Line Arguments ===");
    println!("argc = {}", argc);

    if !argv.is_null() {
        for i in 0..argc {
            let arg_ptr = unsafe { *argv.add(i) };
            if !arg_ptr.is_null() {
                // Find the length of the C string
                let mut len = 0;
                unsafe {
                    while *arg_ptr.add(len) != 0 {
                        len += 1;
                    }
                }
                let arg_slice = unsafe { core::slice::from_raw_parts(arg_ptr, len) };
                if let Ok(s) = core::str::from_utf8(arg_slice) {
                    println!("argv[{}] = \"{}\"", i, s);
                } else {
                    println!("argv[{}] = [invalid utf-8]", i);
                }
            } else {
                println!("argv[{}] = (null)", i);
            }
        }
    }
    println!("==============================");
    println!();
    let mut i = 0u64;
    while i < 10000 {}

    // List of files we want to dump
    let files = [
        b"/proc/cpuinfo\0\0\0",
        b"/proc/meminfo\0\0\0",
        b"/proc/version\0\0\0",
        b"/proc/uptime\0\0\0\0",
        b"/proc/1/cmdline\0",
        b"/proc/1/status\0\0",
        b"/proc/1/stat\0\0\0\0",
    ];

    let mut buf = [0u8; 1024];

    for file in &files {
        let fd = unsafe {
            open(
                file.as_ptr(),
                (FileOpenOptions::READ_WRITE | FileOpenOptions::CREATE).bits(),
            )
        };
        if fd < 0 {
            println!(
                "Failed to open {}",
                core::str::from_utf8(&file[..file.len() - 1]).unwrap()
            );
            continue;
        }

        println!(
            "--- {} ---",
            core::str::from_utf8(&file[..file.len() - 1]).unwrap()
        );

        loop {
            let n = unsafe { read(fd as u64, buf.as_mut_ptr(), buf.len()) };
            if n <= 0 {
                break;
            }
            let slice = &buf[..n as usize];
            let s = core::str::from_utf8(slice).unwrap_or("[Invalid UTF-8]");
            print!("{}", s);
        }

        unsafe { close(fd as u64) };
        println!();
    }
    unsafe { exit(0) };
}
