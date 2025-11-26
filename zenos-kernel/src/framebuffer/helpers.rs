//! Framebuffer helper functions.
//!
//! Provides helper functionality to safely access and write to the framebuffer,
//! as well as other helpers, such as to update the cursor.

use crate::framebuffer::{FRAMEBUFFER, FrameBufferWriter};
use crate::serial_print;
use core::fmt::Write;
use log::error;

/// Executes a closure with a mutable reference to the framebuffer writer if available.
///
/// This function locks the framebuffer and, if a writer is present, applies the given closure.
///
/// # Parameters
/// - `f`: A closure that takes a mutable reference to a FrameBufferWriter.
pub(crate) fn with_writer<T>(f: impl FnOnce(&mut FrameBufferWriter) -> T) -> Result<T, ()> {
    x86_64::instructions::interrupts::without_interrupts(|| {
        let mut fb = FRAMEBUFFER.lock();
        if let Some(ref mut fb_writer) = *fb {
            Ok(f(fb_writer))
        } else {
            // If the framebuffer is not available, return an error.
            error!("Framebuffer not initialized or not available");
            Err(())
        }
    })
}

/// Internal helper to print formatted arguments to the framebuffer.
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
    with_writer(|fb| {
        // Write the formatted string to the framebuffer.
        fb.write_fmt(args).unwrap();
    })
    .unwrap_or_else(|_| {
        // If the framebuffer is not available, we can log to the serial port or panic.
        serial_print!("{}", args);
    });
}

/// Updates the cursor position in the framebuffer.
/// This function should be called after writing to the framebuffer
/// to ensure the cursor is positioned correctly for further writings.
pub fn update_cursor() {
    with_writer(|fb| {
        fb.update_cursor();
    })
    .unwrap_or_else(|_| {
        // If the framebuffer is not available, we can log to the serial port or panic.
        error!("Failed to update cursor: Framebuffer not available");
    });
}
