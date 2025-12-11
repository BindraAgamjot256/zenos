//! Block device abstraction and drivers.
//!
//! This module defines a minimal `BlockDevice` trait and a thin `BlockDeviceDriver`
//! wrapper that adapts concrete drivers to the `fatfs` I/O traits. Concrete
//! implementations live in submodules (e.g., `ahci`).
//!
//! The trait is deliberately small and mirrors the needs of `fatfs` (read, write,
//! seek, error type via `IoBase`) with an extra `block_size()` query used by the
//! kernel in a few places.
//!
//! Typical usage:
//! - Instantiate a concrete device (e.g., `AhciBlockDevice`), wrap it in
//!   `BlockDeviceDriver`, then hand it to the filesystem layer.
//! - End users should not depend on `ahci` specifics; work against the trait.

use alloc::boxed::Box;
use fatfs::{IoBase, Read, Seek, SeekFrom, Write};

pub mod ahci;

// ============================================================================
// Block Device Implementation
// ============================================================================
/// Minimal abstraction for sector-addressable storage.
pub trait BlockDevice: Read + Write + Seek + IoBase {
    /// Logical block size in bytes (e.g., 512).
    fn block_size(&self) -> u64;
}

/// Thin adapter that implements the `fatfs` traits by delegating to an inner
/// `BlockDevice` object. This indirection allows using trait objects behind a
/// `Box` while satisfying `fatfs`'s concrete type requirements.
pub struct BlockDeviceDriver<E> {
    device: Box<dyn BlockDevice<Error=E> + Send + Sync>,
}

impl<E> BlockDeviceDriver<E> {
    /// Create a new driver wrapper around a concrete `BlockDevice`.
    pub fn new(device: Box<dyn BlockDevice<Error=E> + Send + Sync>) -> Self {
        Self { device }
    }
}

impl<E: fatfs::IoError> IoBase for BlockDeviceDriver<E> {
    type Error = E;
}

impl<E: fatfs::IoError> Read for BlockDeviceDriver<E> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        self.device.read(buf)
    }
}

impl<E: fatfs::IoError> Write for BlockDeviceDriver<E> {
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        self.device.write(buf)
    }

    fn flush(&mut self) -> Result<(), Self::Error> {
        self.device.flush()
    }
}

impl<E: fatfs::IoError> Seek for BlockDeviceDriver<E> {
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
        self.device.seek(pos)
    }
}
