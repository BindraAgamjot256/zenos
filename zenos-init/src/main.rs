#![no_std]
#![no_main]

#[panic_handler]
fn panic(info: &core::panic::PanicInfo) -> ! {
    loop {}
}

#[cfg(feature = "stress")]
#[unsafe(no_mangle)]
pub extern "C" fn main() -> ! {
    u64::MAX
}

#[cfg(not(feature = "stress"))]
#[unsafe(no_mangle)]
pub extern "C" fn main() -> u64 {
    u64::MAX
}
