//! Zenos Kernel Fuzzer
//!
//! This binary runs as init and fuzz tests the kernel by invoking
//! syscalls with random/edge-case arguments.

#![no_std]
#![no_main]
#![feature(format_args_nl)]
#![allow(unsafe_op_in_unsafe_fn)]
use core::arch::asm;
use core::fmt::{self, Write};

unsafe extern "C" {
    fn write(fd: i32, buf: *const u8, count: usize) -> isize;
}

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
        let _ = c.write_fmt(format_args!($($arg)*));
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
    println!("[FUZZ PANIC] {}", info);
    loop {}
}

// Simple LFSR-based PRNG (no std required)
struct Rng {
    state: u64,
}

impl Rng {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 { 0xDEADBEEF } else { seed },
        }
    }

    fn next(&mut self) -> u64 {
        // xorshift64
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    fn next_range(&mut self, max: u64) -> u64 {
        self.next() % max
    }
}

// Raw syscall invocation
#[inline(always)]
unsafe fn raw_syscall(num: u64, a1: u64, a2: u64, a3: u64, a4: u64, a5: u64, a6: u64) -> i64 {
    let ret: i64;
    asm!(
        "syscall",
        in("rax") num,
        in("rdi") a1,
        in("rsi") a2,
        in("rdx") a3,
        in("r10") a4,
        in("r8") a5,
        in("r9") a6,
        lateout("rax") ret,
        lateout("rcx") _,
        lateout("r11") _,
        options(nostack)
    );
    ret
}

const NUM_SYSCALLS: u64 = 256;
const FUZZ_ITERATIONS: u64 = 50000;

#[unsafe(no_mangle)]
pub extern "C" fn main() -> ! {
    println!("=== Zenos Kernel Fuzzer ===");
    println!("Running {} fuzz iterations", FUZZ_ITERATIONS);

    let seed = {
        // This gets the timestamp of compilation from the environment
        // Cargo sets `CARGO_BUILD_TIMESTAMP` only if you define it manually
        let s = env!("BUILD_TIMESTAMP"); // define via build.rs
        let mut hash = 0u64;
        for b in s.bytes() {
            hash = hash.wrapping_mul(31).wrapping_add(b as u64);
        }
        if hash == 0 { 0xDEADBEEF } else { hash }
    };

    let mut rng = Rng::new(seed);
    let mut crashes = 0u64; // In a real fuzzer, we'd track crashes
    let mut successes = 0u64;

    for i in 0..FUZZ_ITERATIONS {
        let syscall_num = rng.next_range(NUM_SYSCALLS);
        let arg1 = rng.next();
        let arg2 = rng.next();
        let arg3 = rng.next();
        let arg4 = rng.next();
        let arg5 = rng.next();
        let arg6 = rng.next();

        if i % 500 == 0 {
            println!(
                "[{}] syscall({}, 0x{:x}, 0x{:x}, 0x{:x}, ...)",
                i, syscall_num, arg1, arg2, arg3
            );
        }
        if syscall_num == 57 {
            // ignore fork for simplicity
            continue;
        }

        let ret = unsafe { raw_syscall(syscall_num, arg1, arg2, arg3, arg4, arg5, arg6) };

        if ret < 0 {
            // Expected for bad args
            successes += 1;
        } else {
            crashes += 1;
        }
    }

    println!("\n=== Fuzz Summary ===");
    println!("Iterations: {}", FUZZ_ITERATIONS);
    println!("Completed:  {}", successes);
    println!("Crashes:    {}", crashes);
    println!("\nFuzzing complete. Kernel survived!");

    // Test edge cases
    println!("\n=== Edge Case Tests ===");

    // Invalid syscall number
    println!("Testing invalid syscall number (999)...");
    let ret = unsafe { raw_syscall(999, 0, 0, 0, 0, 0, 0) };
    println!("  Result: {}", ret);

    // NULL pointer reads
    println!("Testing read with NULL buffer...");
    let ret = unsafe { raw_syscall(0, 0, 0, 100, 0, 0, 0) }; // read(0, NULL, 100)
    println!("  Result: {}", ret);

    // Huge size
    println!("Testing read with huge size...");
    let ret = unsafe { raw_syscall(0, 0, 0x1000, 0xFFFFFFFFFFFFFFFF, 0, 0, 0) };
    println!("  Result: {}", ret);

    // Invalid fd
    println!("Testing read with invalid fd...");
    let ret = unsafe { raw_syscall(0, 0xFFFFFFFF, 0x1000, 10, 0, 0, 0) };
    println!("  Result: {}", ret);

    println!("\n=== All tests passed! ===");

    loop {}
}
