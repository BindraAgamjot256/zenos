use crate::disk::vfs::FileType;
use crate::process::PROCESSES;
use crate::syscall::copy_to_user;
use crate::syscall::errors::{EBADF, EFAULT, ESRCH, file_error_to_errno};
use crate::syscall::table::SyscallPtr;
use core::mem::size_of;
use log::{debug, error, info, warn};
use zenos_macros::syscall;

#[repr(C)]
#[derive(Debug, Copy, Clone)]
struct LinuxDirent64 {
    d_ino: u64,
    d_off: u64,
    d_reclen: u16,
    d_type: u8,
    d_name: [u8; 256],
}

fn filetype_to_dtype(filetype: &FileType) -> u8 {
    match filetype {
        FileType::Fifo => 1,
        FileType::CharDevice => 2,
        FileType::Directory => 4,
        FileType::BlockDevice => 6,
        FileType::File => 8,
        FileType::Symlink => 10,
        FileType::Socket => 12,
    }
}

fn align_up(len: usize) -> usize {
    (len + 7) & !7
}

#[syscall(217)]
fn getdents64(rdi: u64, rsi: u64, rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    let fd_idx = rdi;
    let mut user_buf = rsi as *mut u8;
    let buf_size = rdx as usize;

    info!(
        "getdents called: fd_idx={}, user_buf={:?}, buf_size={}",
        fd_idx, user_buf, buf_size
    );

    let guard = PROCESSES.lock();
    let proc = match unsafe { crate::process::current_proc_mut() } {
        Some(p) => p,
        None => {
            error!("Process not found for current CPU");
            return -ESRCH as u64;
        }
    };
    info!("Process found: pid={}", proc.pid);

    let fd = match proc.get_file_handle(fd_idx) {
        Some(fd) => fd.clone(), // Clone the Arc to own it
        None => {
            warn!("Invalid file descriptor {} for pid={}", fd_idx, proc.pid);
            return -EBADF as u64;
        }
    };

    // Drop the PROCESSES lock before doing disk I/O to avoid deadlock.
    // The fd is Arc-cloned, so it's safe to release the process list lock.
    drop(guard);

    let fd_guard = fd.lock();
    let inode_guard_guard = fd_guard.inode.clone();
    let mut inode_guard = inode_guard_guard.lock();

    info!("Reading directory entries for inode {}", inode_guard.num);

    let dentries_result = inode_guard
        .data
        .read_dir()
        .map_err(|e| file_error_to_errno(&e));
    if dentries_result.is_err() {
        error!(
            "Failed to read directory: {:?}",
            dentries_result.as_ref().err()
        );
        return dentries_result.err().unwrap();
    }
    info!("Directory entries read successfully");
    let dentries = dentries_result.unwrap();
    info!("Found {} directory entries", dentries.len());

    let mut bytes_written = 0;
    let mut offset = *fd_guard.cursor.lock(); // resume from last position
    info!("Starting iteration from offset {}", offset);

    for dentry in dentries.iter().skip(offset as usize) {
        let name_bytes = dentry.name.as_bytes();
        let name_len = name_bytes.len().min(255);

        let reclen = align_up(
            size_of::<u64>() + size_of::<u64>() + size_of::<u16>() + size_of::<u8>() + name_len + 1,
        );
        if bytes_written + reclen > buf_size {
            info!(
                "Buffer full: bytes_written={} reclen={} buf_size={}",
                bytes_written, reclen, buf_size
            );
            break;
        }

        let mut linux_dent = LinuxDirent64 {
            d_ino: dentry.inode.lock().num,
            d_off: 0,
            d_reclen: reclen as u16,
            d_type: filetype_to_dtype(&dentry.file_type),
            d_name: [0u8; 256],
        };
        linux_dent.d_name[..name_len].copy_from_slice(&name_bytes[..name_len]);
        linux_dent.d_name[name_len] = 0;

        debug!("Copying dirent to user: {:?}", linux_dent);

        linux_dent.d_off = offset + 1;
        let err = copy_to_user(user_buf as *mut LinuxDirent64, &[linux_dent]);
        if err.is_err() {
            error!("Failed to copy to user buffer: {:?}", err);
            return -EFAULT as u64;
        }

        offset += 1;
        bytes_written += reclen;
        user_buf = unsafe { user_buf.add(reclen) };
    }

    info!(
        "getdents finished: bytes_written={}, new_offset={}",
        bytes_written, offset
    );

    *fd_guard.cursor.lock() = offset;
    bytes_written as u64
}
