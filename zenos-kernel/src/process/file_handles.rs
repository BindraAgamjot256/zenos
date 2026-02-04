use crate::disk::FileError;
use crate::disk::vfs::{File, FileType, Metadata, SeekFrom};
use crate::process::block_current_process;
use crate::tty;
use alloc::boxed::Box;
use bitflags::bitflags;
use core::any::Any;
use core::fmt::Debug;
use core::ops::Deref;
use core::sync::atomic::{AtomicBool, Ordering};

#[derive(Clone)]
pub struct Stdout {
    pid: u64,
}

#[derive(Clone)]
pub struct Stderr {
    pid: u64,
}

#[derive(Clone)]
pub struct Stdin {
    pid: u64,
}

impl Stdout {
    pub fn new(pid: u64) -> Self {
        Self { pid }
    }
}

impl Stderr {
    pub fn new(pid: u64) -> Self {
        Self { pid }
    }
}

impl Stdin {
    pub fn new(pid: u64) -> Self {
        Self { pid }
    }
}

impl File for Stdout {
    fn read(&mut self, _buffer: &mut [u8]) -> Result<usize, FileError> {
        Err(FileError::UnsupportedOperation)
    }

    fn write(&mut self, buffer: &[u8]) -> Result<usize, FileError> {
        tty::tty_write(self.pid, buffer).map_err(|_| FileError::WriteError)
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
            ftype: FileType::File,
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
        tty::tty_write(self.pid, buffer).map_err(|_| FileError::WriteError)
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
            ftype: FileType::File,
            created: 0,
            modified: 0,
            accessed: 0,
        })
    }
}

/// Global flag used to wake processes blocked on stdin when keyboard input arrives.
/// The keyboard handler sets this to `false` when new input is available.
pub static STDIN_BLOCKED: AtomicBool = AtomicBool::new(false);

impl File for Stdin {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, FileError> {
        // Block until we get at least one byte or a newline
        let mut count = 0;

        while count < buffer.len() {
            STDIN_BLOCKED.store(true, Ordering::SeqCst);

            // Try to read non-blocking first
            let n = tty::tty_read_nonblocking(self.pid, &mut buffer[count..]);
            if n.is_err() {
                continue;
            }

            let n = n.unwrap();
            if n > 0 {
                // Check if we got a newline
                for i in 0..n {
                    if buffer[count + i] == b'\n' {
                        STDIN_BLOCKED.store(false, Ordering::SeqCst);
                        return Ok(count + i + 1);
                    }
                }
                count += n;
            } else {
                // No data available, block and yield to scheduler
                block_current_process(&STDIN_BLOCKED);
                // Enable interrupts and halt - timer will context switch
                unsafe {
                    core::arch::asm!("sti; hlt", options(nomem, nostack));
                }
            }
        }

        STDIN_BLOCKED.store(false, Ordering::SeqCst);
        Ok(count)
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
            ftype: FileType::File,
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
}
