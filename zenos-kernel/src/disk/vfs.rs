//! Virtual File System (VFS) abstraction layer.
//!
//! This module provides a unified interface for filesystem operations, allowing
//! multiple filesystem implementations to be mounted and accessed through a
//! single API. The VFS handles path resolution, mount point management, and
//! dispatches operations to the appropriate underlying filesystem.
//!
//! # Architecture
//!
//! The VFS uses a trait-based design with three core abstractions:
//!
//! - [`FileSystem`]: Represents a mountable filesystem (e.g., FAT32, procfs).
//!   Implementations provide access to their root directory.
//!
//! - [`Directory`]: Provides directory operations including listing entries,
//!   opening/creating files and subdirectories, and removing entries.
//!
//! - [`File`]: Provides file I/O operations (read, write, seek, flush) with
//!   cursor-based access similar to standard Unix file descriptors.
//!
//! # Mount Points
//!
//! The VFS supports multiple mount points with longest-prefix matching. When
//! resolving a path like `/proc/cpuinfo`, the VFS finds the most specific
//! mount point (`/proc`) and delegates to that filesystem.
//!
//! # Path Normalization
//!
//! All paths are normalized before resolution:
//! - `.` components are removed
//! - `..` components navigate to parent directories
//! - Multiple slashes are collapsed
//! - Relative paths are treated as absolute (prefixed with `/`)
//!
//! # Example
//!
//! ```ignore
//! let mut vfs = VFS::new();
//!
//! // Mount filesystems
//! vfs.mount("/", fat_filesystem)?;
//! vfs.mount("/proc", proc_filesystem)?;
//!
//! // Access files through unified interface
//! let file = vfs.open_file("/path/to/file.txt")?;
//! let proc_file = vfs.open_file("/proc/cpuinfo")?;
//! ```

use super::FileError;
use alloc::{
    boxed::Box,
    format,
    string::{String, ToString},
    sync::Arc,
    vec::Vec,
};
use bitflags::bitflags;
use core::{fmt::Debug, sync::atomic::AtomicU64};
use hashbrown::HashMap;
use log::{debug, trace};
use spin::Mutex;

/// Specifies the position from which to seek within a file.
///
/// Used with [`File::seek`] to reposition the file cursor.
#[derive(Debug, Clone, Copy)]
pub enum SeekFrom {
    /// Seek from the beginning of the file (absolute offset).
    Start(u64),
    /// Seek from the end of the file (negative values move backward).
    End(i64),
    /// Seek relative to the current cursor position.
    Current(i64),
}

pub struct Inode {
    pub(crate) num: u64,
    pub(crate) kind: FileType,
    pub(crate) size: AtomicU64,
    pub(crate) perms: Permissions,
    pub(crate) links: AtomicU64,
    pub(crate) data: Box<dyn InodeOps + Send + Sync>,
}

impl Debug for Inode {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("Inode")
            .field("num", &self.num)
            .field("kind", &self.kind)
            .field(
                "size",
                &self.size.load(core::sync::atomic::Ordering::SeqCst),
            )
            .field("perms", &self.perms)
            .field(
                "links",
                &self.links.load(core::sync::atomic::Ordering::SeqCst),
            )
            .finish()
    }
}

#[derive(Debug)]
#[allow(dead_code)]
pub(crate) enum FileType {
    File,
    Directory,
    Symlink,
    Device,
    Socket,
    Pipe,
}

bitflags! {
    #[derive(Debug)]
    pub struct Permissions: u16 {
        const OWNER_READ    = 0o400;
        const OWNER_WRITE   = 0o200;
        const OWNER_EXEC    = 0o100;

        const GROUP_READ    = 0o040;
        const GROUP_WRITE   = 0o020;
        const GROUP_EXEC    = 0o010;

        const OTHER_READ    = 0o004;
        const OTHER_WRITE   = 0o002;
        const OTHER_EXEC    = 0o001;
    }
}

pub(crate) trait InodeOps {
    fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, FileError>;
    fn write(&mut self, offset: u64, buf: &[u8]) -> Result<usize, FileError>;

    fn truncate(&mut self, size: u64) -> Result<(), FileError>;
    fn sync(&mut self) -> Result<(), FileError>;

