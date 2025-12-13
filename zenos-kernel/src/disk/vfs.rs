use super::FileError;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::vec::Vec;

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
