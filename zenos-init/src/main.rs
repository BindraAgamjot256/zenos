#![no_std]
#![no_main]
#![feature(format_args_nl)]

use bitflags::bitflags;
use core::fmt::{self, Write};

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

unsafe extern "C" {
    fn open(path: *const u8, flags: i32) -> i32;
    fn close(fd: i32) -> i32;
    fn read(fd: i32, buf: *mut u8, count: usize) -> isize;
    fn write(fd: i32, buf: *const u8, count: usize) -> isize;
    fn lseek(fd: i32, offset: isize, whence: i32) -> isize;
}

// Console writer for println
struct Console;

impl Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        unsafe {
            write(1, s.as_ptr(), s.len());
        }
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

bitflags! {
    #[derive(Default, Debug, Clone, Copy)]
    pub struct FileOpenOptions: u64 {
        const READ = 0b0001;
        const WRITE = 0b0010;
        const CREATE = 0b0100;
        const TRUNCATE = 0b1000;
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // File test
    let path = "/chksum.txt\0";
    let fd = unsafe { open(path.as_ptr(), FileOpenOptions::all().bits() as i32) };
    println!("{:#?}", fd);

    if fd != -1 {
        // Move cursor back to start of file
        unsafe { lseek(fd, 0, 0) };
        let msg = "hello, world\n";
        unsafe { write(fd, msg.as_ptr(), msg.len()) };

        // Move cursor back to start of file
        unsafe { lseek(fd, 0, 0) };
        let mut buffer = [0u8; 32];
        let read_len = unsafe { read(fd, buffer.as_mut_ptr(), buffer.len()) };

        if read_len > 0 {
            println!(
                "File says: {}",
                core::str::from_utf8(&buffer[..read_len as usize]).unwrap_or("?")
            );
        } else {
            println!("file_read failed. reality is pain");
        }

        // Move cursor back to start of file
        unsafe { lseek(fd, 0, 0) };
        let buf = "Goodbye, world\n";
        unsafe { write(fd, buf.as_ptr(), buf.len()) };
        unsafe { lseek(fd, 0, 0) };
        let mut buffer = [0u8; 32];
        let read_len = unsafe { read(fd, buffer.as_mut_ptr(), buffer.len()) };
        if read_len > 0 {
            println!(
                "File says: {}",
                core::str::from_utf8(&buffer[..read_len as usize]).unwrap_or("?")
            );
        } else {
            println!("file_read failed. reality is pain");
        }
    } else {
        println!("file_open failed. reality is pain");
    }

    println!("type something and press enter...");

    // Read input from user
    let mut buf = [0u8; 64];
    let bytes_read = unsafe { read(0, buf.as_mut_ptr(), buf.len()) };
    let len = if bytes_read > 0 {
        bytes_read as usize
    } else {
        0
    };
    let input = core::str::from_utf8(&buf[..len]).unwrap_or("");
    println!("You typed: {}", input);
    loop {}
}
