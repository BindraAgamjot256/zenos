use crate::kprint;

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
