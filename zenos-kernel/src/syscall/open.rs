use crate::fs::{FS, FileWrapper};
use crate::process::file_handles::FileOpenOptions;
use crate::syscall::copy_from_user;
use crate::syscall::table::SyscallPtr;
use alloc::boxed::Box;
use fatfs::Error;
use log::{debug, info};

#[syscall_macro::syscall(2)]
fn open(rdi: u64, rsi: u64, rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    // open(buf, len)
    let buf_ptr = rdi;
    let len = rsi;
    let foo = FileOpenOptions::from_bits_truncate(rdx);
    let mut ret = 0;

    debug!("syscall open: buf={:#x}, len={:#x}", buf_ptr, len);

    let ptr = copy_from_user(buf_ptr as *const u8, len as usize);
    if ptr.is_err() {
        ret = u64::MAX;
    }
    let ptr = &ptr.unwrap();
    let file_name_buf = unsafe { str::from_utf8_unchecked(ptr) };
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
    {
        let fs = FS.lock();
        let res = fs.root_dir().open_file(file_name);
        match res {
            Ok(_) => {}
            Err(e) => {
                info!("Failed to open file '{}': {:?}", file_name, e);
                match e {
                    Error::NotFound => {
                        if foo.contains(FileOpenOptions::CREATE) {
                            info!("Creating file '{}'", file_name);
                            let res = fs.root_dir().create_file(file_name);
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
                    } // Other errors
                }
            }
        }
    }
    let the_box = Box::new(FileWrapper::new(
        alloc::string::String::from(file_name),
        foo,
    ));
    let fd = {
        let mut processes = crate::process::PROCESSES.lock();
        let curr_pid = unsafe { *crate::percpu::get_percpu_data() }.curr_pid;
        let process = processes.iter_mut().find(|p| p.pid == curr_pid)?;
        process.add_file_handle(the_box, foo).ok()?
    };
    Some(fd)
}
