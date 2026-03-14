use crate::syscall::copy_string;
use crate::syscall::errors::{EACCES, EINVAL, file_error_to_errno};
use crate::syscall::table::SyscallPtr;
use bitflags::bitflags;
use log::{debug, error};
use zenos_macros::syscall;

const MAX_PATH_LEN: usize = 4096;

bitflags! {
    struct Mode: u32{
        const F_OK = 0; // Check for existence of file
        const R_OK = 4; // Check for read permission
        const W_OK = 2; // Check for write permission
        const X_OK = 1; // Check for execute permission
    }
}

#[syscall(21)]
fn access(rdi: u64, rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    let user_ptr = rdi as *const u8;
    let mode = match Mode::from_bits(rsi as u32) {
        Some(m) => m,
        None => return (-EINVAL) as u64,
    };

    // Step 1: copy a user-space path into kernel buffer
    let buf = match copy_string(user_ptr, MAX_PATH_LEN) {
        Ok(b) => b,
        Err(e) => {
            error!(
                "access syscall: failed to copy filename from user pointer {:#x}: error code {}",
                user_ptr as usize,
                -(e as i64)
            );
            return e;
        }
    };

    // Step 2: convert to Rust str
    let file_name = buf.as_str();
    debug!("access syscall: filename='{}'", file_name);

    // Step 3: delegate to inner function
    match access_inner(file_name, mode) {
        Ok(()) => 0,
        Err(errno) => errno,
    }
}

fn access_inner(file_name: &str, mode: Mode) -> Result<(), u64> {
    // For now, we just check if the file exists. In a real implementation, we'd also check permissions.
    let vfs = crate::disk::FS.lock();
    match vfs.open_file(file_name) {
        Ok(f) => {
            let locked = f.lock();
            let perms = &locked.perms;
            if mode.contains(Mode::R_OK)
                && !perms.intersects(
                    crate::disk::vfs::Permissions::OWNER_READ
                        | crate::disk::vfs::Permissions::GROUP_READ
                        | crate::disk::vfs::Permissions::OTHER_READ,
                )
            {
                error!("access syscall: file '{}' is not readable", file_name);
                return Err(-EACCES as u64);
            }
            if mode.contains(Mode::W_OK)
                && !perms.intersects(
                    crate::disk::vfs::Permissions::OWNER_WRITE
                        | crate::disk::vfs::Permissions::GROUP_WRITE
                        | crate::disk::vfs::Permissions::OTHER_WRITE,
                )
            {
                error!("access syscall: file '{}' is not writable", file_name);
                return Err(-EACCES as u64);
            }
            if mode.contains(Mode::X_OK)
                && !perms.intersects(
                    crate::disk::vfs::Permissions::OWNER_EXEC
                        | crate::disk::vfs::Permissions::GROUP_EXEC
                        | crate::disk::vfs::Permissions::OTHER_EXEC,
                )
            {
                error!("access syscall: file '{}' is not executable", file_name);
                return Err(-EACCES as u64);
            }
            Ok(())
        }
        Err(e) => {
            error!("access syscall: failed to access '{}': {:?}", file_name, e);
            Err(file_error_to_errno(&e))
        }
    }
}
