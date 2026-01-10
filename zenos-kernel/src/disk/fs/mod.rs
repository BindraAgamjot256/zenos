//! Filesystem integration layer.
//!
//! This module contains the filesystem implementations used by the kernel.
//! The primary filesystem is FAT (FAT12/16/32) implemented natively.
//!
//! All filesystems implement the VFS traits defined in [`crate::disk::vfs`].

pub mod fat;
pub mod proc;
