//! Virtual Filesystem layer — not yet implemented.
//!
//! The plan is to introduce a VFS that can multiplex multiple filesystem
//! backends (e.g., FAT via `fatfs`, future native filesystems) under a common
//! path namespace and permission model. Until then, consumers use the `FS`
//! global from `disk::mod` directly.

