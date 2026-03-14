use crate::disk::FS;
use crate::syscall::errors::{EFAULT, file_error_to_errno};
use crate::syscall::table::SyscallPtr;
use crate::syscall::{copy_string, copy_to_user};
use zenos_macros::syscall;

#[repr(C)]
#[derive(Debug, Clone, Copy)]
pub struct Stat {
    pub st_dev: u64,     // dev_t
    pub st_ino: u64,     // ino_t
    pub st_mode: u32,    // mode_t
    pub st_nlink: u64,   // nlink_t
    pub st_uid: u32,     // uid_t
    pub st_gid: u32,     // gid_t
    pub st_rdev: u64,    // dev_t
    pub st_size: i64,    // off_t
    pub st_blksize: i64, // blksize_t
    pub st_blocks: i64,  // blkcnt_t
    pub st_atime: i64,   // time_t
    pub st_mtime: i64,   // time_t
    pub st_ctime: i64,   // time_t
}

const MAX_PATH_LEN: usize = 4096;

#[syscall(4)]
fn stat(rdi: u64, rsi: u64, _rdx: u64, _r10: u64, _r8: u64, _r9: u64) -> u64 {
    let path_ptr = rdi as *const u8;
    let stat_ptr = rsi as *mut Stat;

    let path = copy_string(path_ptr, MAX_PATH_LEN);
    if path.is_err() {
        return path.err().unwrap();
    }

    let fs = FS
        .lock()
        .open_file(path.unwrap().as_str())
        .map_err(|e| file_error_to_errno(&e));
    if fs.is_err() {
        return fs.err().unwrap();
    }

    let file = fs.unwrap();
    let stat = file.lock().data.stat().map_err(|e| file_error_to_errno(&e));
    if stat.is_err() {
        return stat.err().unwrap();
    }

    let stat = stat.unwrap();

    let ustat = Stat {
        st_dev: stat.st_dev,
        st_ino: stat.st_ino,
        st_mode: stat.st_mode,
        st_nlink: stat.st_nlink,
        st_uid: stat.st_uid,
        st_gid: stat.st_gid,
        st_rdev: stat.st_rdev,
        st_size: stat.st_size,
        st_blksize: stat.st_blksize,
        st_blocks: stat.st_blocks,
        st_atime: stat.st_atime,
        st_mtime: stat.st_mtime,
        st_ctime: stat.st_ctime,
    };

    let res = copy_to_user(stat_ptr, &[ustat]);
    if res.is_err() {
        return -EFAULT as u64;
    }

    0
}