    // directory-only ops (return Err(NotADirectory) otherwise)
    fn lookup(&mut self, name: &str) -> Result<Arc<Mutex<Inode>>, FileError>;
    fn create(
        &mut self,
        name: &str,
        kind: FileType,
        perms: Permissions,
    ) -> Result<Arc<Mutex<Inode>>, FileError>;

    fn read_dir(&mut self) -> Result<Vec<DirEntry>, FileError>;
}

pub struct DirEntry {
    pub name: String,
    pub inode: Arc<Mutex<Inode>>,
}

#[derive(Debug)]
pub struct OpenFile {
    pub(crate) inode: Arc<Mutex<Inode>>,
    pub(crate) cursor: Mutex<u64>,
    pub(crate) file_open_options: FileOpenOptions,
}

impl Clone for OpenFile {
    fn clone(&self) -> Self {
        OpenFile {
            inode: self.inode.clone(),
            cursor: Mutex::new(*self.cursor.lock()),
            file_open_options: self.file_open_options,
        }
    }
}

impl OpenFile {
    pub fn seek(&mut self, from: SeekFrom) -> u64 {
        let current_pos = *self.cursor.lock();
        let new_pos = match from {
            SeekFrom::Start(pos) => pos,
            SeekFrom::End(offset) => {
                let file_size = self
                    .inode
                    .lock()
                    .size
                    .load(core::sync::atomic::Ordering::SeqCst);
                if offset < 0 {
                    file_size.saturating_sub((-offset) as u64)
                } else {
                    file_size.saturating_add(offset as u64)
                }
            }
            SeekFrom::Current(offset) => {
                if offset < 0 {
                    current_pos.saturating_sub((-offset) as u64)
                } else {
                    current_pos.saturating_add(offset as u64)
                }
            }
        };
        *self.cursor.lock() = new_pos;
        new_pos
    }

    pub fn read(&mut self, buf: &mut [u8]) -> Result<usize, FileError> {
        let mut ino = self.inode.lock();
        if !self
            .file_open_options
            .contains(FileOpenOptions::READ_ONLY | FileOpenOptions::READ_WRITE)
            && !ino.perms.contains(Permissions::OWNER_READ)
        {
            return Err(FileError::PermissionDenied);
        }
        let offset = *self.cursor.lock();
        let bytes_read = ino.data.read(offset, buf)?;
        *self.cursor.lock() += bytes_read as u64;
        ino.data.sync()?;
        Ok(bytes_read)
    }
    pub fn write(&mut self, buf: &[u8]) -> Result<usize, FileError> {
        let mut ino = self.inode.lock();
        if !(self
            .file_open_options
            .contains(FileOpenOptions::WRITE_ONLY | FileOpenOptions::READ_WRITE)
            || ino.perms.contains(Permissions::OWNER_WRITE))
        {
            // if someone wonders how this works, I used de Morgan's law.
            return Err(FileError::PermissionDenied);
        }
        let offset = *self.cursor.lock();
        let bytes_written = ino.data.write(offset, buf)?;
        *self.cursor.lock() += bytes_written as u64;
        ino.data.sync()?;
        Ok(bytes_written)
    }
}

bitflags! {
    #[derive(Debug, Clone, Copy)]
    pub struct FileOpenOptions: u64 {
        // Access modes (mutually exclusive)
        const READ_ONLY  = 0; // O_RDONLY
        const WRITE_ONLY = 1; // O_WRONLY
        const READ_WRITE = 2; // O_RDWR

        // Flags
        const CREATE        = 0o100;      // O_CREAT
        const EXCLUSIVE     = 0o200;      // O_EXCL
        const NOCTTY        = 0o400;      // O_NOCTTY
        const TRUNCATE      = 0o1000;     // O_TRUNC
        const APPEND        = 0o2000;     // O_APPEND
        const NONBLOCK      = 0o4000;     // O_NONBLOCK
        const SYNC          = 0o10000;    // O_SYNC
        const CLOSE_ON_EXEC = 0o2000000;  // O_CLOEXEC
    }

}

impl Default for FileOpenOptions {
    fn default() -> Self {
        let mut foo = FileOpenOptions::empty();
        foo |= Self::READ_ONLY;
        foo |= Self::CREATE;
        foo |= Self::EXCLUSIVE;

        foo
    }
}

/// Trait for file system implementations.
///
/// Implementors provide access to the root directory, from which all
/// other file system operations can be performed.
pub trait FileSystem: Send + Sync {
    /// Returns the root directory of this file system.
    fn root_dir(&self) -> Result<Arc<Mutex<Inode>>, FileError>;
}

