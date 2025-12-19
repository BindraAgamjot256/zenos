use crate::disk::vfs::SeekFrom;
use crate::syscall::table::SyscallPtr;
use log::debug;
use zenos_macros::syscall;

#[syscall(8)]
fn lseek(rdi: u64, rsi: u64, rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    // lseek(fd, offset, whence)
    let fd = rdi;
    let offset = rsi as i64;
    let whence = rdx;
    let mut ret = 0;

    debug!(
        "syscall lseek: fd={}, offset={}, whence={}",
        fd, offset, whence
    );

    ret = seek_inner(fd, offset, whence).unwrap_or(u64::MAX);

    ret
}

fn seek_inner(fd: u64, offset: i64, whence: u64) -> Option<u64> {
    let pid = unsafe { *crate::percpu::get_percpu_data() }.curr_pid;
    let mut processes = crate::process::PROCESSES.lock();
    let process = processes.iter_mut().find(|p| p.pid == pid)?;
    let file_handle = process.get_file_handle(fd)?;
    let seek_from = match whence {
        0 => SeekFrom::Start(offset as u64),
        1 => SeekFrom::Current(offset),
        2 => SeekFrom::End(offset),
        _ => return None,
    };
    let new_pos = file_handle.descriptor().seek(seek_from).ok()?;
    Some(new_pos)
}
