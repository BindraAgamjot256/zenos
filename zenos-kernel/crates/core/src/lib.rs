//! Shared kernel facilities used by the kernel binary and hardware drivers.

#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
#![allow(macro_expanded_macro_exports_accessed_by_absolute_paths)]

extern crate alloc;

pub mod arch;
pub mod bus;
pub mod firmware;
pub mod irq;
pub mod log;
pub mod mm;
pub mod scheduler;
pub mod vmm;

pub use bootloader_api::BootInfo;
pub use device_core;
