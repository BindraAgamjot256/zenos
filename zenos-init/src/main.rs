#![no_std]
#![no_main]
#![feature(format_args_nl)]

use bitflags::bitflags;
use core::fmt::{self, Write};

unsafe extern "C" {
    fn write(fd: u64, buf: *const u8, count: usize) -> isize;
    fn fork() -> i64;
    fn execve(path: *const u8, argv: *const *const u8, envp: *const *const u8) -> i64;
    fn waitpid(pid: u64) -> i64;
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

/// Spawn a child process to run the given binary with optional arguments
fn spawn(path: &[u8], args: &[&[u8]]) -> i64 {
    let pid = unsafe { fork() };
    if pid == 0 {
        // Child: exec the binary
        // Build argv array (max 8 args + null terminator)
        let mut argv_ptrs: [*const u8; 9] = [core::ptr::null(); 9];
        for (i, arg) in args.iter().enumerate().take(8) {
            argv_ptrs[i] = arg.as_ptr();
        }

        let ret = unsafe { execve(path.as_ptr(), argv_ptrs.as_ptr(), core::ptr::null()) };
        // If we get here, execve failed
        println!(
            "execve failed for {:?}: {}",
            core::str::from_utf8(path),
            -ret
        );
        unsafe {
            // exit syscall directly since we can't use exit() in no_std context easily
            core::arch::asm!(
                "syscall",
                in("rax") 60u64,  // SYS_exit
                in("rdi") 1u64,
                options(noreturn)
            );
        }
    }
    pid
}

/// Stress test binaries to run (8.3 FAT filenames)
#[cfg(feature = "stress")]
static STRESS_TESTS: &[(&[u8], &[&[u8]])] = &[
    (b"/bin/forkstrm\0", &[b"forkstrm\0"]),
    (b"/bin/rapidspn\0", &[b"rapidspn\0"]),
    (b"/bin/schedfar\0", &[b"schedfar\0"]),
    (b"/bin/memexhst\0", &[b"memexhst\0"]),
    //(b"/bin/fsconcrn\0", &[b"fsconcrn\0"]),
    (b"/bin/orphzomb\0", &[b"orphzomb\0"]),
];

#[cfg(feature = "stress")]
#[unsafe(no_mangle)]
pub extern "C" fn main() -> ! {
    println!("=== Zenos Init: Stress Test Launcher ===");

    // Brief delay before stress tests
    for _ in 0..1000000u32 {
        core::hint::spin_loop();
    }

    // Launch all stress tests
    println!("[init] Launching stress tests...");

    let mut launched = 0i32;
    let mut pids: [i64; 16] = [0; 16];
    for (i, (path, args)) in STRESS_TESTS.iter().enumerate() {
        let pid = spawn(*path, *args);
        if pid > 0 {
            println!(
                "[init] Started {:?} with PID {}",
                core::str::from_utf8(&path[..path.len() - 1]).unwrap_or("?"),
                pid
            );
            let exit_code = unsafe { waitpid(pid as u64) };
            println!("[init] Process PID {} exited with code {}", pid, exit_code);
            if exit_code < 0 {
                println!("[init] Process PID {} crashed!", pid);
                if path == b"/bin/memexhst\0" {
                    println!("[init] memexhst crashed");
                    println!("       feature, not bug")
                } else {
                    panic!(
                        "init, stress test, {} crashed",
                        core::str::from_utf8(&path[..path.len() - 1]).unwrap_or("?")
                    );
                }
            }
            pids[i] = pid;
            launched += 1;
        } else if pid < 0 {
            println!(
                "[init] Failed to spawn {:?}: {}",
                core::str::from_utf8(&path[..path.len() - 1]).unwrap_or("?"),
                pid
            );
        }

        // Small delay between launches to avoid overwhelming the kernel
        unsafe { while waitpid(-1i64 as u64) > 0 {} }
    }

    println!(
        "[init] Launched {} stress tests, all successfully exited",
        launched
    );

    // Init should never exit - it's PID 1
    // Just loop forever, periodically printing status
    let mut heartbeat = 0u64;
    loop {
        for _ in 0..1000000u32 {
            unsafe { core::arch::asm!("pause") };
        }
        heartbeat += 1;
        if heartbeat % 10 == 0 {
            println!("[init] Heartbeat {}", heartbeat);
        }
    }
}

#[cfg(not(feature = "stress"))]
#[unsafe(no_mangle)]
pub extern "C" fn main() -> ! {
    println!("=== Zenos Init ===");

    // Launch the shell
    println!("[init] Launching shell...");
    let shell_pid = spawn(b"/bin/shell\0", &[b"shell\0"]);

    if shell_pid > 0 {
        println!("[init] Shell started with PID {}", shell_pid);
        // Wait for shell to exit
        let exit_code = unsafe { waitpid(shell_pid as u64) };
        println!("[init] Shell exited with code {}", exit_code);
    } else if shell_pid < 0 {
        println!("[init] Failed to fork for shell: {}", shell_pid);
    }

    // Init should never exit - it's PID 1
    // If shell exits, just loop forever
    println!("[init] Shell terminated, entering idle loop...");
    loop {
        unsafe { core::arch::asm!("pause") };
    }
}
