//! POSIX-compatible errno values for syscall returns.
//! On error, syscalls return the negated errno value (e.g., -ENOENT).
#![allow(dead_code)]

use crate::disk::FileError;
// Standard POSIX errno values (negated for syscall returns)
pub const EPERM: i64 = 1; // Operation not permitted
pub const ENOENT: i64 = 2; // No such file or directory
pub const ESRCH: i64 = 3; // No such process
pub const EINTR: i64 = 4; // Interrupted system call
pub const EIO: i64 = 5; // I/O error
pub const ENXIO: i64 = 6; // No such device or address
pub const E2BIG: i64 = 7; // Argument list too long
pub const ENOEXEC: i64 = 8; // Exec format error
pub const EBADF: i64 = 9; // Bad file descriptor
pub const ECHILD: i64 = 10; // No child processes
pub const EAGAIN: i64 = 11; // Try again (also EWOULDBLOCK)
pub const ENOMEM: i64 = 12; // Out of memory
pub const EACCES: i64 = 13; // Permission denied
pub const EFAULT: i64 = 14; // Bad address
pub const ENOTBLK: i64 = 15; // Block device required
pub const EBUSY: i64 = 16; // Device or resource busy
pub const EEXIST: i64 = 17; // File exists
pub const EXDEV: i64 = 18; // Cross-device link
pub const ENODEV: i64 = 19; // No such device
pub const ENOTDIR: i64 = 20; // Not a directory
pub const EISDIR: i64 = 21; // Is a directory
pub const EINVAL: i64 = 22; // Invalid argument
pub const ENFILE: i64 = 23; // File table overflow
pub const EMFILE: i64 = 24; // Too many open files
pub const ENOTTY: i64 = 25; // Not a typewriter
pub const ETXTBSY: i64 = 26; // Text file busy
pub const EFBIG: i64 = 27; // File too large
pub const ENOSPC: i64 = 28; // No space left on device
pub const ESPIPE: i64 = 29; // Illegal seek
pub const EROFS: i64 = 30; // Read-only file system
pub const EMLINK: i64 = 31; // Too many links
pub const EPIPE: i64 = 32; // Broken pipe
pub const EDOM: i64 = 33; // Math argument out of domain
pub const ERANGE: i64 = 34; // Math result not representable
pub const EDEADLK: i64 = 35; // Resource deadlock would occur
pub const ENAMETOOLONG: i64 = 36; // File name too long
pub const ENOLCK: i64 = 37; // No record locks available
pub const ENOSYS: i64 = 38; // Function not implemented
pub const ENOTEMPTY: i64 = 39; // Directory not empty
pub const ELOOP: i64 = 40; // Too many symbolic links encountered
pub const ENODATA: i64 = 61; // No data available
pub const EOVERFLOW: i64 = 75; // Value too large for defined data type

/// Convert a FileError to a negated errno value suitable for syscall return.
pub fn file_error_to_errno(err: &FileError) -> u64 {
    let errno = match err {
        FileError::UnsupportedOperation => ENOSYS,
        FileError::InvalidDescriptor => EBADF,
        FileError::ReadError => EIO,
        FileError::WriteError => EIO,
        FileError::SeekError => ESPIPE,
        FileError::InvalidFileDescriptor => EBADF,
        FileError::NotFound => ENOENT,
        FileError::AlreadyExists => EEXIST,
        FileError::DirectoryNotEmpty => ENOTEMPTY,
        FileError::Other(_) => EIO,
    };
    (-errno) as u64
}

#[cfg(feature = "run-kunittest")]
mod tests {
    use super::*;
    use crate::Test;
    use crate::test_assert_eq as assert_eq;

    #[zenos_macros::test]
    pub fn test_errno_values_match_posix() -> Option<()> {
        // Verify key errno values match POSIX standard
        assert_eq!(ENOENT, 2);
        assert_eq!(EIO, 5);
        assert_eq!(EBADF, 9);
        assert_eq!(ENOMEM, 12);
        assert_eq!(EACCES, 13);
        assert_eq!(EFAULT, 14);
        assert_eq!(EEXIST, 17);
        assert_eq!(EINVAL, 22);
        assert_eq!(ENOSYS, 38);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_file_error_to_errno_not_found() -> Option<()> {
        let err = FileError::NotFound;
        let errno = file_error_to_errno(&err);
        assert_eq!(errno, (-ENOENT) as u64);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_file_error_to_errno_already_exists() -> Option<()> {
        let err = FileError::AlreadyExists;
        let errno = file_error_to_errno(&err);
        assert_eq!(errno, (-EEXIST) as u64);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_file_error_to_errno_bad_fd() -> Option<()> {
        let err = FileError::InvalidDescriptor;
        let errno = file_error_to_errno(&err);
        assert_eq!(errno, (-EBADF) as u64);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_file_error_to_errno_io_errors() -> Option<()> {
        let read_err = FileError::ReadError;
        let write_err = FileError::WriteError;
        assert_eq!(file_error_to_errno(&read_err), (-EIO) as u64);
        assert_eq!(file_error_to_errno(&write_err), (-EIO) as u64);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_file_error_to_errno_seek_error() -> Option<()> {
        let err = FileError::SeekError;
        let errno = file_error_to_errno(&err);
        assert_eq!(errno, (-ESPIPE) as u64);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_file_error_to_errno_dir_not_empty() -> Option<()> {
        let err = FileError::DirectoryNotEmpty;
        let errno = file_error_to_errno(&err);
        assert_eq!(errno, (-ENOTEMPTY) as u64);
        Some(())
    }

    #[zenos_macros::test]
    pub fn test_negated_errno_is_negative_in_i64() -> Option<()> {
        // Verify that negated errno values are negative when interpreted as i64
        let errno = file_error_to_errno(&FileError::NotFound);
        let as_i64 = errno as i64;
        crate::test_assert!(as_i64 < 0);
        assert_eq!(as_i64, -ENOENT);
        Some(())
    }
}
