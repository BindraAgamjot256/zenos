use crate::disk::FileError;
use crate::disk::vfs::{File, Metadata, SeekFrom};
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

pub struct FileHandle {
    descriptor: Box<dyn File>,
    pub foo: FileOpenOptions,
}

impl FileHandle {
    pub fn new(_id: u32, descriptor: Box<dyn File>, foo: FileOpenOptions) -> Self {
        Self { descriptor, foo }
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
    #[derive(Debug, Clone, Copy)]
    pub struct FileOpenOptions: i32 {
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
        FileOpenOptions::empty()
    }
}

#[cfg(feature = "run-kunittest")]
mod tests {
    use super::*;
    use crate::Test;
    use crate::test_assert_eq as assert_eq;

    #[zenos_macros::test]
    pub fn test_file_open_options_default() -> Option<()> {
        let opts = FileOpenOptions::default();
        crate::test_assert!(opts.is_empty());
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
        let opts = FileOpenOptions::from_bits_truncate(0b0101);
        crate::test_assert!(opts.contains(FileOpenOptions::READ_ONLY));
        crate::test_assert!(opts.contains(FileOpenOptions::CREATE));
        crate::test_assert!(!opts.contains(FileOpenOptions::WRITE_ONLY));
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_file_open_options_bits_values() -> Option<()> {
        assert_eq!(FileOpenOptions::READ_ONLY.bits(), 0b0001);
        assert_eq!(FileOpenOptions::WRITE_ONLY.bits(), 0b0010);
        assert_eq!(FileOpenOptions::CREATE.bits(), 0b0100);
        assert_eq!(FileOpenOptions::TRUNCATE.bits(), 0b1000);
        Some(())
    }
}
