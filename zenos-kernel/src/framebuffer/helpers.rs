//! Framebuffer helper functions.
//!
//! Provides helper functionality for printing to the TTY.

use crate::serial_print;
use crate::tty::TTY;
use core::fmt::Write;

/// Internal helper to print formatted arguments to the TTY.
///
/// # Parameters
/// - `args`: The format arguments to be printed.
///
/// # Example
/// ```rust
/// zenos_kernel::kprintln!("Goodbye, World!");
/// ```
/// This function should not be called directly; instead, use the `kprint`/`kprintln!` macro.
pub fn _print(args: core::fmt::Arguments) {
    unsafe {
        TTY.force_unlock();
    }
    let mut tty = TTY.lock();
    if let Some(ref mut tty_device) = *tty {
        tty_device.write_fmt(args).unwrap();
    } else {
        // TTY not initialized, fall back to serial
        serial_print!("{}", args);
    }
}
