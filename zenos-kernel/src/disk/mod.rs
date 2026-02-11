//! Disk subsystem: block devices (AHCI SATA) + VFS + filesystems.
//!
//! This module provides the complete storage stack for the kernel, wiring together
//! multiple layers of abstraction from raw hardware access to high-level file operations.
//!
//! # Layers
//!
//! - **Block layer** ([`block`]): Low-level sector-addressable storage abstraction.
//!   Provides the [`BlockDevice`](block::BlockDevice) trait and concrete drivers.
//!   Currently implements an AHCI driver ([`block::ahci`]) for SATA drives using DMA.
//!
//! - **VFS layer** ([`vfs`]): Virtual File System abstraction providing unified
//!   filesystem traits ([`FileSystem`](vfs::FileSystem), [`Inode`](vfs::InodeOps)) that can be implemented by multiple backends. Handles
//!   mount point management and path resolution across mounted filesystems.
//!
//! - **Filesystem layer** ([`fs`]): Concrete filesystem implementations:
//!   - [`fs::fat`]: Native FAT12/FAT16/FAT32 implementation for persistent storage.
//!   - [`fs::proc`]: Virtual procfs exposing kernel and process information at `/proc`.
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
//! │                        VFS (vfs.rs)                         │
//! │   Mount management, path resolution, unified file access    │
//! └─────────────────────────────────────────────────────────────┘
//!                    │                     │
//!                    ▼                     ▼
//! ┌──────────────────────────┐  ┌──────────────────────────────┐
//! │   FAT Filesystem (fs/fat)│  │    ProcFS (fs/proc)          │
//! │   FatFileSystem          │  │    Virtual /proc filesystem  │
//! │   FatDirectory, FatFile  │  │    cpuinfo, meminfo, etc.    │
//! └──────────────────────────┘  └──────────────────────────────┘
//!              │
//!              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │              Block Device Layer (block/mod.rs)              │
//! │         BlockDevice trait, BlockDeviceDriver wrapper        │
//! └─────────────────────────────────────────────────────────────┘
//!                              │
//!                              ▼
//! ┌─────────────────────────────────────────────────────────────┐
//! │                 AHCI Driver (block/ahci.rs)                 │
//! │    AhciBlockDevice: SATA access via DMA command submission  │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Global Filesystem
//!
//! The kernel provides a global [`FS`] instance that mounts:
//! - FAT filesystem from AHCI port 0 at `/`
//! - ProcFS at `/proc`
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
//!
//! # Error Handling
//!
//! All operations return [`FileError`] variants for consistent error handling
//! across the storage stack.

pub(crate) mod block;
pub mod fs;
pub mod vfs;

use crate::disk::block::BlockDeviceDriver;
use crate::disk::vfs::{Inode, VFS};
use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use block::ahci::{AhciBlockDevice, init};
use core::sync::atomic::Ordering;
use fs::fat::FatFileSystem;
use spin::{Lazy, Mutex};

#[derive(Debug, Clone)]
pub enum FileError {
    /// Operation is not supported for the requested file or handle.
    UnsupportedOperation,
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
    /// Attempted to read/write a directory as a file.
    NotADirectory,
    /// Attempted to perform a directory only operation on a file.
    IsADirectory,
    /// Operation not permitted due to insufficient permissions.
    PermissionDenied,
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
    // Mount procfs at /proc
    vfs.mount("/proc", Arc::new(fs::proc::ProcFs))
        .expect("Failed to mount procfs at /proc");
    Mutex::new(vfs)
});

#[allow(dead_code)]
pub(crate) fn get_len(file: &mut Arc<Inode>) -> Result<u64, ()> {
    Ok(file.size.load(Ordering::SeqCst))
}
