use crate::fs::File;
use crate::kprint;
use crate::syscall::write::FileDescriptor::{Stderr, Stdout};
use fatfs::Write;

#[repr(u64)]
pub enum FileDescriptor<'a> {
    Stdout,
    Stderr,
    File(File<'a>),
}

impl TryFrom<u64> for FileDescriptor<'_> {
    type Error = ();
    fn try_from(value: u64) -> Result<Self, Self::Error> {
        let val = match value {
            1 => Stdout,
            2 => Stderr,
            _ => return Err(()),
        };
        Ok(val)
    }
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
        FileDescriptor::File(mut file) => {
            let bool = file.write(buf).is_ok();
            if bool { Some(buf.len() as u64) } else { None }
        }
    }
}
