//! Disk subsystem: block devices (AHCI SATA) + VFS + filesystems.
//!
//! This module provides the complete storage stack for the kernel, wiring together
//! multiple layers of abstraction from raw hardware access to high-level file operations.
//!
//! # Layers
//!
//! - **Block layer** ([`block`]): Low-level sector-addressable storage abstraction.
//!   Provides the [`BlockDevice`](BlockDevice) trait and concrete drivers.
//!   Currently, implements an AHCI driver ([`block::ahci`]) for SATA drives using DMA.
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

use crate::disk::block::{BlockDeviceDriver, GPTPartitionEntry};
use crate::disk::{
    block::{BlockDevice, ahci::init_ahcibd},
    vfs::VFS,
};
use alloc::boxed::Box;
use alloc::vec::Vec;
use alloc::{format, string::String, sync::Arc};
use fs::fat::FatFileSystem;
use log::info;
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
pub static FS: Lazy<Mutex<VFS>> = Lazy::new(|| Mutex::new(VFS::new()));

static BLOCKDEVICES: &[(u8, unsafe fn() -> Option<Arc<Mutex<dyn BlockDevice>>>)] = &[(0, || {
    let ahcibd = unsafe { init_ahcibd() };
    if let Some(ahcibd) = ahcibd {
        Some(Arc::new(Mutex::new(ahcibd)))
    } else {
        None
    }
})];
static FILESYSTEMS: &[(
    &str,
    fn(
        blockdev: BlockDeviceDriver,
        part_entry: &GPTPartitionEntry,
    ) -> Option<Arc<dyn vfs::FileSystem>>,
)] = &[("FAT", |blockdev, part_entry| {
    assert!(
        part_entry.is_used(),
        "Partition entry must be used to mount filesystem"
    );
    if let Ok(fatfs) = FatFileSystem::mount(blockdev, part_entry.starting_lba * 512) {
        Some(Arc::new(fatfs))
    } else {
        None
    }
})];

pub fn init() {
    let mut vfs = FS.lock();
    for (blocdev_id, devinit_fn) in BLOCKDEVICES.iter() {
        if let Some(mut blockdev) = unsafe { devinit_fn() } {
            info!("found block device, id: {}", blocdev_id);
            let gpt = blockdev.get_header().unwrap();
            let mut guard = blockdev.lock();

            let entries = gpt.get_partition_entries(&mut *guard).unwrap();

            let mut partitions = entries
                .iter()
                .filter(|entry| entry.is_used())
                .collect::<Vec<_>>();
            drop(guard);
            info!("GPT found {} partitions", partitions.len());
            let mut root_mounted = false;
            for (partition, (name, fsinitfn)) in partitions.iter_mut().zip(FILESYSTEMS.iter()) {
                info!(
                    "Trying to mount partition {} with filesystem {name}",
                    String::from_utf16_lossy(&partition.partition_name).trim_matches(char::from(0))
                );
                if let Some(fs) = fsinitfn(
                    BlockDeviceDriver::new(Box::new(blockdev.clone())),
                    *partition,
                ) {
                    info!(
                        "Mounted partition \"{}\" with filesystem {name}",
                        String::from_utf16_lossy(&partition.partition_name)
                            .trim_matches(char::from(0))
                    );
                    if !root_mounted {
                        vfs.mount("/", fs).unwrap();
                        root_mounted = true;
                    } else {
                        // For simplicity, we mount additional filesystems at /mnt/partitionN
                        let mount_point = format!(
                            "/mnt/{}",
                            String::from_utf16_lossy(&partition.partition_name)
                                .trim_matches(char::from(0))
                        );
                        vfs.mount(&mount_point, fs).unwrap();
                        info!(
                            "Mounted partition {} at {}",
                            String::from_utf16_lossy(&partition.partition_name)
                                .trim_matches(char::from(0)),
                            mount_point
                        );
                    }
                } else {
                    info!(
                        "Failed to mount partition {} with filesystem {name}",
                        String::from_utf16_lossy(&partition.partition_name)
                            .trim_matches(char::from(0))
                    );
                }
            }
        } else {
            info!("No block device found at port {}", blocdev_id);
        }
    }
    vfs.mount("/proc", Arc::new(fs::proc::ProcFs)).unwrap();
}
