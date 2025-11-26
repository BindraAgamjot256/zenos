use crate::kprintln;
use alloc::boxed::Box;
use fatfs::{IoBase, Read, Seek, SeekFrom, Write};

struct Stdout;
struct Stderr;
struct Stdin;

impl IoBase for Stdout {
    type Error = ();
}

impl FileLike for Stdout {
    fn read(&mut self, _buffer: &mut [u8]) -> Result<usize, Self::Error> {
        Err(())
    }

    fn write(&mut self, buffer: &[u8]) -> Result<usize, Self::Error> {
        unsafe { kprintln!("{}", core::str::from_utf8_unchecked(buffer)) };
        Ok(buffer.len())
    }

    fn seek(&mut self, _position: u64) -> Result<u64, Self::Error> {
        Err(())
    }
}

impl IoBase for Stderr {
    type Error = ();
}

impl FileLike for Stderr {
    fn read(&mut self, _buffer: &mut [u8]) -> Result<usize, Self::Error> {
        Err(())
    }

    fn write(&mut self, buffer: &[u8]) -> Result<usize, Self::Error> {
        unsafe { kprintln!("{}", core::str::from_utf8_unchecked(buffer)) };
        Ok(buffer.len())
    }

    fn seek(&mut self, _position: u64) -> Result<u64, Self::Error> {
        Err(())
    }
}

impl IoBase for Stdin {
    type Error = ();
}

impl FileLike for Stdin {
    fn read(&mut self, _buffer: &mut [u8]) -> Result<usize, Self::Error> {
        todo!("Stdin read not implemented")
    }

    fn write(&mut self, _buffer: &[u8]) -> Result<usize, Self::Error> {
        Err(())
    }

    fn seek(&mut self, _position: u64) -> Result<u64, Self::Error> {
        Err(())
    }
}

struct FileHandle {
    id: u32,
    descriptor: Box<dyn FileLike<Error = ()>>,
}

trait FileLike: IoBase {
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, <Self as IoBase>::Error>;
    fn write(&mut self, buffer: &[u8]) -> Result<usize, <Self as IoBase>::Error>;
    fn seek(&mut self, position: u64) -> Result<u64, <Self as IoBase>::Error>;
}

impl<T> FileLike for T
where
    T: Read + Write + Seek + IoBase,
{
    fn read(&mut self, buffer: &mut [u8]) -> Result<usize, <Self as IoBase>::Error> {
        Read::read(self, buffer)
    }

    fn write(&mut self, buffer: &[u8]) -> Result<usize, <Self as IoBase>::Error> {
        Write::write(self, buffer)
    }

    fn seek(&mut self, position: u64) -> Result<u64, <Self as IoBase>::Error> {
        Seek::seek(self, SeekFrom::Start(position))
    }
}
