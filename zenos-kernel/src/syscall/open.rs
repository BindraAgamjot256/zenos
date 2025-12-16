use crate::disk::vfs::File;
use crate::disk::{FileError, FS};
use crate::process::file_handles::FileOpenOptions;
use crate::syscall::copy_from_user;
use crate::syscall::table::SyscallPtr;
use alloc::boxed::Box;
use core::ffi::CStr;
use log::{debug, info};

#[syscall_macro::syscall(2)]
fn open(rdi: u64, rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    // open(buf, len)
    let buf_ptr = rdi;
    let foo = FileOpenOptions::from_bits_truncate(rsi);
    let mut ret = 0;

    debug!("syscall open: buf={:#x}", buf_ptr,);

    let cstr = unsafe { CStr::from_ptr(buf_ptr as *const i8) };
    let ptr = copy_from_user(
        cstr.to_bytes_with_nul().as_ptr() as *mut u8,
        cstr.to_bytes_with_nul().len(),
    );
    if ptr.is_err() {
        ret = u64::MAX;
        return ret;
    }
    let ptr = &ptr.unwrap();
    let file_name_buf = CStr::from_bytes_with_nul(ptr);
    if file_name_buf.is_err() {
        ret = u64::MAX;
        return ret;
    }
    let file_name_buf = file_name_buf.unwrap().to_str().unwrap();
    let val = open_inner(file_name_buf, foo);
    if val.is_some() {
        ret = val.unwrap();
        info!("Opened file '{}' with fd {}", file_name_buf, ret);
    } else {
        ret = u64::MAX;
    }
    ret
}

pub(crate) fn open_inner(file_name: &str, foo: FileOpenOptions) -> Option<u64> {
    let fs = FS.lock();
    let mut root = match fs.root_dir() {
        Ok(r) => r,
        Err(e) => {
            info!("Failed to get root dir: {:?}", e);
            return None;
        }
    };
    let res = root.open_file(file_name);
    match res {
        Ok(_) => {}
        Err(e) => {
            info!("Failed to open file '{}': {:?}", file_name, e);
            match e {
                FileError::NotFound => {
                    if foo.contains(FileOpenOptions::CREATE) {
                        info!("Creating file '{}'", file_name);
                        let res = root.create_file(file_name);
                        match res {
                            Ok(_) => {}
                            Err(e) => {
                                info!("Failed to create file '{}': {:?}", file_name, e);
                                return None;
                            }
                        }
                    } else {
                        return None;
                    }
                }
                _ => {
                    return None;
                }
            }
        }
    }
    let the_box = root.open_file(file_name).unwrap();
    let fd = {
        let mut processes = crate::process::PROCESSES.lock();
        let curr_pid = unsafe { *crate::percpu::get_percpu_data() }.curr_pid;
        let process = processes.iter_mut().find(|p| p.pid == curr_pid)?;
        process.add_file_handle(the_box, foo).ok()?
    };
    Some(fd)
}
