use crate::process::{PROCESSES, current_pid};
use crate::syscall::copy_to_user;
use crate::syscall::errors::{EBADF, ESRCH, file_error_to_errno};
use crate::syscall::table::SyscallPtr;
use crate::tty::TTY;
use alloc::rc::Rc;
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
    let len = core::cmp::min(len, 0x10000);

    let mut kernel_buf = alloc::vec![0u8; len as usize];

    match read_inner(fd, &mut kernel_buf) {
        Ok(n) => {
            copy_to_user(buf_ptr as *mut u8, &kernel_buf[..n]).unwrap();
            info!("read returning {} bytes", n);
            n as u64
        }
        Err(errno) => {
            if (errno as i64) < 0 {
                debug!("read error: errno={:#x}", errno);
                errno
            } else {
                debug!("read unknown error: {}", errno);
                -(errno as i64) as u64
            }
        }
    }
}

fn read_inner(fd: u64, buf: &mut [u8]) -> Result<usize, u64> {
    info!("read_inner called with fd: {}, buf len: {}", fd, buf.len());
    let mut processes = PROCESSES.lock();
    let curr_pid = current_pid();
    let process = processes
        .iter_mut()
        .find(|p| p.pid == curr_pid)
        .ok_or((-ESRCH) as u64)?;
    let file_handle = Rc::clone(process.get_file_handle(fd).ok_or((-EBADF) as u64)?);
    let handle = file_handle;
    let mut handle = handle.lock();
    drop(processes);

    // allow blocking read on stdin, but not on other fds
    if fd == 0 {
        unsafe {
            TTY.force_unlock(); // todo: Fix this.
        }
    }
    let read_res = handle.read(buf).map_err(|e| {
        let errno = file_error_to_errno(&e);
        log::error!(
            "read_inner: read error for fd {}: {:?}, errno={}",
            fd,
            e,
            errno as i64
        );
        errno
    })?;
    Ok(read_res)
}
