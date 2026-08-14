//! Zenos Kernel Fuzzer
//!
//! This binary runs as init and fuzz tests the kernel by invoking
//! syscalls with random/edge-case arguments.

#![no_std]
#![no_main]

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[unsafe(no_mangle)]
pub extern "C" fn main() -> u64 {
   -1i64 as u64
}
