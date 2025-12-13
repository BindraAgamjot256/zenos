use crate::disk::vfs::SeekFrom;
use crate::disk::FileError;
use crate::hardware::keyboard;
use crate::kprint;
use alloc::boxed::Box;
use bitflags::bitflags;
use core::any::Any;
use core::fmt::Debug;
use core::ops::Deref;

#[derive(Clone)]
pub struct Stdout;
#[derive(Clone)]
pub struct Stderr;
#[derive(Clone)]
pub struct Stdin;

pub(crate) trait FileLike: Send + Sync {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, FileError>;
    fn write(&mut self, buffer: &[u8]) -> Result<usize, FileError>;
    fn seek(&mut self, position: SeekFrom) -> Result<u64, FileError>;
}

impl FileLike for Stdout {
    fn read(&mut self, _buffer: &mut [u8]) -> Result<usize, FileError> {
        Err(FileError::UnsupportedOperation)
    }

    fn write(&mut self, buffer: &[u8]) -> Result<usize, FileError> {
        unsafe { kprint!("{}", core::str::from_utf8_unchecked(buffer)) };
        Ok(buffer.len())
    }

    fn seek(&mut self, _position: SeekFrom) -> Result<u64, FileError> {
        Err(FileError::UnsupportedOperation)
    }
}

impl FileLike for Stderr {
    fn read(&mut self, _buffer: &mut [u8]) -> Result<usize, FileError> {
        Err(FileError::UnsupportedOperation)
    }

    fn write(&mut self, buffer: &[u8]) -> Result<usize, FileError> {
        unsafe { kprint!("{}", core::str::from_utf8_unchecked(buffer)) };
        Ok(buffer.len())
    }

    fn seek(&mut self, _position: SeekFrom) -> Result<u64, FileError> {
        Err(FileError::UnsupportedOperation)
    }
}

impl FileLike for Stdin {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, FileError> {
        let n = keyboard::read_exact(buffer);
        Ok(n)
    }

    fn write(&mut self, _buffer: &[u8]) -> Result<usize, FileError> {
        Err(FileError::UnsupportedOperation)
    }

    fn seek(&mut self, _position: SeekFrom) -> Result<u64, FileError> {
        Err(FileError::UnsupportedOperation)
    }
}

pub(crate) struct FileHandle {
    descriptor: Box<dyn FileLike>,
    _foo: FileOpenOptions,
}

impl FileHandle {
    pub fn new(_id: u32, descriptor: Box<dyn FileLike>, _foo: FileOpenOptions) -> Self {
        Self { descriptor, _foo }
    }

    pub fn descriptor(&mut self) -> &mut dyn FileLike {
        self.descriptor.as_mut()
    }
}

impl Debug for FileHandle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FileHandle")
            .field("type", &self.descriptor.deref().type_id())
            .finish()
    }
}

bitflags! {
    #[derive(Default, Debug, Clone, Copy)]
    pub struct FileOpenOptions: u64 {
        const READ = 0b0001;
        const WRITE = 0b0010;
        const CREATE = 0b0100;
        const TRUNCATE = 0b1000;
    }
}
