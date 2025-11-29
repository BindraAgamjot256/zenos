use crate::syscall::open::open_inner;
use crate::syscall::table::SyscallPtr;
use crate::syscall::{copy_from_user, copy_to_user};
use log::{debug, error, info};

#[syscall_macro::syscall(0)]
fn read(rdi: u64, rsi: u64, rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    // read(fd, buf, len)
    let mut ret = 0;
    let fd = rdi;
    let buf_ptr = rsi;
    let len = rdx;
    debug!(
        "syscall read: fd={:#x}, buf={:#x}, len={:#x}",
        fd, buf_ptr, len
    );
    let buf = copy_from_user(buf_ptr as *mut u8, len as usize);
    if buf.is_err() {
        ret = u64::MAX;
        return ret;
    }
    let mut buf = buf.unwrap();
    let val = read_inner(fd, &mut buf);
    ret = if val.is_ok() {
        copy_to_user(buf_ptr as *mut u8, &mut buf).unwrap(); // we can unwrap here because we already copied from user, so we know it's valid.
        val.unwrap() as u64
    } else {
        error!("read failed");
        u64::MAX
    };
    ret
}

fn read_inner(fd: u64, buf: &mut [u8]) -> Result<usize, u64> {
    info!("read_inner called with fd: {}, buf len: {}", fd, buf.len());
    let mut processes = crate::process::PROCESSES.lock();
    let curr_pid = unsafe { *crate::percpu::get_percpu_data() }.curr_pid;
    let process = processes
        .iter_mut()
        .find(|p| p.pid == curr_pid)
        .ok_or(u64::MAX)?;
    let file_table = process.get_file_handle(fd).ok_or(u64::MAX)?;
    let handle = &mut *file_table.descriptor();
    let read_res = crate::process::file_handles::FileLike::read(handle, buf).map_err(|e| {
        info!("err:{:#?}", e);
        u64::MAX
    })?;
    info!("read_inner read {} bytes", read_res);
    Ok(read_res)
}
