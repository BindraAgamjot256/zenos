#![no_std]
#![no_main]
#![feature(format_args_nl)]

use bitflags::bitflags;
use core::arch::global_asm;
use core::fmt::{self, Write};

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

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    println!("panic occurred: {}", _info);
    loop {}
}
unsafe extern "C" {
    fn _start() -> !;
}

global_asm!(
    r#"
.global _start
_start:
    sub rsp, 8      
    jmp main
    ud2
"#
);
#[unsafe(no_mangle)]
pub extern "C" fn main() -> ! {
    // File test
    let path = "/chksum.txt\0";
    let fd = unsafe { open(path.as_ptr(), FileOpenOptions::all().bits() as i32) };

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
            panic!("file_read failed. reality is pain");
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
            panic!("file_read failed. reality is pain");
        }
        let ret = unsafe { close(fd) };
        if ret != 0 {
            panic!("close failed. reality is pain");
        }
        println!("File closed.")
    } else {
        println!("file_open failed. reality is pain");
    }
    let mut stdin_buf = [0u8; 64];
    let ret = unsafe { read(0, stdin_buf.as_mut_ptr(), stdin_buf.len()) };
    if ret > 0 {
        println!(
            "Read {} bytes from stdin: {}",
            ret,
            core::str::from_utf8(&stdin_buf[..ret as usize]).unwrap_or("?")
        );
    } else {
        println!("stdin read failed. reality is pain");
    }
    panic!();
}
