//! Disk subsystem: block devices (AHCI SATA) + VFS + FAT filesystem.
//!
//! This module wires together three layers:
//! - Block layer ([`block`]): low-level access to storage controllers. We currently
//!   implement an AHCI driver that can read/write SATA drives using DMA.
//! - VFS layer ([`vfs`]): abstract filesystem traits (FileSystem, Directory, File)
//!   that can be implemented by multiple filesystem backends.
//! - Filesystem layer ([`fs`]): high-level file and directory access. Currently, only
//!   implements FAT12/16/32 natively.
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    Kernel / User Code                       │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                     VFS Traits (vfs.rs)                     │
//! │        FileSystem, Directory, File, Metadata, etc.          │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                  FAT Filesystem (fs/fat/)                   │
//! │         FatFileSystem, FatDirectory, FatFile                │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                 Block Device (block/mod.rs)                 │
//! │            BlockDevice trait, BlockDeviceDriver             │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                    AHCI Driver (block/ahci.rs)              │
//! │              AhciBlockDevice, DMA operations                │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Usage Example
//!
//! ```rust,ignore
//! use crate::disk::block::ahci::{init, AhciBlockDevice};
//! use crate::disk::fs::fat::FatFileSystem;
//! use crate::disk::vfs::{FileSystem, Directory, File, SeekFrom};
//!
//! // Initialize AHCI (usually done at boot)
//! unsafe { init(); }
//!
//! // Create block device for port 0
//! let device = AhciBlockDevice::new(0).expect("No disk");
//!
//! // Mount FAT filesystem
//! let fs = FatFileSystem::mount(device).expect("Mount failed");
//!
//! // Access root directory
//! let mut root = fs.root_dir().expect("No root");
//!
//! // Read directory contents
//! for entry in root.read_dir().expect("Read failed") {
//!     println!("{}: {} bytes", entry.name, entry.metadata.size);
//! }
//!
//! // Open and read a file
//! let mut file = root.open_file("README.TXT").expect("Not found");
//! let mut buf = [0u8; 1024];
//! let n = file.read(&mut buf).expect("Read failed");
//! ```

pub(crate) mod block;
pub mod fs;
pub mod vfs;

use crate::disk::block::BlockDeviceDriver;
use crate::disk::vfs::VFS;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use block::ahci::{AhciBlockDevice, init};
use fs::fat::FatFileSystem;
use spin::{Lazy, Mutex};

#[derive(Debug, Clone)]
pub enum FileError {
    /// Operation is not supported for the requested file or handle.
    UnsupportedOperation,
    /// The requested file/handle/path is invalid or not open.
    InvalidDescriptor,
    /// A read failed at the block device or filesystem level.
    ReadError,
    /// A write failed at the block device or filesystem level.
    WriteError,
    /// Seek position overflow/underflow or other seek failure.
    SeekError,
    /// Invalid numeric file descriptor value provided from userland.
    InvalidFileDescriptor,
    /// File or directory not found.
    NotFound,
    /// File or directory already exists.
    AlreadyExists,
    /// Directory is not empty.
    DirectoryNotEmpty,
    /// Generic error with a message.
    Other(String),
}

/// Global FAT filesystem instance backed by the AHCI block device on port 0.
pub static FS: Lazy<Mutex<VFS>> = Lazy::new(|| {
    unsafe { init() }
    let device = BlockDeviceDriver::new(Box::new(
        AhciBlockDevice::new(0).expect("Port 0 unavailable"),
    ));
    let fatfs = FatFileSystem::mount(device).expect("Failed to mount FAT filesystem");
    let mut vfs = VFS::new();
    vfs.mount("/", Arc::new(fatfs))
        .expect("Failed to mount FAT filesystem at /");
    Mutex::new(vfs)
});
