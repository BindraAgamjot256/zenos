use anyhow::Context;
use core::fmt::{Debug, Formatter};

use std::io::Cursor;
use std::path::PathBuf;
use std::{fs, io};

#[derive(Clone)]
/// Defines a data source, either a source `std::path::PathBuf`, or a vector of bytes.
pub enum FileDataSource {
    File(PathBuf),
    Bytes(&'static [u8]),
}

impl Debug for FileDataSource {
    fn fmt(&self, f: &mut Formatter<'_>) -> core::fmt::Result {
        match self {
            FileDataSource::File(file) => {
                f.write_fmt(format_args!("data source: File {}", file.display()))
            }
            FileDataSource::Bytes(b) => {
                f.write_fmt(format_args!("data source: {} raw bytes ", b.len()))
            }
        }
    }
}

impl FileDataSource {
    /// Get the length of the inner data source
    pub fn len(&self) -> anyhow::Result<u64> {
        Ok(match self {
            FileDataSource::File(path) => fs::metadata(path)
                .with_context(|| format!("failed to read metadata of file `{}`", path.display()))?
                .len(),
            FileDataSource::Bytes(s) => s.len() as u64,
        })
    }
    /// Copy this data source to the specified target that implements io::Write
    pub fn copy_to(&self, target: &mut dyn io::Write) -> anyhow::Result<()> {
        match self {
            FileDataSource::File(file_path) => {
                io::copy(
                    &mut fs::File::open(file_path).with_context(|| {
                        format!("failed to open `{}` for copying", file_path.display())
                    })?,
                    target,
                )?;
            }
            FileDataSource::Bytes(contents) => {
                let mut cursor = Cursor::new(contents);
                io::copy(&mut cursor, target)?;
            }
        };

        Ok(())
    }

    /// Write this data source to a file at the specified path
    pub fn write_to_path(&self, path: &std::path::Path) -> anyhow::Result<()> {
        match self {
            FileDataSource::File(file_path) => {
                fs::copy(file_path, path).with_context(|| {
                    format!(
                        "failed to copy `{}` to `{}`",
                        file_path.display(),
                        path.display()
                    )
                })?;
            }
            FileDataSource::Bytes(contents) => {
                fs::write(path, contents)
                    .with_context(|| format!("failed to write bytes to `{}`", path.display()))?;
            }
        };
        Ok(())
    }
}
