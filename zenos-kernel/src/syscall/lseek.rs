use crate::disk::vfs::SeekFrom;
use crate::syscall::errors::{EBADF, EINVAL, ESRCH, file_error_to_errno};
use crate::syscall::table::SyscallPtr;
use log::debug;
use zenos_macros::syscall;

#[syscall(8)]
fn lseek(rdi: u64, rsi: u64, rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    // lseek(fd, offset, whence)
    let fd = rdi;
    let offset = rsi as i64;
    let whence = rdx;

    debug!(
        "syscall lseek: fd={}, offset={}, whence={}",
        fd, offset, whence
    );

    seek_inner(fd, offset, whence).unwrap_or_else(|errno| errno)
}

fn seek_inner(fd: u64, offset: i64, whence: u64) -> Result<u64, u64> {
    let pid = unsafe { *crate::percpu::get_percpu_data() }.curr_pid;
    let mut processes = crate::process::PROCESSES.lock();
    let process = processes
        .iter_mut()
        .find(|p| p.pid == pid)
        .ok_or((-ESRCH) as u64)?;
    let file_handle = process.get_file_handle(fd).ok_or((-EBADF) as u64)?;
    let seek_from = match whence {
        0 => SeekFrom::Start(offset as u64),
        1 => SeekFrom::Current(offset),
        2 => SeekFrom::End(offset),
        _ => return Err((-EINVAL) as u64),
    };
    let new_pos = file_handle
        .descriptor()
        .seek(seek_from)
        .map_err(|e| file_error_to_errno(&e))?;
    Ok(new_pos)
}
