#![no_std]
#![no_main]

use core::hint::spin_loop;


/* THIS function is a pain in ze ass */
#[cfg_attr(not(test), panic_handler)]
fn _panic(_info: &core::panic::PanicInfo) -> ! {
    loop {
        spin_loop(); 
    }
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
