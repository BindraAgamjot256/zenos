use crate::kprint;
use crate::syscall::table::SyscallPtr;
use crate::syscall::{copy_from_user, write};
use log::debug;

#[repr(u64)]
pub enum FileDescriptor {
    Stdout,
    Stderr,
}

impl TryFrom<u64> for FileDescriptor {
    type Error = ();
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        let val = match value {
            1 => Self::Stdout,
            2 => Self::Stderr,
            _ => return Err(()),
        };
        Ok(val)
    }
}

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
    let fd = FileDescriptor::try_from(fd);
    if fd.is_err() {
        ret = u64::MAX;
    } else {
        let val = sys_write(&mut buf, fd.unwrap());
        if val.is_some() {
            ret = val.unwrap();
        } else {
            ret = u64::MAX;
        }
    }
    ret
}

pub(crate) fn sys_write(buf: &[u8], fd: FileDescriptor) -> Option<u64> {
    match fd {
        FileDescriptor::Stdout => {
            kprint!("{}", core::str::from_utf8(buf).unwrap());
            Some(buf.len() as u64)
        }
        FileDescriptor::Stderr => {
            kprint!("ERR, {}", core::str::from_utf8(buf).unwrap());
            Some(buf.len() as u64)
        }
    }
}
