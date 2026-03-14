//! Filesystem implementations.
//!
//! This module contains concrete filesystem implementations used by the kernel.
//! All filesystems implement the VFS traits defined in [`crate::disk::vfs`].
//!
//! # Available Filesystems
//!
//! - [`fat`]: Native FAT12/FAT16/FAT32 implementation for persistent storage
//!   on block devices. Supports full read/write operations including file
//!   creation, deletion, and directory management.
//!
//! - [`proc`]: Virtual procfs exposing kernel and process information.
//!   Mounted at `/proc`, provides Linux-compatible files like `cpuinfo`,
//!   `meminfo`, `uptime`, and per-process directories.

pub mod ext2;
pub mod fat;
pub mod proc;
