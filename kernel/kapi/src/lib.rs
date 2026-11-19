#![cfg_attr(not(test), no_std)]
#![feature(coerce_unsized)]
#![feature(unsize)]
#![feature(ptr_metadata)]

extern crate alloc as std_alloc;

pub mod ffi;
#[macro_use]
pub mod macros;
pub mod log;
pub mod types;

pub use stabby;
pub use types::*;

#[cfg(feature = "driver")]
mod __rt_glue {
    use core::alloc::{GlobalAlloc, Layout};
    use core::fmt::{self, Write};
    use core::panic::PanicInfo;

    unsafe extern "C" {
        pub fn __kernel_panic_handler(
            msg_ptr: *const u8,
            msg_len: usize,
            file_ptr: *const u8,
            file_len: usize,
            line: u32,
        ) -> !;
        pub fn __kernel_alloc(size: usize, align: usize) -> *mut u8;
        pub fn __kernel_dealloc(ptr: *mut u8, size: usize, align: usize);
    }

    struct StackBuf {
        buf: [u8; 256],
        len: usize,
    }

    impl StackBuf {
        const fn new() -> Self {
            Self {
                buf: [0; 256],
                len: 0,
            }
        }

        fn as_bytes(&self) -> &[u8] {
            &self.buf[..self.len]
        }
    }

    impl Write for StackBuf {
        fn write_str(&mut self, s: &str) -> fmt::Result {
            let remaining = self.buf.len() - self.len;
            let bytes = s.as_bytes();
            let copy_len = bytes.len().min(remaining);

            // Prevent splitting a multi-byte UTF-8 character at the 256-byte boundary
            let valid_len = match core::str::from_utf8(&bytes[..copy_len]) {
                Ok(_) => copy_len,
                Err(e) => e.valid_up_to(),
            };

            self.buf[self.len..self.len + valid_len].copy_from_slice(&bytes[..valid_len]);
            self.len += valid_len;
            Ok(())
        }
    }

    #[panic_handler]
    fn panic(info: &PanicInfo) -> ! {
        let mut stack_buf = StackBuf::new();
        let _ = write!(stack_buf, "{}", info.message());
        let msg = stack_buf.as_bytes();

        let (file, line) = match info.location() {
            Some(loc) => (loc.file(), loc.line()),
            None => ("<unknown>", 0),
        };

        unsafe { __kernel_panic_handler(msg.as_ptr(), msg.len(), file.as_ptr(), file.len(), line) }
    }

    struct KernelAllocator;

    unsafe impl GlobalAlloc for KernelAllocator {
        unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
            unsafe { __kernel_alloc(layout.size(), layout.align()) }
        }

        unsafe fn dealloc(&self, ptr: *mut u8, layout: Layout) {
            unsafe { __kernel_dealloc(ptr, layout.size(), layout.align()) }
        }
    }

    #[global_allocator]
    static GLOBAL_ALLOCATOR: KernelAllocator = KernelAllocator;
}
