#![no_std]
#![no_main]
#![feature(format_args_nl)]

use bitflags::bitflags;
use core::fmt::{self, Write};

unsafe extern "C" {
    fn open(path: *const u8, flags: u64) -> isize;
    fn close(fd: u64) -> u64;
    fn read(fd: u64, buf: *mut u8, count: usize) -> isize;
    fn write(fd: u64, buf: *const u8, count: usize) -> isize;
    fn lseek(fd: u64, offset: isize, whence: u64) -> isize;
    fn fork() -> i64;
    fn execve(path: *const u8, argv: *const *const u8, envp: *const *const u8) -> i64;
}

// Console writer for println
struct Console;

impl Write for Console {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        let err = unsafe { write(1, s.as_ptr(), s.len()) };
        if err < 0 {
            return Err(fmt::Error);
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

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo) -> ! {
    println!("panic occurred: {}", _info);
    loop {}
}
unsafe extern "C" {
    fn _start() -> !;
}

#[unsafe(no_mangle)]
pub extern "C" fn main() -> ! {
    // File test
    let path = "/chksum.txt\0";
    let fd = unsafe {
        open(
            path.as_ptr(),
            (FileOpenOptions::CREATE | FileOpenOptions::READ_WRITE).bits(),
        )
    };

    if fd >= 0 {
        // Move cursor back to start of file
        let fd = fd as u64;
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
            panic!("file_read failed. reality is pain, err:{}", -read_len);
        }
        let ret = unsafe { close(fd) };
        if ret != 0 {
            panic!("close failed. reality is pain");
        }
        println!("File closed.")
    } else {
        println!("file_open failed. reality is pain, err:{}", fd);
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
    let err = unsafe { fork() };
    if err > 0 {
        // Parent
        println!("Hello from the parent process! Child PID: {}", err);
        loop {}
    } else if err == 0 {
        // Child
        println!("Hello from the child process!, fork returned: {}", err);

        // Build argv array: ["dump.elf", "hello", "from", "init", NULL]
        let arg0 = "dump.elf\0";
        let arg1 = "hello\0";
        let arg2 = "from\0";
        let arg3 = "init\0";
        let argv: [*const u8; 5] = [
            arg0.as_ptr(),
            arg1.as_ptr(),
            arg2.as_ptr(),
            arg3.as_ptr(),
            core::ptr::null(),
        ];

        let ret = unsafe { execve("/bin/dump.elf\0".as_ptr(), argv.as_ptr(), core::ptr::null()) };
        if ret != 0 {
            panic!("execve failed. reality is pain, err:{}", -ret);
        }
        unreachable!()
    } else {
        println!("Fork failed with error code: {}", err);
    }
    unreachable!()
}
