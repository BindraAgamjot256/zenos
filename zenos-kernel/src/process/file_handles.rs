use crate::disk::FileError;
pub(crate) use crate::disk::vfs::FileOpenOptions;
use crate::disk::vfs::{DirEntry, FileType, Inode, InodeOps, Permissions};
use crate::process::block_current_process;
use crate::{kprint, kprintln, tty};
use alloc::sync::Arc;
use alloc::vec::Vec;
use core::sync::atomic::{AtomicBool, Ordering};
use spin::Mutex;

#[derive(Clone)]
pub struct Stdout {
    _pid: u64,
}

#[derive(Clone)]
pub struct Stderr {
    _pid: u64,
}

#[derive(Clone)]
pub struct Stdin {
    pid: u64,
}

impl Stdout {
    pub fn new(pid: u64) -> Self {
        Self { _pid: pid }
    }
}

impl Stderr {
    pub fn new(pid: u64) -> Self {
        Self { _pid: pid }
    }
}

impl Stdin {
    pub fn new(pid: u64) -> Self {
        Self { pid }
    }
}

impl InodeOps for Stdout {
    fn read(&mut self, _offset: u64, _buf: &mut [u8]) -> Result<usize, FileError> {
        Err(FileError::UnsupportedOperation)
    }

    fn write(&mut self, _offset: u64, buf: &[u8]) -> Result<usize, FileError> {
        let buf = core::str::from_utf8(buf).unwrap_or("<invalid utf-8>");
        kprint!("{}", buf);
        Ok(buf.len())
    }

    fn truncate(&mut self, _size: u64) -> Result<(), FileError> {
        Err(FileError::UnsupportedOperation)
    }

    fn sync(&mut self) -> Result<(), FileError> {
        Ok(())
    }

    fn lookup(&mut self, _name: &str) -> Result<Arc<Mutex<Inode>>, FileError> {
        Err(FileError::UnsupportedOperation)
    }

    fn create(
        &mut self,
        _name: &str,
        _kind: FileType,
        _perms: Permissions,
    ) -> Result<Arc<Mutex<Inode>>, FileError> {
        Err(FileError::UnsupportedOperation)
    }

    fn read_dir(&mut self) -> Result<Vec<DirEntry>, FileError> {
        Err(FileError::UnsupportedOperation)
    }
}

impl InodeOps for Stderr {
    fn read(&mut self, _offset: u64, _buf: &mut [u8]) -> Result<usize, FileError> {
        Err(FileError::UnsupportedOperation)
    }

    fn write(&mut self, _offset: u64, buf: &[u8]) -> Result<usize, FileError> {
        let buf = core::str::from_utf8(buf).unwrap_or("<invalid utf-8>");
        kprintln!("{}", buf);
        Ok(buf.len())
    }

    fn truncate(&mut self, _size: u64) -> Result<(), FileError> {
        Err(FileError::UnsupportedOperation)
    }

    fn sync(&mut self) -> Result<(), FileError> {
        Ok(())
    }

    fn lookup(&mut self, _name: &str) -> Result<Arc<Mutex<Inode>>, FileError> {
        Err(FileError::UnsupportedOperation)
    }

    fn create(
        &mut self,
        _name: &str,
        _kind: FileType,
        _perms: Permissions,
    ) -> Result<Arc<Mutex<Inode>>, FileError> {
        Err(FileError::UnsupportedOperation)
    }

    fn read_dir(&mut self) -> Result<Vec<DirEntry>, FileError> {
        Err(FileError::UnsupportedOperation)
    }
}

impl InodeOps for Stdin {
    fn read(&mut self, _offset: u64, buffer: &mut [u8]) -> Result<usize, FileError> {
        // Block until we get at least one byte or a newline
        let mut count = 0;

        while count < buffer.len() {
            STDIN_BLOCKED.store(true, Ordering::SeqCst);

            // Try to read non-blocking first
            let mut temp = [0u8; 1];
            let n = tty::tty_read_nonblocking(self.pid, &mut temp);
            if n.is_err() {
                continue;
            }

            let n = n.unwrap();
            if n > 0 {
                let byte = temp[0];
                if byte == b'\n' {
                    buffer[count] = byte;
                    STDIN_BLOCKED.store(false, Ordering::SeqCst);
                    return Ok(count + 1);
                } else if byte == b'\x08' {
                    // Backspace: remove last character from buffer
                    if count > 0 {
                        count -= 1;
                    }
                } else {
                    buffer[count] = byte;
                    count += 1;
                }
            } else {
                // No data available, block and yield to scheduler
                block_current_process(&STDIN_BLOCKED);
                // Enable interrupts and halt atomically - keyboard interrupt will wake us
                unsafe {
                    core::arch::asm!("sti; hlt", options(nomem, nostack));
                }
            }
        }

        STDIN_BLOCKED.store(false, Ordering::SeqCst);
        Ok(count)
    }

    fn write(&mut self, _offset: u64, _buf: &[u8]) -> Result<usize, FileError> {
        todo!()
    }

    fn truncate(&mut self, _size: u64) -> Result<(), FileError> {
        Ok(())
    }

    fn sync(&mut self) -> Result<(), FileError> {
        Ok(())
    }

    fn lookup(&mut self, _name: &str) -> Result<Arc<Mutex<Inode>>, FileError> {
        Err(FileError::NotADirectory)
    }

    fn create(
        &mut self,
        _name: &str,
        _kind: FileType,
        _perms: Permissions,
    ) -> Result<Arc<Mutex<Inode>>, FileError> {
        Err(FileError::NotADirectory)
    }

    fn read_dir(&mut self) -> Result<Vec<DirEntry>, FileError> {
        Err(FileError::NotADirectory)
    }
}
/// Global flag used to wake processes blocked on stdin when keyboard input arrives.
/// The keyboard handler sets this to `false` when new input is available.
pub static STDIN_BLOCKED: AtomicBool = AtomicBool::new(false);
