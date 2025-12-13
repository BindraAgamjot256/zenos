//! Disk subsystem: block devices (AHCI SATA) + VFS + FAT filesystem.
//!
//! This module wires together three layers:
//! - Block layer ([`block`]): low-level access to storage controllers. We currently
//!   implement an AHCI driver that can read/write SATA drives using DMA.
//! - VFS layer ([`vfs`]): abstract filesystem traits (FileSystem, Directory, File)
//!   that can be implemented by multiple filesystem backends.
//! - Filesystem layer ([`fs`]): high-level file and directory access. Currently
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
use alloc::boxed::Box;
use alloc::string::String;
use block::ahci::{init, AhciBlockDevice};
use fs::fat::FatFileSystem;
use spin::{Lazy, Mutex};
use vfs::{File, FileSystem, SeekFrom};

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
pub static FS: Lazy<Mutex<FatFileSystem<BlockDeviceDriver>>> = Lazy::new(|| {
    unsafe { init() }
    let device = BlockDeviceDriver::new(Box::new(
        AhciBlockDevice::new(0).expect("Port 0 unavailable"),
    ));
    let fs = FatFileSystem::mount(device).expect("Failed to mount FAT filesystem");
    Mutex::new(fs)
});

/// Thin handle used by the process layer to perform I/O on a path within the
/// mounted filesystem.
///
/// The wrapper keeps a logical cursor (`seek` position). On each operation it
/// reopens the file from `FS` and performs the requested read/write/seek.
pub struct FileWrapper {
    path: String,
    cursor: u64,
    options: crate::process::file_handles::FileOpenOptions,
}

impl FileWrapper {
    /// Create a new file wrapper for a path with the specified open options.
    pub fn new(path: String, options: crate::process::file_handles::FileOpenOptions) -> Self {
        Self {
            path,
            cursor: 0,
            options,
        }
    }

    fn with_file<F, T>(&mut self, f: F) -> Result<T, FileError>
    where
        F: FnOnce(&mut Box<dyn File>, u64) -> Result<T, FileError>,
    {
        let fs = FS.lock();
        let mut root = fs.root_dir()?;
        let mut file = root.open_file(&self.path)?;
        f(&mut file, self.cursor)
    }
}

impl crate::process::file_handles::FileLike for FileWrapper {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, FileError> {
        use crate::process::file_handles::FileOpenOptions;
        if !self.options.contains(FileOpenOptions::READ) {
            return Err(FileError::UnsupportedOperation);
        }

        let cursor = self.cursor;
        let res = self.with_file(|file, _| {
            file.seek(SeekFrom::Start(cursor))?;
            file.read(buf)
        })?;
        self.cursor += res as u64;
        Ok(res)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, FileError> {
        use crate::process::file_handles::FileOpenOptions;
        if !self.options.contains(FileOpenOptions::WRITE) {
            return Err(FileError::UnsupportedOperation);
        }

        let cursor = self.cursor;
        let res = self.with_file(|file, _| {
            file.seek(SeekFrom::Start(cursor))?;
            file.write(buf)
        })?;
        self.cursor += res as u64;
        Ok(res)
    }

    fn seek(&mut self, pos: SeekFrom) -> Result<u64, FileError> {
        match pos {
            SeekFrom::Start(o) => {
                self.cursor = o;
                Ok(o)
            }
            SeekFrom::Current(o) => {
                let new_cursor = if o >= 0 {
                    self.cursor.checked_add(o as u64)
                } else {
                    self.cursor.checked_sub(o.unsigned_abs())
                };
                match new_cursor {
                    Some(c) => {
                        self.cursor = c;
                        Ok(c)
                    }
                    None => Err(FileError::SeekError),
                }
            }
            SeekFrom::End(o) => {
                let res = self.with_file(|file, _| file.seek(SeekFrom::End(o)))?;
                self.cursor = res;
                Ok(res)
            }
        }
    }
}
