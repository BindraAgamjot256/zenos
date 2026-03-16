use crate::syscall::copy_from_user;
use crate::syscall::errors::{EBADF, EFAULT, ESRCH, file_error_to_errno};
use crate::syscall::table::SyscallPtr;
use log::{debug, info};
use zenos_macros::syscall;

#[syscall(1)]
fn write(rdi: u64, rsi: u64, rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    // write(fd, buf, len)
    let fd = rdi;
    let buf_ptr = rsi;
    let len = rdx;

    debug!(
        "syscall write: fd={:#x}, buf={:#x}, len={:#x}",
        fd, buf_ptr, len
    );

    let buf = match copy_from_user(buf_ptr as *const u8, len as usize) {
        Ok(b) => b,
        Err(_) => return (-EFAULT) as u64,
    };

    write_inner(&buf, fd).unwrap_or_else(|errno| errno)
}

pub(crate) fn write_inner(buf: &[u8], fd: u64) -> Result<u64, u64> {
    let process = unsafe { crate::process::current_proc_mut() }.ok_or((-ESRCH) as u64)?;
    info!(
        "write_inner: pid={}, fd={}, len={}",
        process.pid,
        fd,
        buf.len()
    );
    let file_handle = process.get_file_handle(fd).ok_or((-EBADF) as u64)?;
    let handle = &mut *file_handle;
    let mut handle = handle.lock();
    let write_res = handle.write(buf).map_err(|e| {
        let errno = file_error_to_errno(&e);
        debug!("write_inner: write error for fd {}: {:?}", fd, e);
        errno
    })?;
    info!("write_inner: wrote {} bytes to fd {}", write_res, fd);
    Ok(write_res as u64)
}
