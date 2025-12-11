//! Filesystem integration layer.
//!
//! This module contains the filesystem adapters used by the kernel. At the
//! moment we rely on the external `fatfs` crate for FAT12/16/32 support and
//! provide only thin glue at the disk root (`FS`, `File`, `FileWrapper`).
//!
//! Future work will move toward native filesystem implementations in
//! `disk::fs` (see `fs::fat`).

mod fat;
