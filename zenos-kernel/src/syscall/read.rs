use crate::disk::vfs::InodeOps;
use crate::process::file_handles::Stdin;
use crate::syscall::errors::{EBADF, EFAULT, ESRCH, file_error_to_errno};
use crate::syscall::table::SyscallPtr;
use crate::syscall::{copy_from_user, copy_to_user};
use log::{debug, info};
use zenos_macros::syscall;

#[syscall(0)]
fn read(rdi: u64, rsi: u64, rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    // read(fd, buf, len)
    let fd = rdi;
    let buf_ptr = rsi;
    let len = rdx;
    debug!(
        "syscall read: fd={:#x}, buf={:#x}, len={:#x}",
        fd, buf_ptr, len
    );
    let buf = copy_from_user(buf_ptr as *mut u8, len as usize);
    if buf.is_err() {
        return (-EFAULT) as u64;
    }
    let mut buf = buf.unwrap();
    match read_inner(fd, &mut buf) {
        Ok(n) => {
            // Safe to unwrap: we already validated the user pointer above
            copy_to_user(buf_ptr as *mut u8, &buf).unwrap();
            n as u64
        }
        Err(errno) => errno,
    }
}

fn read_inner(fd: u64, buf: &mut [u8]) -> Result<usize, u64> {
    info!("read_inner called with fd: {}, buf len: {}", fd, buf.len());

    // handle stdin fd as a special case
    if fd == 0 {
        let curr_pid = unsafe { *crate::percpu::get_percpu_data() }.curr_pid;
        let mut stdin = Stdin::new(curr_pid);
        let read_res = stdin.read(0, buf).map_err(|e| {
            let errno = file_error_to_errno(&e);
            debug!("read_inner: read error for stdin: {:?}", e);
            errno
        })?;
        return Ok(read_res);
    }
    // For other fds, we need to look up the file handle
    let mut processes = crate::process::PROCESSES.lock();
    let curr_pid = unsafe { *crate::percpu::get_percpu_data() }.curr_pid;
    let process = processes
        .iter_mut()
        .find(|p| p.pid == curr_pid)
        .ok_or((-ESRCH) as u64)?;
    let file_handle = process.get_file_handle(fd).ok_or((-EBADF) as u64)?;
    let handle = &mut *file_handle;
    let read_res = handle.read(buf).map_err(|e| {
        let errno = file_error_to_errno(&e);
        debug!("read_inner: read error for fd {}: {:?}", fd, e);
        errno
    })?;
    info!("read_inner read {} bytes", read_res);
    Ok(read_res)
}