/// Mapping of filesystem mount points to their drivers.
type FSMap = HashMap<String, Arc<dyn FileSystem + Send + Sync>>;

/// Virtual File System manager.
///
/// Manages multiple mounted file systems and provides unified path-based
/// access to files and directories across all mounts. The VFS automatically
/// resolves paths to the appropriate file system based on mount points.
pub struct VFS {
    /// Map of mount points to their file system implementations.
    fs: FSMap,
}

impl VFS {
    /// Creates a new empty VFS with no mounted file systems.
    pub fn new() -> Self {
        VFS { fs: HashMap::new() }
    }

    /// Mounts a file system at the specified path.
    ///
    /// # Errors
    ///
    /// Returns [`FileError::AlreadyExists`] if a file system is already
    /// mounted at the given path.
    pub fn mount(
        &mut self,
        path: &str,
        fs: Arc<dyn FileSystem + Sync + Send>,
    ) -> Result<(), FileError> {
        debug!("VFS: mounting filesystem at '{}'", path);
        if self.fs.contains_key(path) {
            debug!("VFS: mount failed - path already exists");
            return Err(FileError::AlreadyExists);
        }
        self.fs.insert(path.to_string(), fs);
        debug!("VFS: mount successful");
        Ok(())
    }

    /// Unmounts the file system at the specified path.
    ///
    /// # Errors
    ///
    /// Returns [`FileError::NotFound`] if no file system is mounted at the path.
    pub fn unmount(&mut self, path: &str) -> Result<(), FileError> {
        debug!("VFS: unmounting filesystem at '{}'", path);
        if self.fs.remove(path).is_none() {
            debug!("VFS: unmount failed - path not found");
            return Err(FileError::NotFound);
        }
        debug!("VFS: unmount successful");
        Ok(())
    }

    /// Returns the file system mounted at the exact path, if any.
    pub fn get_fs(&self, path: &str) -> Option<Arc<dyn FileSystem + Sync + Send>> {
        trace!("VFS: looking up filesystem for path '{}'", path);
        self.fs.get(path).cloned()
    }

    /// Returns the root directory of the file system mounted at "/".
    ///
    /// # Errors
    ///
    /// Returns [`FileError::NotFound`] if no file system is mounted at "/".
    pub fn root_dir(&self) -> Result<Arc<Mutex<Inode>>, FileError> {
        trace!("VFS: getting root directory");
        let fs = self.get_fs("/").ok_or(FileError::NotFound)?;
        fs.root_dir()
    }

    /// Finds the file system responsible for the given path.
    ///
    /// Returns the mount point and file system that best matches the path
    /// (longest matching prefix). Returns `None` if no matching mount exists.
    pub fn path_to_fs(&self, path: &str) -> Option<(&str, Arc<dyn FileSystem + Sync + Send>)> {
        let normalized_path = normalize_path(path);
        let mut best_match: Option<(&str, Arc<dyn FileSystem + Sync + Send>)> = None;
        for (mount_point, fs) in &self.fs {
            if normalized_path.starts_with(mount_point) {
                if let Some((best_mount, _)) = &best_match {
                    if mount_point.len() > best_mount.len() {
                        best_match = Some((mount_point.as_str(), fs.clone()));
                    }
                } else {
                    best_match = Some((mount_point.as_str(), fs.clone()));
                }
            }
        }
        best_match
    }

