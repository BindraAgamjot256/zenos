use crate::disk::FS;
use crate::disk::FileError;
use crate::process::file_handles::FileOpenOptions;
use crate::syscall::copy_from_user;
use crate::syscall::errors::{EFAULT, EINVAL, EMFILE, ENOENT, ESRCH, file_error_to_errno};
use crate::syscall::table::SyscallPtr;
use core::ffi::CStr;
use log::{debug, info};
use zenos_macros::syscall;

const MAX_PATH_LEN: usize = 4096;

#[syscall(2)]
fn open(rdi: u64, rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    let user_ptr = rdi as *const u8;
    let flags = FileOpenOptions::from_bits_truncate(rsi);

    debug!("syscall open: buf={:#x}", user_ptr as usize);

    // Step 1: copy a user-space path into kernel buffer
    let buf = match copy_from_user(user_ptr, MAX_PATH_LEN) {
        Ok(b) => b,
        Err(_) => return (-EFAULT) as u64,
    };

    // Step 2: find the NUL terminator
    let path_len = match buf.iter().position(|&c| c == 0) {
        Some(pos) => pos,
        None => return (-EINVAL) as u64,
    };

    // Step 3: convert to Rust str
    let file_name = match str::from_utf8(&buf[..path_len]) {
        Ok(s) => s,
        Err(_) => return (-EINVAL) as u64,
    };

    debug!("open syscall: filename='{}', flags={:?}", file_name, flags);

    // Step 4: delegate to inner function
    match open_inner(file_name, flags) {
        Ok(fd) => {
            info!("Opened file '{}' with fd {}", file_name, fd);
            fd
        }
        Err(errno) => errno,
    }
}

pub(crate) fn open_inner(file_name: &str, foo: FileOpenOptions) -> Result<u64, u64> {
    let fs = FS.lock();
    let mut root = fs.root_dir().map_err(|e| file_error_to_errno(&e))?;

    // Try to open the file, create if necessary
    match root.open_file(file_name) {
        Ok(_) => {}
        Err(FileError::NotFound) => {
            if foo.contains(FileOpenOptions::CREATE) {
                info!("Creating file '{}'", file_name);
                root.create_file(file_name)
                    .map_err(|e| file_error_to_errno(&e))?;
            } else {
                return Err((-ENOENT) as u64);
            }
        }
        Err(e) => return Err(file_error_to_errno(&e)),
    }

    let file_handle = root
        .open_file(file_name)
        .map_err(|e| file_error_to_errno(&e))?;

    // Add file handle to the current process
    let fd = {
        let mut processes = crate::process::PROCESSES.lock();
        let curr_pid = unsafe { *crate::percpu::get_percpu_data() }.curr_pid;
        let process = processes
            .iter_mut()
            .find(|p| p.pid == curr_pid)
            .ok_or((-ESRCH) as u64)?;
        process
            .add_file_handle(file_handle, foo)
            .map_err(|_| (-EMFILE) as u64)?
    };

    Ok(fd)
}
