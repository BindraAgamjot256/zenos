use crate::disk::vfs::File;
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
    let mut processes = crate::process::PROCESSES.lock();
    let curr_pid = unsafe { *crate::percpu::get_percpu_data() }.curr_pid;
    let process = processes
        .iter_mut()
        .find(|p| p.pid == curr_pid)
        .ok_or((-ESRCH) as u64)?;
    info!(
        "write_inner: pid={}, fd={}, len={}",
        curr_pid,
        fd,
        buf.len()
    );
    let file_handle = process.get_file_handle(fd).ok_or((-EBADF) as u64)?;
    let handle = &mut *file_handle.descriptor();
    let written = File::write(handle, buf).map_err(|e| file_error_to_errno(&e))?;
    Ok(written as u64)
}