    /// Opens an existing file at the specified absolute path.
    ///
    /// The path is normalized and resolved to the appropriate mounted
    /// file system based on the longest matching mount point.
    ///
    /// # Errors
    ///
    /// Returns [`FileError::NotFound`] if the path doesn't exist or no
    /// file system is mounted for this path.
    pub fn open_file(&self, path: &str) -> Result<Arc<Mutex<Inode>>, FileError> {
        trace!("VFS: opening file at path '{}'", path);
        let normalized_path = normalize_path(path);
        debug!("VFS: normalized path '{}'", normalized_path);
        let (mount_point, fs) = self
            .path_to_fs(&normalized_path)
            .ok_or(FileError::NotFound)?;

        debug!(
            "VFS: matched mount point '{}' for path '{}'",
            mount_point, normalized_path
        );
        let relative_path = normalized_path[mount_point.len()..].trim_start_matches('/');
        debug!("VFS: relative path '{}'", relative_path);
        let mut dir_inode = fs.root_dir()?;

        let parts: Vec<&str> = relative_path.split('/').filter(|p| !p.is_empty()).collect();
        debug!("VFS: path parts {:?}", parts);
        if parts.is_empty() {
            return Ok(dir_inode);
        }
        for part in &parts[..parts.len() - 1] {
            debug!("VFS: looking up directory '{}'", part);
            let di = dir_inode.lock().data.lookup(part)?;
            if di.lock().perms.contains(Permissions::OWNER_EXEC) {
                debug!("VFS: directory '{}' is executable", part);
            } else {
                debug!("VFS: directory '{}' is not executable", part);
                return Err(FileError::PermissionDenied);
            }
            debug!("VFS: directory lookup successful");
            dir_inode = di;
        }
        debug!(
            "VFS: opened directory at path '{}', inode: {:#?}",
            relative_path, dir_inode
        );
        let file_inode = dir_inode.lock().data.lookup(parts.last().unwrap())?;
        if file_inode.lock().perms.contains(Permissions::OWNER_READ) {
            debug!("VFS: file '{}' is readable", parts.last().unwrap());
        } else {
            debug!("VFS: file '{}' is not readable", parts.last().unwrap());
            return Err(FileError::PermissionDenied);
        }
        debug!(
            "VFS: file inode found with number {}",
            file_inode.lock().num
        );
        Ok(file_inode)
    }

    /// Creates a new file at the specified absolute path.
    ///
    /// Parent directories must already exist.
    ///
    /// # Errors
    ///
    /// Returns [`FileError::NotFound`] if the parent path doesn't exist
    /// or no file system is mounted for this path.
    pub fn create_file(
        &self,
        path: &str,
        perms: Permissions,
    ) -> Result<Arc<Mutex<Inode>>, FileError> {
        trace!("VFS: creating file at path '{}'", path);
        let normalized_path = normalize_path(path);
        let (mount_point, fs) = self
            .path_to_fs(&normalized_path)
            .ok_or(FileError::NotFound)?;

        let relative_path = normalized_path[mount_point.len()..].trim_start_matches('/');
        let mut dir_inode = fs.root_dir()?;

        let parts: Vec<&str> = relative_path.split('/').filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            return Ok(dir_inode);
        }
        for part in &parts[..parts.len() - 1] {
            let di = dir_inode.lock().data.lookup(part)?;
            if di
                .lock()
                .perms
                .contains(Permissions::OWNER_EXEC | Permissions::OWNER_WRITE)
            {
                debug!("VFS: directory '{}' is executable", part);
            } else {
                debug!("VFS: directory '{}' is not executable", part);
                return Err(FileError::PermissionDenied);
            }
            dir_inode = di;
        }
        dir_inode
            .lock()
            .data
            .create(parts.last().unwrap(), FileType::File, perms)
    }
}

/// Normalizes a path by resolving `.` and `..` components.
///
/// Returns an absolute path starting with `/`. Empty paths become `/`.
fn normalize_path(path: &str) -> String {
    let mut components = Vec::new();
    for part in path.split('/') {
        match part {
            "" | "." => continue,
            ".." => {
                components.pop();
            }
            _ => components.push(part),
        }
    }
    format!("/{}", components.join("/"))
}

#[cfg(feature = "run-kunittest")]
mod tests {
    use super::*;
    use crate::Test;
    use crate::test_assert_eq as assert_eq;

