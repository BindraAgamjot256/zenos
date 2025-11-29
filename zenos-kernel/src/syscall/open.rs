use crate::fs::{FS, FileWrapper};
use crate::process::file_handles::FileHandle;
use crate::syscall::copy_from_user;
use crate::syscall::table::SyscallPtr;
use crate::syscall::write::write_inner;
use alloc::boxed::Box;
use log::{debug, info};

#[syscall_macro::syscall(2)]
fn open(rdi: u64, rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    // open(buf, len)
    let buf_ptr = rdi;
    let len = rsi;
    let mut ret = 0;

    debug!("syscall open: buf={:#x}, len={:#x}", buf_ptr, len);

    let ptr = copy_from_user(buf_ptr as *const u8, len as usize);
    if ptr.is_err() {
        ret = u64::MAX;
    }
    let ptr = &ptr.unwrap();
    let file_name_buf = unsafe { str::from_utf8_unchecked(ptr) };
    let val = open_inner(file_name_buf);
    if val.is_some() {
        ret = val.unwrap();
        info!("Opened file '{}' with fd {}", file_name_buf, ret);
    } else {
        ret = u64::MAX;
    }
    ret
}

pub(crate) fn open_inner(file_name: &str) -> Option<u64> {
    {
        let fs = FS.lock();
        fs.root_dir().open_file(file_name).ok()?;
    }
    let the_box = Box::new(FileWrapper::new(alloc::string::String::from(file_name)));
    let fd = {
        let mut processes = crate::process::PROCESSES.lock();
        let curr_pid = unsafe { *crate::percpu::get_percpu_data() }.curr_pid;
        let process = processes.iter_mut().find(|p| p.pid == curr_pid)?;
        let fd = process.add_file_handle(the_box).ok()?;
        fd
    };
    Some(fd)
}
