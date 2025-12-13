use super::FileError;
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::sync::Arc;
use alloc::vec::Vec;
use hashbrown::HashMap;

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
        if self.fs.contains_key(path) {
            return Err(FileError::AlreadyExists);
        }
        self.fs.insert(path.to_string(), fs);
        Ok(())
    }

    pub fn unmount(&mut self, path: &str) -> Result<(), FileError> {
        if self.fs.remove(path).is_none() {
            return Err(FileError::NotFound);
        }
        Ok(())
    }

    pub fn get_fs(&self, path: &str) -> Option<Arc<dyn FileSystem + Sync + Send>> {
        self.fs.get(path).cloned()
    }
    pub fn root_dir(&self) -> Result<Box<dyn Directory>, FileError> {
        let fs = self.get_fs("/").ok_or(FileError::NotFound)?;
        fs.root_dir()
    }
}
