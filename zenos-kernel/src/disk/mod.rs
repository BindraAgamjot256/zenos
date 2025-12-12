//! Disk subsystem: block devices (AHCI SATA) + a FAT filesystem powered by [`fatfs`].
//!
//! This module wires together three layers:
//! - Block layer ([`block`]): low-level access to storage controllers. We currently
//!   implement an AHCI driver that can read/write SATA drives using DMA.
//! - Filesystem layer ([`fs`]): high-level file and directory access. For now we
//!   integrate the third-party [`fatfs`] crate to work with FAT12/16/32 volumes.
//! - Thin kernel adapters on top ([`FS`], [`File`], and [`FileWrapper`]) used by the process
//!   subsystem to open/read/write/seek files.
//!
//! Boot-time initialization: on first access, [`FS`] lazily initializes AHCI (probing ports)
//! and mounts the first port (`port 0`) as a FAT filesystem. All file operations in this
//! module go through that global instance.
//!
//! Notes and limitations:
//! - Only a single drive/partition is mounted (AHCI port 0).
//! - Concurrency: accesses are synchronized with a [`Mutex`] around the filesystem.
//! - [`FileWrapper`] re-opens the underlying FAT file on each operation and tracks a
//!   per-wrapper cursor. This keeps the wrapper small and avoids keeping [`fatfs::File`]
//!   instances across syscalls.
//! - Error handling follows [`fatfs`] conventions via the custom [`FileError`] implementing
//!   [`IoError`].
//!
//! See also:
//! - [`block::ahci`] for controller details
//! - [`block::BlockDevice`] for the abstract I/O interface used by [`fatfs`]
//! - [`crate::process::file_handles`] for how user-facing file descriptors map to [`FileWrapper`]

pub(crate) mod block;
pub(crate) mod fs;
//mod vfs;

use crate::disk::block::ahci::{init, AhciBlockDevice};
use crate::disk::block::BlockDeviceDriver;
use crate::process::file_handles::FileOpenOptions;
use alloc::boxed::Box;
use alloc::string::String;
use fatfs::{
    DefaultTimeProvider, Error, FileSystem, IoBase, IoError, LossyOemCpConverter, Read, Seek,
    SeekFrom, Write,
};
use spin::{Lazy, Mutex};

#[derive(Debug)]
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
    /// Propagated `fatfs` I/O error (erased generic parameter).
    IoError(Error<()>),
    /// Invalid numeric file descriptor value provided from userland.
    InvalidFileDescriptor,
}

impl IoError for FileError {
    fn is_interrupted(&self) -> bool {
        false
    }

    fn new_unexpected_eof_error() -> Self {
        Self::IoError(Error::UnexpectedEof)
    }

    fn new_write_zero_error() -> Self {
        Self::IoError(Error::WriteZero)
    }
}

impl From<Error<FileError>> for FileError {
    fn from(err: Error<FileError>) -> Self {
        unsafe {
            match err {
                Error::Io(e) => e,
                other => {
                    FileError::IoError(core::mem::transmute::<Error<FileError>, Error<()>>(other))
                }
            }
        }
    }
}

/// Global FAT filesystem instance backed by the AHCI block device on port 0.
///
/// This is lazily initialized on first use by calling `ahci::init()` and then
/// constructing a `fatfs::FileSystem` on top of a `BlockDeviceDriver` that wraps
/// `AhciBlockDevice`.
pub static FS: Lazy<Mutex<FileSystem<BlockDeviceDriver<FileError>>>> = Lazy::new(|| {
    unsafe { init() }
    let fs = FileSystem::new(
        BlockDeviceDriver::new(Box::new(
            AhciBlockDevice::new(0).expect("Port 0 unavailable"),
        )),
        fatfs::FsOptions::new(),
    )
        .expect("Panics");

    Mutex::new(fs)
});

/// Convenience alias for a `fatfs::File` using our block device and default providers.
pub type File<'a> =
fatfs::File<'a, BlockDeviceDriver<FileError>, DefaultTimeProvider, LossyOemCpConverter>;

/// Thin handle used by the process layer to perform I/O on a path within the
/// mounted FAT filesystem.
///
/// The wrapper keeps a logical cursor (`seek` position). On each operation it
/// reopens the file from `FS` and performs the requested read/write/seek.
pub struct FileWrapper {
    path: String,
    cursor: u64,
    foo: FileOpenOptions,
}

impl FileWrapper {
    /// Create a new file wrapper for a path with the specified open options.
    pub fn new(path: String, foo: FileOpenOptions) -> Self {
        Self {
            path,
            cursor: 0,
            foo,
        }
    }
}

impl IoBase for FileWrapper {
    type Error = FileError;
}

impl Read for FileWrapper {
    /// Read bytes into `buf` at the current cursor.
    ///
    /// Fails with `WriteError` if the file wasn't opened with read permissions.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, Self::Error> {
        if !self.foo.contains(FileOpenOptions::READ) {
            return Err(FileError::WriteError);
        }
        let fs = FS.lock();
        let mut file = fs
            .root_dir()
            .open_file(&self.path)
            .map_err(|_| FileError::InvalidDescriptor)?;
        file.seek(SeekFrom::Start(self.cursor))
            .map_err(FileError::from)?;
        let res = file.read(buf).map_err(FileError::from)?;
        self.cursor += res as u64;
        Ok(res)
    }
}

impl Write for FileWrapper {
    /// Write bytes from `buf` at the current cursor.
    ///
    /// Fails with `WriteError` if the file wasn't opened with write permissions.
    fn write(&mut self, buf: &[u8]) -> Result<usize, Self::Error> {
        if !self.foo.contains(FileOpenOptions::WRITE) {
            return Err(FileError::WriteError);
        }
        let fs = FS.lock();
        let mut file = fs
            .root_dir()
            .open_file(&self.path)
            .map_err(|_| FileError::InvalidDescriptor)?;
        file.seek(SeekFrom::Start(self.cursor))
            .map_err(FileError::from)?;
        let res = file.write(buf).map_err(FileError::from)?;
        self.cursor += res as u64;
        Ok(res)
    }
    fn flush(&mut self) -> Result<(), Self::Error> {
        let fs = FS.lock();
        let mut file = fs
            .root_dir()
            .open_file(&self.path)
            .map_err(|_| FileError::InvalidDescriptor)?;
        file.flush().map_err(FileError::from)
    }
}

impl Seek for FileWrapper {
    /// Update or query the logical cursor using `SeekFrom` semantics.
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, Self::Error> {
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
            SeekFrom::End(_) => {
                let fs = FS.lock();
                let mut file = fs
                    .root_dir()
                    .open_file(&self.path)
                    .map_err(|_| FileError::InvalidDescriptor)?;
                let res = file.seek(pos).map_err(FileError::from)?;
                self.cursor = res;
                Ok(res)
            }
        }
    }
}
