use super::FileError;
use alloc::boxed::Box;
use alloc::format;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use hashbrown::HashMap;
use log::{debug, trace};

#[derive(Debug, Clone, Copy)]
pub enum SeekFrom {
    Start(u64),
    End(i64),
    Current(i64),
}

#[derive(Debug, Clone)]
pub struct Metadata {
    pub size: u64,
    pub is_dir: bool,
    pub is_file: bool,
    pub created: u64,
    pub modified: u64,
    pub accessed: u64,
}

#[derive(Debug, Clone)]
pub struct DirEntry {
    pub name: String,
    pub metadata: Metadata,
}

pub trait FileSystem: Send + Sync {
    fn root_dir(&self) -> Result<Box<dyn Directory>, FileError>;
}

pub trait Directory: Send + Sync {
    fn open_file(&mut self, name: &str) -> Result<Box<dyn File>, FileError>;
    fn create_file(&mut self, name: &str) -> Result<Box<dyn File>, FileError>;
    fn open_dir(&mut self, name: &str) -> Result<Box<dyn Directory>, FileError>;
    fn create_dir(&mut self, name: &str) -> Result<Box<dyn Directory>, FileError>;
    fn remove(&mut self, name: &str) -> Result<(), FileError>;
    fn read_dir(&mut self) -> Result<Vec<DirEntry>, FileError>;
}

pub trait File: Send + Sync {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, FileError>;
    fn write(&mut self, buf: &[u8]) -> Result<usize, FileError>;
    fn seek(&mut self, pos: SeekFrom) -> Result<u64, FileError>;
    fn flush(&mut self) -> Result<(), FileError>;
    fn metadata(&self) -> Result<Metadata, FileError>;
}

/// Mapping of filesystem mount points to their drivers.
type FSMap = HashMap<String, Arc<dyn FileSystem + Send + Sync>>;

pub struct VFS {
    fs: FSMap,
}

impl VFS {
    pub fn new() -> Self {
        VFS { fs: HashMap::new() }
    }

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

    pub fn unmount(&mut self, path: &str) -> Result<(), FileError> {
        debug!("VFS: unmounting filesystem at '{}'", path);
        if self.fs.remove(path).is_none() {
            debug!("VFS: unmount failed - path not found");
            return Err(FileError::NotFound);
        }
        debug!("VFS: unmount successful");
        Ok(())
    }

    pub fn get_fs(&self, path: &str) -> Option<Arc<dyn FileSystem + Sync + Send>> {
        trace!("VFS: looking up filesystem for path '{}'", path);
        self.fs.get(path).cloned()
    }
    pub fn root_dir(&self) -> Result<Box<dyn Directory>, FileError> {
        trace!("VFS: getting root directory");
        let fs = self.get_fs("/").ok_or(FileError::NotFound)?;
        fs.root_dir()
    }

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

    pub fn open_file(&self, path: &str) -> Result<Box<dyn File>, FileError> {
        trace!("VFS: opening file at path '{}'", path);
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

        dir.open_file(parts.last().unwrap())
    }
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
            is_dir: false,
            is_file: true,
            created: 0,
            modified: 0,
            accessed: 0,
        };
        assert_eq!(meta.size, 1024);
        crate::test_assert!(meta.is_file);
        crate::test_assert!(!meta.is_dir);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_metadata_directory() -> Option<()> {
        let meta = Metadata {
            size: 0,
            is_dir: true,
            is_file: false,
            created: 100,
            modified: 200,
            accessed: 300,
        };
        crate::test_assert!(meta.is_dir);
        crate::test_assert!(!meta.is_file);
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
