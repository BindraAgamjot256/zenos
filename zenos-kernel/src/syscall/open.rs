use crate::disk::FS;
use crate::disk::vfs::Permissions;
use crate::process::PROCESSES;
use crate::process::file_handles::FileOpenOptions;
use crate::syscall::copy_from_user;
use crate::syscall::errors::{EFAULT, EINVAL, EMFILE, ESRCH, file_error_to_errno};
use crate::syscall::table::SyscallPtr;
use alloc::string::{String, ToString};
use alloc::vec::{Vec};
use log::{debug, error, info};
use zenos_macros::syscall;

const MAX_PATH_LEN: usize = 4096;

#[syscall(2)]
fn open(rdi: u64, rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    let user_ptr = rdi as *const u8;
    let flags = FileOpenOptions::from_bits_truncate(rsi);

    debug!("syscall open: buf={:#x}", user_ptr as usize);

    // Step 1: copy a user-space path into kernel buffer
    let buf = match copy_string(user_ptr, MAX_PATH_LEN) {
        Ok(b) => b,
        Err(e) => {
            error!(
                "open syscall: failed to copy filename from user pointer {:#x}: error code {}",
                user_ptr as usize,
                -(e as i64)
            );
            return e;
        }
    };

    // Step 2: convert to Rust str
    let file_name = buf.as_str();
    debug!("open syscall: filename='{}', flags={:?}", file_name, flags);

    // Step 3: delegate to inner function
    match open_inner(file_name, flags) {
        Ok(fd) => {
            info!("Opened file '{}' with fd {}", file_name, fd);
            fd
        }
        Err(errno) => errno,
    }
}

fn copy_string(user_ptr: *const u8, max_len: usize) -> Result<String, u64> {
    let mut vec = Vec::new();
    unsafe {
        for i in 0..max_len {
            let byte = copy_from_user(user_ptr.add(i), 1).map_err(|_| -EFAULT as u64)?[0];
            vec.push(byte);
            if byte == 0 {
                break;
            }
        }
    }
    let s = String::from_utf8(vec).map_err(|e| {
        error!("invalid UTF-8 in filename: {}", e);
        -EINVAL as u64
    })?;
    Ok(s.trim_end_matches('\0').to_string())
}

pub(crate) fn open_inner(file_name: &str, foo: FileOpenOptions) -> Result<u64, u64> {
    use x86_64::instructions::interrupts;

    debug!("open_inner: attempting to lock FS for '{}'", file_name);

    // Disable interrupts to prevent deadlock with spinlocks during preemption
    let file = interrupts::without_interrupts(|| {
        let fs = FS.lock();
        debug!("open_inner: FS lock acquired for '{}'", file_name);
        let res = fs.open_file(file_name);
        debug!(
            "open_inner: open_file result for '{}': {:?}",
            file_name, res
        );
        if res.is_err() {
            if foo.contains(FileOpenOptions::CREATE) {
                let res = fs.create_file(
                    file_name,
                    (Permissions::OWNER_READ | Permissions::OWNER_WRITE | Permissions::OWNER_EXEC)
                        | (Permissions::GROUP_READ)
                        | (Permissions::OTHER_READ),
                );
                if res.is_err() {
                    return Err(file_error_to_errno(&res.err().unwrap()));
                }
            } else {
                return Err(file_error_to_errno(&res.err().unwrap()));
            }
        }
        Ok(fs.open_file(file_name).unwrap())
    })?;

    let process = crate::process::current_pid();
    let mut binding = PROCESSES.lock();
    let process = binding
        .iter_mut()
        .find(move |proc| proc.pid == process)
        .ok_or(ESRCH as u64)?;
    let fd = process
        .add_file_handle(file, foo)
        .map_err(|_| EMFILE as u64)?;
    Ok(fd)
}
