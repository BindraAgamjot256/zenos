use crate::kprint;
use alloc::boxed::Box;
use bitflags::bitflags;
use core::any::Any;
use core::cmp::Ordering;
use core::fmt::Debug;
use fatfs::{Error, IoBase, IoError, Read, Seek, SeekFrom, Write};

#[derive(Clone)]
pub struct Stdout;
#[derive(Clone)]
pub struct Stderr;
#[derive(Clone)]
pub struct Stdin;

impl IoBase for Stdout {
    type Error = FileError;
}

impl FileLike for Stdout {
    fn read(&mut self, _buffer: &mut [u8]) -> Result<usize, Self::Error> {
        Err(FileError::UnsupportedOperation)
    }

    fn write(&mut self, buffer: &[u8]) -> Result<usize, Self::Error> {
        unsafe { kprint!("{}", core::str::from_utf8_unchecked(buffer)) };
        Ok(buffer.len())
    }

    fn seek(&mut self, _position: SeekFrom) -> Result<u64, <Self as IoBase>::Error> {
        Err(FileError::UnsupportedOperation)
    }
}

impl IoBase for Stderr {
    type Error = FileError;
}

impl FileLike for Stderr {
    fn read(&mut self, _buffer: &mut [u8]) -> Result<usize, Self::Error> {
        Err(FileError::UnsupportedOperation)
    }

    fn write(&mut self, buffer: &[u8]) -> Result<usize, Self::Error> {
        unsafe { kprint!("{}", core::str::from_utf8_unchecked(buffer)) };
        Ok(buffer.len())
    }

    fn seek(&mut self, _position: SeekFrom) -> Result<u64, <Self as IoBase>::Error> {
        Err(FileError::UnsupportedOperation)
    }
}

impl IoBase for Stdin {
    type Error = FileError;
}

impl FileLike for Stdin {
    fn read(&mut self, _buffer: &mut [u8]) -> Result<usize, Self::Error> {
        todo!("Stdin read not implemented")
    }

    fn write(&mut self, _buffer: &[u8]) -> Result<usize, Self::Error> {
        Err(FileError::UnsupportedOperation)
    }

    fn seek(&mut self, _position: SeekFrom) -> Result<u64, <Self as IoBase>::Error> {
        Err(FileError::UnsupportedOperation)
    }
}

pub(crate) trait FileLike: IoBase {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, <Self as IoBase>::Error>;
    fn write(&mut self, buffer: &[u8]) -> Result<usize, <Self as IoBase>::Error>;
    fn seek(&mut self, position: SeekFrom) -> Result<u64, <Self as IoBase>::Error>;
}

impl<T> FileLike for T
where
    T: Read + Write + Seek,
{
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, <Self as IoBase>::Error> {
        Seek::seek(self, SeekFrom::Start(0))?;
        Read::read(self, buffer)
    }

    fn write(&mut self, buffer: &[u8]) -> Result<usize, <Self as IoBase>::Error> {
        Write::write(self, buffer)
    }

    fn seek(&mut self, position: SeekFrom) -> Result<u64, <Self as IoBase>::Error> {
        Seek::seek(self, position)
    }
}

pub(crate) struct FileHandle {
    id: u32,
    descriptor: Box<dyn FileLike<Error = FileError>>,
    foo: FileOpenOptions,
}

impl FileHandle {
    pub fn new(
        id: u32,
        descriptor: Box<dyn FileLike<Error = FileError>>,
        foo: FileOpenOptions,
    ) -> Self {
        Self {
            id,
            descriptor,
            foo,
        }
    }

    pub fn id(&self) -> u32 {
        self.id
    }

    pub fn descriptor(&mut self) -> &mut dyn FileLike<Error = FileError> {
        self.descriptor.as_mut()
    }
}

impl Debug for FileHandle {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FileHandle")
            .field("id", &self.id)
            .field("type", &self.descriptor.type_id())
            .finish()
    }
}

impl Eq for FileHandle {}

impl PartialEq<Self> for FileHandle {
    fn eq(&self, other: &Self) -> bool {
        self.id == other.id
    }
}

impl PartialOrd<Self> for FileHandle {
    fn partial_cmp(&self, other: &Self) -> Option<Ordering> {
        Some(self.id.cmp(&other.id))
    }
}

impl Ord for FileHandle {
    fn cmp(&self, other: &Self) -> Ordering {
        self.id.cmp(&other.id)
    }
}

#[derive(Debug)]
pub enum FileError {
    UnsupportedOperation,
    InvalidDescriptor,
    ReadError,
    WriteError,
    SeekError,
    IoError(Error<()>),
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
                other => FileError::IoError(core::mem::transmute(other)),
            }
        }
    }
}

bitflags! {
    #[derive(Default, Debug, Clone, Copy)]
    pub struct FileOpenOptions: u64 {
        const READ = 0b0001;
        const WRITE = 0b0010;
        const CREATE = 0b0100;
        const TRUNCATE = 0b1000;
        //todo: more options
    }
}
