//! Virtual File System (VFS) abstraction layer.
//!
//! This module provides a unified interface for file system operations, allowing
//! multiple file system implementations to be mounted and accessed through a
//! single API. The VFS handles path resolution, mount point management, and
//! dispatches operations to the appropriate underlying file system.
//!
//! # Architecture
//!
//! The VFS uses a trait-based design with three core abstractions:
//! - [`FileSystem`]: Represents a mountable file system (e.g., FAT32, ext4)
//! - [`Directory`]: Provides directory operations (list, create, remove entries)
//! - [`File`]: Provides file I/O operations (read, write, seek)
//!
//! # Example
//!
//! ```ignore
//! let mut vfs = VFS::new();
//! vfs.mount("/", my_filesystem)?;
//! let file = vfs.open_file("/path/to/file.txt")?;
//! ```

use super::FileError;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use hashbrown::HashMap;
use log::{debug, trace};

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

/// File or directory metadata.
///
/// Contains information about a file system entry including size,
/// type, and timestamps.
#[derive(Debug, Clone)]
pub struct Metadata {
    /// Size of the file in bytes (0 for directories).
    pub size: u64,
    /// Whether this entry is a directory.
    pub ftype: FileType,
    /// Creation timestamp (filesystem-dependent format).
    pub created: u64,
    /// Last modification timestamp.
    pub modified: u64,
    /// Last access timestamp.
    pub accessed: u64,
}

#[derive(Debug, Clone)]
pub enum FileType {
    File,
    Directory,
}

/// A directory entry returned when listing directory contents.
#[derive(Debug, Clone)]
pub struct DirEntry {
    /// Name of the file or subdirectory.
    pub name: String,
    /// Metadata for this entry.
    pub metadata: Metadata,
}

/// Trait for file system implementations.
///
/// Implementors provide access to the root directory, from which all
/// other file system operations can be performed.
pub trait FileSystem: Send + Sync {
    /// Returns the root directory of this file system.
    fn root_dir(&self) -> Result<Box<dyn Directory>, FileError>;
}

/// Trait for directory operations.
///
/// Provides methods for navigating the directory tree and managing
/// files and subdirectories.
pub trait Directory: Send + Sync {
    /// Opens an existing file in this directory.
    fn open_file(&mut self, name: &str) -> Result<Box<dyn File>, FileError>;
    /// Creates a new file in this directory.
    fn create_file(&mut self, name: &str) -> Result<Box<dyn File>, FileError>;
    /// Opens an existing subdirectory.
    fn open_dir(&mut self, name: &str) -> Result<Box<dyn Directory>, FileError>;
    /// Creates a new subdirectory.
    fn create_dir(&mut self, name: &str) -> Result<Box<dyn Directory>, FileError>;
    /// Removes a file or empty directory.
    fn remove(&mut self, name: &str) -> Result<(), FileError>;
    /// Lists all entries in this directory.
    fn read_dir(&mut self) -> Result<Vec<DirEntry>, FileError>;
}

/// Trait for file I/O operations.
///
/// Provides standard read, write, and seek operations on an open file.
pub trait File: Send + Sync {
    /// Reads bytes into the buffer, returning the number of bytes read.
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, FileError>;
    /// Writes bytes from the buffer, returning the number of bytes written.
    fn write(&mut self, buf: &[u8]) -> Result<usize, FileError>;
    /// Repositions the file cursor, returning the new absolute position.
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, FileError>;
    /// Flushes any buffered writes to the underlying storage.
    fn flush(&mut self) -> Result<(), FileError>;
    /// Returns metadata about this file.
    fn metadata(&self) -> Result<Metadata, FileError>;
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
    pub fn root_dir(&self) -> Result<Box<dyn Directory>, FileError> {
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
    pub fn open_file(&self, path: &str) -> Result<Box<dyn File>, FileError> {
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
        let mut dir = fs.root_dir()?;

        let parts: Vec<&str> = relative_path.split('/').filter(|p| !p.is_empty()).collect();
        debug!("VFS: path parts {:?}", parts);
        if parts.is_empty() {
            return Err(FileError::NotFound);
        }

        for part in &parts[..parts.len() - 1] {
            dir = dir.open_dir(part)?;
        }

        dir.open_file(parts.last().unwrap())
    }

    /// Creates a new file at the specified absolute path.
    ///
    /// Parent directories must already exist.
    ///
    /// # Errors
    ///
    /// Returns [`FileError::NotFound`] if the parent path doesn't exist
    /// or no file system is mounted for this path.
    pub fn create_file(&self, path: &str) -> Result<Box<dyn File>, FileError> {
        trace!("VFS: creating file at path '{}'", path);
        let normalized_path = normalize_path(path);
        let (mount_point, fs) = self
            .path_to_fs(&normalized_path)
            .ok_or(FileError::NotFound)?;

        let relative_path = normalized_path[mount_point.len()..].trim_start_matches('/');
        let mut dir = fs.root_dir()?;

        let parts: Vec<&str> = relative_path.split('/').filter(|p| !p.is_empty()).collect();
        if parts.is_empty() {
            return Err(FileError::NotFound);
        }

        for part in &parts[..parts.len() - 1] {
            dir = dir.open_dir(part)?;
        }

        dir.create_file(parts.last().unwrap())
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
    pub fn test_metadata_file() -> Option<()> {
        let meta = Metadata {
            size: 1024,
            ftype: FileType::File,
            created: 0,
            modified: 0,
            accessed: 0,
        };
        assert_eq!(meta.size, 1024);
        crate::test_assert!(matches!(meta.ftype, FileType::File));
        crate::test_assert!(!matches!(meta.ftype, FileType::Directory));
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_metadata_directory() -> Option<()> {
        let meta = Metadata {
            size: 0,
            ftype: FileType::Directory,
            created: 100,
            modified: 200,
            accessed: 300,
        };
        crate::test_assert!(matches!(meta.ftype, FileType::Directory));
        crate::test_assert!(!matches!(meta.ftype, FileType::File));
        assert_eq!(meta.created, 100);
        assert_eq!(meta.modified, 200);
        assert_eq!(meta.accessed, 300);
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
}