    #[zenos_macros::test]
    pub fn test_seek_from_start() -> Option<()> {
        let seek = SeekFrom::Start(100);
        match seek {
            SeekFrom::Start(pos) => assert_eq!(pos, 100),
            _ => return None,
        }
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_seek_from_end() -> Option<()> {
        let seek = SeekFrom::End(-50);
        match seek {
            SeekFrom::End(offset) => assert_eq!(offset, -50),
            _ => return None,
        }
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_seek_from_current() -> Option<()> {
        let seek = SeekFrom::Current(25);
        match seek {
            SeekFrom::Current(offset) => assert_eq!(offset, 25),
            _ => return None,
        }
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_vfs_new() -> Option<()> {
        let vfs = VFS::new();
        crate::test_assert!(vfs.get_fs("/").is_none());
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_vfs_unmount_not_found() -> Option<()> {
        let mut vfs = VFS::new();
        let result = vfs.unmount("/nonexistent");
        match result {
            Err(FileError::NotFound) => {}
            _ => return None,
        }
        Some(())
    }
    #[zenos_macros::test]
    pub fn test_file_open_options_default() -> Option<()> {
        let opts = FileOpenOptions::default();
        crate::test_assert!(!opts.is_empty());
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_file_open_options_read() -> Option<()> {
        let opts = FileOpenOptions::READ_ONLY;
        crate::test_assert!(opts.contains(FileOpenOptions::READ_ONLY));
        crate::test_assert!(!opts.contains(FileOpenOptions::WRITE_ONLY));
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_file_open_options_combined() -> Option<()> {
        let opts = FileOpenOptions::READ_ONLY | FileOpenOptions::WRITE_ONLY;
        crate::test_assert!(opts.contains(FileOpenOptions::READ_ONLY));
        crate::test_assert!(opts.contains(FileOpenOptions::WRITE_ONLY));
        crate::test_assert!(!opts.contains(FileOpenOptions::CREATE));
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_file_open_options_all() -> Option<()> {
        let opts = FileOpenOptions::all();
        crate::test_assert!(opts.contains(FileOpenOptions::READ_ONLY));
        crate::test_assert!(opts.contains(FileOpenOptions::WRITE_ONLY));
        crate::test_assert!(opts.contains(FileOpenOptions::CREATE));
        crate::test_assert!(opts.contains(FileOpenOptions::TRUNCATE));
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_file_open_options_from_bits() -> Option<()> {
        let opts = FileOpenOptions::from_bits_truncate(
            FileOpenOptions::READ_ONLY.bits() | FileOpenOptions::CREATE.bits(),
        );
        crate::test_assert!(opts.contains(FileOpenOptions::READ_ONLY));
        crate::test_assert!(opts.contains(FileOpenOptions::CREATE));
        crate::test_assert!(!opts.contains(FileOpenOptions::WRITE_ONLY));
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_file_open_options_bits_values() -> Option<()> {
        assert_eq!(FileOpenOptions::READ_ONLY.bits(), 0);
        assert_eq!(FileOpenOptions::WRITE_ONLY.bits(), 0b0001);
        assert_eq!(FileOpenOptions::CREATE.bits(), 0o100);
        assert_eq!(FileOpenOptions::TRUNCATE.bits(), 0o1000);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_open_dir_inode() -> Option<()> {
        use crate::disk::fs::proc::ProcFs;

        let procfs = ProcFs;
        let root_inode = procfs.root_dir().ok()?;
        let inode_guard = root_inode.lock();

        // Verify it's a directory
        match inode_guard.kind {
            FileType::Directory => {}
            _ => return None,
        }

        // Verify read permissions
        crate::test_assert!(inode_guard.perms.contains(Permissions::OWNER_READ));

        // Verify directory operations return IsADirectory for read/write
        drop(inode_guard);
        let mut inode_guard = root_inode.lock();
        let mut buf = [0u8; 16];
        match inode_guard.data.read(0, &mut buf) {
            Err(crate::disk::FileError::IsADirectory) => {}
            _ => return None,
        }

        // Verify read_dir works
        let entries = inode_guard.data.read_dir().ok()?;
        crate::test_assert!(!entries.is_empty());

        Some(())
    }

    #[zenos_macros::test]
    pub fn test_open_dir_from_vfs() -> Option<()> {
        use crate::disk::fs::proc::ProcFs;

        let mut vfs = VFS::new();
        vfs.mount("/proc", Arc::new(ProcFs)).ok()?;

        let inode = vfs.open_file("/proc").ok()?;
        let inode_guard = inode.lock();

        // Verify it's a directory
        match inode_guard.kind {
            FileType::Directory => {}
            _ => return None,
        }

        // Verify read permissions
        crate::test_assert!(inode_guard.perms.contains(Permissions::OWNER_READ));

        // Verify directory operations return IsADirectory for read/write
        drop(inode_guard);
        let mut inode_guard = inode.lock();
        let mut buf = [0u8; 16];
        match inode_guard.data.read(0, &mut buf) {
            Err(FileError::IsADirectory) => {}
            _ => return None,
        }

        // Verify read_dir works
        let entries = inode_guard.data.read_dir().ok()?;
        crate::test_assert!(!entries.is_empty());

        None
    }
}
