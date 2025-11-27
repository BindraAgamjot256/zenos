use crate::kprint;
use crate::process::Process;
use crate::process::file_handles::FileLike;
use crate::syscall::copy_from_user;
use crate::syscall::table::SyscallPtr;
use log::debug;

#[syscall_macro::syscall(1)]
fn write(rdi: u64, rsi: u64, rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    // write(fd, buf, len)
    let fd = rdi;
    let buf_ptr = rsi;
    let len = rdx;
    let mut ret = 0;

    debug!(
        "syscall write: fd={:#x}, buf={:#x}, len={:#x}",
        fd, buf_ptr, len
    );

    let ptr = copy_from_user(buf_ptr as *const u8, len as usize);
    if ptr.is_err() {
        ret = u64::MAX;
    }
    let mut buf = ptr.unwrap();
    let val = write_inner(&mut buf, fd);
    if val.is_some() {
        ret = val.unwrap();
    } else {
        ret = u64::MAX;
    }
    ret
}

pub(crate) fn write_inner(buf: &[u8], fd: u64) -> Option<u64> {
    let mut processes = crate::process::PROCESSES.lock();
    let curr_pid = unsafe { *crate::percpu::get_percpu_data() }.curr_pid;
    let process = processes.iter_mut().find(|p| p.pid == curr_pid)?;
    let file_table = process.get_file_handle(fd);
    if file_table.is_none() {
        return None;
    }
    let file_table = file_table.unwrap();
    let handle = &mut *file_table.descriptor();
    let write_res = FileLike::write(handle, buf);
    if write_res.is_err() {
        return None;
    }
    Some(write_res.unwrap() as u64)
}
