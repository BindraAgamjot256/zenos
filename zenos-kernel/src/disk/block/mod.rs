//! Block device abstraction and drivers.
//!
//! This module defines a minimal [`BlockDevice`] trait and concrete drivers.
//! Concrete implementations live in submodules (e.g., [`ahci`]).
#![allow(dead_code)]
use crate::disk::vfs::SeekFrom;
use alloc::boxed::Box;
use alloc::sync::Arc;
use log::trace;
use spin::Mutex;

pub mod ahci;

/// Block device errors.
#[derive(Debug, Clone)]
pub enum BlockError {
    /// Read operation failed.
    ReadError,
    /// Write operation failed.
    WriteError,
    /// Seek operation failed or invalid position.
    SeekError,
    /// Operation not supported.
    UnsupportedOperation,
    /// Device not found or not available.
    DeviceNotFound,
}

/// Minimal abstraction for sector-addressable storage.
pub trait BlockDevice: Send + Sync {
    /// Logical block size in bytes (e.g., 512).
    fn block_size(&self) -> u64;
    /// Read bytes into `buf` starting at the current cursor.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, BlockError>;
    /// Write bytes from `buf` starting at the current cursor.
    fn write(&mut self, buf: &[u8]) -> Result<usize, BlockError>;
    /// Adjust the logical cursor position.
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, BlockError>;
    /// Flush any buffered writes.
    fn flush(&mut self) -> Result<(), BlockError>;
}

/// Thin adapter that wraps a concrete [`BlockDevice`] behind a trait object.
pub struct BlockDeviceDriver {
    device: Box<dyn BlockDevice>,
}

impl BlockDevice for BlockDeviceDriver {
    fn block_size(&self) -> u64 {
        self.device.block_size()
    }

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, BlockError> {
        trace!("BlockDeviceDriver: read {} bytes", buf.len());
        self.device.read(buf)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, BlockError> {
        trace!("BlockDeviceDriver: write {} bytes", buf.len());
        self.device.write(buf)
    }

    fn seek(&mut self, pos: SeekFrom) -> Result<u64, BlockError> {
        trace!("BlockDeviceDriver: seek to {:?}", pos);
        self.device.seek(pos)
    }

    fn flush(&mut self) -> Result<(), BlockError> {
        trace!("BlockDeviceDriver: flush");
        self.device.flush()
    }
}

impl BlockDeviceDriver {
    /// Create a new driver wrapper around a concrete [`BlockDevice`].
    pub fn new(device: Box<dyn BlockDevice>) -> Self {
        Self { device }
    }

    pub fn block_size(&self) -> u64 {
        self.device.block_size()
    }

    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, BlockError> {
        self.device.read(buf)
    }

    pub fn write(&mut self, buf: &[u8]) -> Result<usize, BlockError> {
        self.device.write(buf)
    }

    pub fn seek(&mut self, pos: SeekFrom) -> Result<u64, BlockError> {
        self.device.seek(pos)
    }

    pub fn flush(&mut self) -> Result<(), BlockError> {
        self.device.flush()
    }
}

// in preparation for a future with block devices accessible through /dev/**
impl<T> BlockDevice for Arc<Mutex<T>>
where
    T: BlockDevice,
{
    fn block_size(&self) -> u64 {
        self.lock().block_size()
    }

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, BlockError> {
        self.lock().read(buf)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, BlockError> {
        self.lock().write(buf)
    }

    fn seek(&mut self, pos: SeekFrom) -> Result<u64, BlockError> {
        self.lock().seek(pos)
    }

    fn flush(&mut self) -> Result<(), BlockError> {
        self.lock().flush()
    }
}
