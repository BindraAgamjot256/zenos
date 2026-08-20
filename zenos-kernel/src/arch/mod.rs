//! Architecture-specific code for the Zenos kernel.
//!
//! This module provides abstractions and implementations for architecture-dependent functionality.
//! Currently, only x86_64 is supported.
//!
//! The module includes:
//! - `addr`: Physical and virtual address types with validation
//! - `mem`: Memory management including frame allocation and paging
//! - `ports`: Low-level I/O port access for hardware communication
//! - `serial`: Serial port driver for debugging and early-stage output

#[cfg(target_arch = "x86_64")]
mod x86_64;

#[cfg(target_arch = "x86_64")]
pub use x86_64::*;

#[cfg(not(target_arch = "x86_64"))]
compile_error!("Unsupported architecture");

#[derive(Debug)]
pub enum MemMapErr {
    Uninit,
    AlreadyMapped,
    ParentHugePage,
}
