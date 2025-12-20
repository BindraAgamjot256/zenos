use crate::disk::FS;
use crate::disk::FileError;
use crate::process::file_handles::FileOpenOptions;
use crate::syscall::copy_from_user;
use crate::syscall::errors::{EFAULT, EINVAL, EMFILE, ENOENT, ESRCH, file_error_to_errno};
use crate::syscall::table::SyscallPtr;
use core::ffi::CStr;
use log::{debug, info};
use zenos_macros::syscall;

#[syscall(2)]
fn open(rdi: u64, rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    // open(path, flags)
    let buf_ptr = rdi;
    let foo = FileOpenOptions::from_bits_truncate(rsi);

    debug!("syscall open: buf={:#x}", buf_ptr);

    let cstr = unsafe { CStr::from_ptr(buf_ptr as *const i8) };
    let buf = match copy_from_user(
        cstr.to_bytes_with_nul().as_ptr() as *mut u8,
        cstr.to_bytes_with_nul().len(),
    ) {
        Ok(b) => b,
        Err(_) => return (-EFAULT) as u64,
    };

    let file_name = match CStr::from_bytes_with_nul(&buf) {
        Ok(c) => match c.to_str() {
            Ok(s) => s,
            Err(_) => return (-EINVAL) as u64,
        },
        Err(_) => return (-EINVAL) as u64,
    };

    match open_inner(file_name, foo) {
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

    // Try to open the file
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

    let the_box = root
        .open_file(file_name)
        .map_err(|e| file_error_to_errno(&e))?;

    let fd = {
        let mut processes = crate::process::PROCESSES.lock();
        let curr_pid = unsafe { *crate::percpu::get_percpu_data() }.curr_pid;
        let process = processes
            .iter_mut()
            .find(|p| p.pid == curr_pid)
            .ok_or((-ESRCH) as u64)?;
        process
            .add_file_handle(the_box, foo)
            .map_err(|_| (-EMFILE) as u64)?
    };
    Ok(fd)
}
