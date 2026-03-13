//! Block device abstraction and drivers.
//!
//! This module provides the low-level storage abstraction layer, defining
//! the [`BlockDevice`] trait for sector-addressable storage devices.
//!
//! # Overview
//!
//! Block devices provide byte-level access with an internal cursor, abstracting
//! away the underlying sector-based nature of storage hardware. The trait
//! supports read, write, seek, and flush operations.
//!
//! # Components
//!
//! - [`BlockDevice`]: Core trait for storage devices with cursor-based I/O.
//! - [`BlockDeviceDriver`]: Wrapper that boxes a concrete [`BlockDevice`]
//!   implementation for use as a trait object.
//! - [`BlockError`]: Error type for block-level operations.
//!
//! # Implementations
//!
//! - [`ahci::AhciBlockDevice`]: SATA storage via AHCI controller using DMA.
// #![allow(dead_code)]
use crate::disk::vfs::SeekFrom;
use alloc::boxed::Box;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use log::{error, trace};
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
    /// Invalid GPT header or partition table.
    InvalidGPT,
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
    /// read exact number of bytes into `buf` starting at the current cursor.
    fn read_exact(&mut self, buf: &mut [u8]) -> Result<usize, BlockError> {
        let mut total_read = 0;
        while total_read < buf.len() {
            match self.read(&mut buf[total_read..]) {
                Ok(0) => break, // EOF
                Ok(n) => total_read += n,
                Err(e) => return Err(e),
            }
        }
        if total_read == buf.len() {
            Ok(total_read)
        } else {
            Err(BlockError::ReadError)
        }
    }

    /// Get GPT header
    fn get_header(&mut self) -> Result<GPTHeader, BlockError> {
        let block_size = self.block_size() as usize;
        let mut buf = vec![0u8; block_size];
        self.seek(SeekFrom::Start(1 * block_size as u64))?;
        self.read_exact(&mut buf)?;
        GPTHeader::deserialize(&buf).ok_or(BlockError::InvalidGPT)
    }
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
}

// in preparation for a future with block devices accessible through /dev/**
impl<T> BlockDevice for Arc<Mutex<T>>
where
    T: BlockDevice + ?Sized,
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

impl BlockDevice for Box<dyn BlockDevice> {
    fn block_size(&self) -> u64 {
        (**self).block_size()
    }

