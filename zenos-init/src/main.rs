#![no_std]
#![no_main]

use core::arch::asm;
use core::fmt::{self, Write};

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    loop {}
}

// Console writer for println
struct Console;

impl Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        unsafe {
            let _ret: isize;
            asm!(
            "int 0x80",
            in("rax") 1usize,     // write
            in("rdi") 1usize,     // stdout
            in("rsi") s.as_ptr(),
            in("rdx") s.len(),
            lateout("rax") _ret,
            out("rcx") _,
            out("r11") _,
            );
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
        print!($($arg)*);
        print!("\n");
    })
}

macro_rules! readln {
    () => {{
        let mut buf = [0u8; 64];
        let mut bytes_read: isize;
        unsafe {
            asm!(
                "int 0x80",
                in("rax") 0usize,  // read
                in("rdi") 0usize,  // stdin
                in("rsi") buf.as_mut_ptr(),
                in("rdx") buf.len(),
                lateout("rax") bytes_read,
                out("rcx") _,
                out("r11") _,
            );
        }
        let len = if bytes_read > 0 { bytes_read as usize } else { 0 };
        (buf, len)
    }};
}

// ================================
// File I/O Macros
// ================================

macro_rules! file_open {
    ($path:expr) => {{
        let mut fd: usize;
        unsafe {
            asm!(
                "int 0x80",
                in("rax") 2usize,           // open
                in("rdi") $path.as_ptr(),   // path ptr
                in("rsi") $path.len(),      // length
                lateout("rax") fd,
                out("rcx") _,
                out("r11") _,
            );
        }
        fd
    }};
}

macro_rules! file_write {
    ($fd:expr, $data:expr) => {{
        unsafe {
            let _ret: isize;
            asm!(
                "int 0x80",
                in("rax") 1usize,               // write
                in("rdi") $fd,                  // fd
                in("rsi") $data.as_ptr(),       // buffer
                in("rdx") $data.len(),          // len
                lateout("rax") _ret,
                out("rcx") _,
                out("r11") _,
            );
        }
    }};
}

macro_rules! file_read {
    ($fd:expr, $buf:expr) => {{
        let mut bytes: isize;
        unsafe {
            asm!(
                "int 0x80",
                in("rax") 0usize,        // read
                in("rdi") $fd,
                in("rsi") $buf.as_mut_ptr(),
                in("rdx") $buf.len(),
                lateout("rax") bytes,
                out("rcx") _,
                out("r11") _,
            );
        }
        println!("read returned: {}", bytes);
        if bytes > 0 { bytes as usize } else { 0 }
    }};
}

// optional, only if your kernel supports
macro_rules! file_close {
    ($fd:expr) => {{
        unsafe {
            asm!(
                "int 0x80",
                in("rax") 3usize,   // close syscall maybe
                in("rdi") $fd,
            );
        }
    }};
}

#[unsafe(no_mangle)]
pub extern "C" fn _start() -> ! {
    // File test
    let path = "/chksum.txt";
    let fd = file_open!(path) + 2; // adjust for stdio fds
    println!("{:#?}", fd);

    if fd != usize::MAX {
        file_write!(fd, "hello, world\n");

        let mut buffer = [0u8; 32];
        let read_len = file_read!(fd, &mut buffer);
        if read_len > 0 {
            println!(
                "File says: {}",
                core::str::from_utf8(&buffer[..read_len]).unwrap_or("?")
            );
        } else {
            println!("file_read failed. reality is pain");
            loop {}
        }
    } else {
        println!("file_open failed. reality is pain");
    }

    println!("type something and press enter...");

    // Read input from user
    let (buf, len) = readln!();
    let input = core::str::from_utf8(&buf[..len]).unwrap_or("");
    println!("You typed: {}", input);
    loop {}
}
