use crate::disk::vfs::{File, Metadata, SeekFrom};
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

impl File for Stdout {
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
    fn flush(&mut self) -> Result<(), FileError> {
        Ok(())
    }
    fn metadata(&self) -> Result<Metadata, FileError> {
        Ok(Metadata {
            size: 0,
            is_dir: false,
            is_file: true,
            created: 0,
            modified: 0,
            accessed: 0,
        })
    }
}

impl File for Stderr {
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
    fn flush(&mut self) -> Result<(), FileError> {
        Ok(())
    }
    fn metadata(&self) -> Result<Metadata, FileError> {
        Ok(Metadata {
            size: 0,
            is_dir: false,
            is_file: true,
            created: 0,
            modified: 0,
            accessed: 0,
        })
    }
}

impl File for Stdin {
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
    fn flush(&mut self) -> Result<(), FileError> {
        Ok(())
    }
    fn metadata(&self) -> Result<Metadata, FileError> {
        Ok(Metadata {
            size: 0,
            is_dir: false,
            is_file: true,
            created: 0,
            modified: 0,
            accessed: 0,
        })
    }
}

pub(crate) struct FileHandle {
    descriptor: Box<dyn File>,
    _foo: FileOpenOptions,
}

impl FileHandle {
    pub fn new(_id: u32, descriptor: Box<dyn File>, _foo: FileOpenOptions) -> Self {
        Self { descriptor, _foo }
    }

    pub fn descriptor(&mut self) -> &mut dyn File {
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