    fn read(&mut self, buf: &mut [u8]) -> Result<usize, BlockError> {
        (**self).read(buf)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, BlockError> {
        (**self).write(buf)
    }

    fn seek(&mut self, pos: SeekFrom) -> Result<u64, BlockError> {
        (**self).seek(pos)
    }

    fn flush(&mut self) -> Result<(), BlockError> {
        (**self).flush()
    }
}

#[derive(Debug, Clone)]
pub struct GPTHeader {
    pub signature: [u8; 8],
    pub rev: u32,
    pub header_size: u32,
    pub crc32: u32,
    pub reserved: u32,
    pub current_lba: u64,
    pub backup_lba: u64,
    pub first_usable_lba: u64,
    pub last_usable_lba: u64,
    pub disk_guid: [u8; 16],
    pub partition_entry_lba: u64,
    pub num_partition_entries: u32,
    pub size_of_partition_entry: u32,
    pub partition_entry_array_crc32: u32,
}

#[derive(Debug, Clone)]
pub struct GPTPartitionEntry {
    pub partition_type_guid: [u8; 16],
    pub unique_partition_guid: [u8; 16],
    pub starting_lba: u64,
    pub ending_lba: u64,
    pub attributes: u64,
    pub partition_name: [u16; 36], // UTF-16LE encoded
}

impl GPTPartitionEntry {
    pub fn deserialize(buf: &[u8]) -> Option<Self> {
        if buf.len() < 128 {
            error!("GPT partition entry too small: {} bytes", buf.len());
            return None;
        }

        let partition_type_guid: [u8; 16] = buf.get(0..16)?.try_into().ok()?;
        let unique_partition_guid: [u8; 16] = buf.get(16..32)?.try_into().ok()?;
        let starting_lba = u64::from_le_bytes(buf.get(32..40)?.try_into().ok()?);
        let ending_lba = u64::from_le_bytes(buf.get(40..48)?.try_into().ok()?);
        let attributes = u64::from_le_bytes(buf.get(48..56)?.try_into().ok()?);
        let f = |chunk: &[u8]| u16::from_le_bytes(chunk.try_into().unwrap());
        let partition_name: [u16; 36] = buf
            .get(56..128)?
            .chunks_exact(2)
            .map(f)
            .collect::<Vec<u16>>()
            .try_into()
            .ok()?;

        Some(Self {
            partition_type_guid,
            unique_partition_guid,
            starting_lba,
            ending_lba,
            attributes,
            partition_name,
        })
    }
    pub fn is_used(&self) -> bool {
        self.partition_type_guid != [0u8; 16]
    }
}
#[inline(always)]
fn crc32(data: &[u8]) -> u32 {
    const CRC32_POLY: u32 = 0xEDB88320;
    let mut crc: u32 = 0xFFFF_FFFF;

    for &byte in data {
        crc ^= byte as u32;
        for _ in 0..8 {
            if crc & 1 != 0 {
                crc = (crc >> 1) ^ CRC32_POLY;
            } else {
                crc >>= 1;
            }
        }
    }

    !crc
}

impl GPTHeader {
    pub fn deserialize(buf: &[u8]) -> Option<Self> {
        // Minimum GPT header size per UEFI spec
        if buf.len() < 92 {
            error!("GPT header too small: {} bytes", buf.len());
            return None;
        }

        // Signature must be "EFI PART"
        if &buf[0..8] != b"EFI PART" {
            error!("GPT header mismatch");
            return None;
        }

        let mut offset = 0;

        let signature: [u8; 8] = buf.get(offset..offset + 8)?.try_into().ok()?;
        offset += 8;

        let rev = u32::from_le_bytes(buf.get(offset..offset + 4)?.try_into().ok()?);
        offset += 4;

        let header_size = u32::from_le_bytes(buf.get(offset..offset + 4)?.try_into().ok()?);
        offset += 4;

        // Validate header size
        if header_size < 92 || header_size as usize > buf.len() {
            error!("Invalid GPT header size: {}", header_size);
            return None;
        }

        let stored_crc32 = u32::from_le_bytes(buf.get(offset..offset + 4)?.try_into().ok()?);
        offset += 4;

        let reserved = u32::from_le_bytes(buf.get(offset..offset + 4)?.try_into().ok()?);
        offset += 4;

        let current_lba = u64::from_le_bytes(buf.get(offset..offset + 8)?.try_into().ok()?);
        offset += 8;

        let backup_lba = u64::from_le_bytes(buf.get(offset..offset + 8)?.try_into().ok()?);
        offset += 8;

        let first_usable_lba = u64::from_le_bytes(buf.get(offset..offset + 8)?.try_into().ok()?);
        offset += 8;

        let last_usable_lba = u64::from_le_bytes(buf.get(offset..offset + 8)?.try_into().ok()?);
        offset += 8;

        let disk_guid: [u8; 16] = buf.get(offset..offset + 16)?.try_into().ok()?;
        offset += 16;

        let partition_entry_lba = u64::from_le_bytes(buf.get(offset..offset + 8)?.try_into().ok()?);
        offset += 8;

        let num_partition_entries =
            u32::from_le_bytes(buf.get(offset..offset + 4)?.try_into().ok()?);
        offset += 4;

        let size_of_partition_entry =
            u32::from_le_bytes(buf.get(offset..offset + 4)?.try_into().ok()?);
        offset += 4;

        let partition_entry_array_crc32 =
            u32::from_le_bytes(buf.get(offset..offset + 4)?.try_into().ok()?);

        // ----- CRC VALIDATION -----

        let mut header_bytes = buf.get(0..header_size as usize)?.to_vec();

        // Zero out the CRC field before computing
        header_bytes[16..20].fill(0);

        let computed_crc = crc32(&header_bytes);

        if computed_crc != stored_crc32 {
            error!("GPT header crc32 mismatch");
            return None;
        }

        Some(Self {
            signature,
            rev,
            header_size,
            crc32: stored_crc32,
            reserved,
            current_lba,
            backup_lba,
            first_usable_lba,
            last_usable_lba,
            disk_guid,
            partition_entry_lba,
            num_partition_entries,
            size_of_partition_entry,
            partition_entry_array_crc32,
        })
    }
    pub fn get_partition_entries(
        &self,
        device: &mut dyn BlockDevice,
    ) -> Option<Vec<GPTPartitionEntry>> {
        let entry_size = self.size_of_partition_entry as usize;
        let total_size = self.num_partition_entries as usize * entry_size;
        let mut buf = vec![0u8; total_size];
        let offset = self.partition_entry_lba * device.block_size();
        device.seek(SeekFrom::Start(offset)).ok()?;
        device.read_exact(&mut buf).ok()?;

        let mut entries = Vec::new();
        for i in 0..self.num_partition_entries {
            let start = (i as usize) * entry_size;
            let end = start + entry_size;
            if let Some(entry) = GPTPartitionEntry::deserialize(&buf[start..end]) {
                entries.push(entry);
            }
        }
        Some(entries)
    }
}
