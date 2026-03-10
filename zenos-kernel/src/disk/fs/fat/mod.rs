//! FAT filesystem implementation (FAT12/FAT16/FAT32).
//!
//! This module provides a native FAT filesystem implementation that integrates
//! with the VFS traits defined in [`vfs`].
//!
//! # Overview
//!
//! The FAT (File Allocation Table) filesystem is a simple, widely-supported
//! filesystem used on removable media and EFI system partitions. This
//! implementation supports all three FAT variants:
//!
//! - **FAT12**: For small volumes (< 4085 clusters), rarely used today
//! - **FAT16**: For medium volumes (< 65525 clusters)
//! - **FAT32**: For large volumes, supports long filenames (not implemented)
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────┐
//! │                      Boot Sector (BPB)                      │
//! ├─────────────────────────────────────────────────────────────┤
//! │                    FAT Table(s)                             │
//! │              (cluster allocation chain)                     │
//! ├─────────────────────────────────────────────────────────────┤
//! │              Root Directory (FAT12/16 only)                 │
//! ├─────────────────────────────────────────────────────────────┤
//! │                       Data Region                           │
//! │              (files and directories in clusters)            │
//! └─────────────────────────────────────────────────────────────┘
//! ```
//!
//! # Components
//!
//! - [`FatFileSystem`]: Main filesystem handle, created via `mount()`. Provides
//!   access to the root directory and manages the underlying block device.
//!
//! - [`FatDirectory`]: Directory handle implementing [`Directory`](InodeOps).
//!   Supports listing, creating, and removing files and subdirectories.
//!
//! - [`FatFile`]: File handle implementing [`File`](InodeOps).
//!   Supports read, write, seek, and flush with automatic cluster allocation.
//!
//! # Limitations
//!
//! - Only 8.3 short filenames are supported (no VFAT long filenames)
//! - Timestamps are not fully implemented
//! - No filesystem-level caching (relies on block device)
//!
//! # Usage Example
//!
//! ```rust,ignore
//! use crate::disk::fs::fat::FatFileSystem;
//! use crate::disk::block::ahci::AhciBlockDevice;
//! use crate::disk::vfs::{FileSystem, Directory, File, SeekFrom};
//!
//! // Create a block device
//! let device = AhciBlockDevice::new(0).expect("No disk found");
//!
//! // Mount the FAT filesystem
//! let fs = FatFileSystem::mount(device).expect("Failed to mount");
//!
//! // Get root directory
//! let mut root = fs.root_dir().expect("Failed to get root");
//!
//! // Open and read a file
//! let mut file = root.open_file("README.TXT").expect("File not found");
//! let mut buf = [0u8; 256];
//! let bytes_read = file.read(&mut buf).expect("Read failed");
//!
//! // Create a new file
//! let mut new_file = root.create_file("NEWFILE.TXT").expect("Create failed");
//! new_file.write(b"Hello, FAT!").expect("Write failed");
//! new_file.flush().expect("Flush failed");
//! ```

//todo: note: rewrite the fatfs to be more standards compliant, support LFN, and add more features.
// this is just a basic implementation to get something working, made with ❤️ by github copilot.
// see the FatFs crate for a more complete reference implementation.

mod plumbing;

use crate::disk::block::BlockDevice;
use crate::disk::vfs::{self, DirEntry, FileType, Inode, InodeOps, Permissions, SeekFrom};
use crate::disk::{FileError, FsMountError};
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use alloc::sync::{Arc, Weak};
use alloc::vec;
use alloc::vec::Vec;
use core::sync::atomic::AtomicU64;
use hashbrown::HashMap;
use log::{debug, error, trace, warn};
use plumbing::{
    BiosParameterBlock, FatDirEntry, FatTable, FatType, name_to_8_3, read_cluster, write_cluster,
};
use spin::Mutex;

#[derive(Debug, Hash, Copy, Clone, Eq, PartialEq)]
struct InodeKey {
    dir_cluster: u32,
    entry_index: usize,
    in_root: bool,
}

/// Shared filesystem state wrapped in Arc<Mutex<...>>.
struct FatFileSystemInner<D: BlockDevice> {
    device: D,
    bpb: BiosParameterBlock,
    fat_type: FatType,
    partition_offset: u64,
    ino_cache: HashMap<InodeKey, Weak<Mutex<Inode>>>,
}

/// FAT filesystem instance.
pub struct FatFileSystem<D: BlockDevice + 'static> {
    inner: Arc<Mutex<FatFileSystemInner<D>>>,
}

impl<D: BlockDevice + 'static> FatFileSystem<D> {
    /// Mount a FAT filesystem from the given block device.
    pub fn mount(mut device: D, partition_offset: u64) -> Result<Self, FsMountError> {
        debug!("FatFileSystem: mounting filesystem");
        // Read boot sector
        let mut boot_sector = [0u8; 512];
        device
            .seek(SeekFrom::Start(0 + partition_offset))
            .map_err(|e| {
                error!("FatFileSystem: failed to seek to boot sector: {:?}", e);
                FsMountError::ReadError
            })?;
        device.read(&mut boot_sector).map_err(|e| {
            error!("FatFileSystem: failed to read boot sector: {:?}", e);
            FsMountError::ReadError
        })?;

        // Parse BPB
        let bpb = BiosParameterBlock::parse(&boot_sector).map_err(|e| {
            error!("FatFileSystem: failed to parse BPB: {:?}", e);
            FsMountError::ReadError
        })?;
        bpb.validate().map_err(|e| {
            error!("FatFileSystem: invalid BPB: {:?}", e);
            e
        })?;
        let fat_type = bpb.fat_type();
        debug!("FatFileSystem: detected {:?} filesystem", fat_type);

        Ok(Self {
            inner: Arc::new(Mutex::new(FatFileSystemInner {
                device,
                bpb,
                fat_type,
                partition_offset,
                ino_cache: HashMap::new(),
            })),
        })
    }

    /// Get the FAT type (FAT12, FAT16, or FAT32).
    pub fn fat_type(&self) -> FatType {
        self.inner.lock().fat_type
    }
}

impl<D: BlockDevice + 'static> vfs::FileSystem for FatFileSystem<D> {
    fn root_dir(&self) -> Result<Arc<Mutex<Inode>>, FileError> {
        trace!("FatFileSystem: getting root directory");
        let inner = self.inner.lock();
        let root_cluster = if inner.fat_type == FatType::Fat32 {
            inner.bpb.root_cluster
        } else {
            0 // FAT12/16 use fixed root directory
        };
        drop(inner);

        let data = Box::new(FatDirectory::<D> {
            inner: Arc::clone(&self.inner),
            cluster: root_cluster,
            is_root: true,
        });
        Ok(Arc::new(Mutex::new(Inode {
            num: -1i64 as u64, // FAT doesn't have inodes, so we can use a dummy value
            kind: FileType::Directory,
            size: AtomicU64::new(0), // Size is not meaningful for directories in FAT
            perms: Permissions::all(),
            links: AtomicU64::new(1),
            data,
        })))
    }
}

/// FAT directory handle.
pub struct FatDirectory<D: BlockDevice + 'static> {
    inner: Arc<Mutex<FatFileSystemInner<D>>>,
    cluster: u32,
    is_root: bool,
}

unsafe impl<D: BlockDevice + 'static> Send for FatDirectory<D> {}
unsafe impl<D: BlockDevice + 'static> Sync for FatDirectory<D> {}

impl<D: BlockDevice + 'static> FatDirectory<D> {
    fn read_entries(&mut self) -> Result<Vec<(FatDirEntry, usize)>, FileError> {
        trace!(
            "FatDirectory: reading directory entries from cluster {}",
            self.cluster
        );
        let mut inner = self.inner.lock();
        let mut entries = Vec::new();
        let bpb = inner.bpb.clone();
        let fat_type = inner.fat_type;

        if self.is_root && fat_type != FatType::Fat32 {
            // FAT12/16 root directory is at a fixed location
            let root_dir_sector =
                bpb.reserved_sector_count as u32 + (bpb.num_fats as u32 * bpb.fat_size());
            let root_dir_size = bpb.root_entry_count as usize * FatDirEntry::SIZE;
            trace!(
                "FatDirectory: reading FAT12/16 root directory at sector {}, size:{}",
                root_dir_sector, root_dir_size
            );

            let mut buf = vec![0u8; root_dir_size];
            let offset =
                (root_dir_sector as u64 * bpb.bytes_per_sector as u64) + inner.partition_offset;

            inner.device.seek(SeekFrom::Start(offset)).map_err(|e| {
                error!("FatDirectory: failed to seek to root dir: {:?}", e);
                FileError::SeekError
            })?;
            inner.device.read(&mut buf).map_err(|e| {
                error!("FatDirectory: failed to read root dir: {:?}", e);
                FileError::ReadError
            })?;

            for (i, chunk) in buf.chunks(FatDirEntry::SIZE).enumerate() {
                if chunk[0] == 0x00 {
                    break;
                }
                if let Some(entry) = FatDirEntry::parse(chunk) {
                    if !entry.is_long_name() && !entry.is_volume_id() {
                        debug!("FatDirectory: found entry '{}'", entry.short_name());
                        entries.push((entry, i));
                    }
                }
            }
        } else {
            // Cluster-based directory
            let cluster_size = bpb.bytes_per_cluster() as usize;
            let mut buf = vec![0u8; cluster_size];
            let mut cluster = self.cluster;
            let mut global_idx = 0;

            loop {
                let po = inner.partition_offset;
                read_cluster(&mut inner.device, &bpb, cluster, &mut buf, po)?;

                for chunk in buf.chunks(FatDirEntry::SIZE) {
                    if chunk[0] == 0x00 {
                        return Ok(entries);
                    }
                    if let Some(entry) = FatDirEntry::parse(chunk) {
                        if !entry.is_long_name() && !entry.is_volume_id() {
                            entries.push((entry, global_idx));
                        }
                    }
                    global_idx += 1;
                }

                // Follow cluster chain
                let po = inner.partition_offset;
                let mut fat = FatTable::new(&mut inner.device, &bpb, po);
                let next = fat.read_entry(cluster)?;
                if fat.is_eoc(next) {
                    break;
                }
                cluster = next;
            }
        }

        Ok(entries)
    }

    fn find_entry(&mut self, name: &str) -> Result<Option<(FatDirEntry, usize)>, FileError> {
        trace!("FatDirectory: finding entry '{}'", name);
        let entries = self.read_entries()?;
        let target = name.to_uppercase();

        for (entry, idx) in entries {
            if entry.short_name().to_uppercase() == target {
                debug!("found entry '{}' at index {}", name, idx);
                return Ok(Some((entry, idx)));
            }
        }

        Ok(None)
    }

    fn create_entry(&mut self, name: &str, is_dir: bool) -> Result<FatDirEntry, FileError> {
        debug!(
            "FatDirectory: creating entry '{}' (is_dir={})",
            name, is_dir
        );
        let mut inner = self.inner.lock();
        let bpb = inner.bpb.clone();
        let fat_type = inner.fat_type;

        // Allocate a cluster for the new file/directory
        let new_cluster = if is_dir {
            let po = inner.partition_offset;
            let mut fat = FatTable::new(&mut inner.device, &bpb, po);
            let cluster = fat.allocate_cluster()?;
            // Zero out the new directory cluster
            let cluster_size = bpb.bytes_per_cluster() as usize;
            let zeros = vec![0u8; cluster_size];
            write_cluster(&mut inner.device, &bpb, cluster, &zeros, po)?;
            cluster
        } else {
            0 // Files start with no clusters until data is written
        };

        // Create directory entry
        let mut entry = FatDirEntry {
            name: name_to_8_3(name),
            attr: if is_dir {
                FatDirEntry::ATTR_DIRECTORY
            } else {
                FatDirEntry::ATTR_ARCHIVE
            },
            nt_reserved: 0,
            create_time_tenth: 0,
            create_time: 0,
            create_date: 0,
            last_access_date: 0,
            first_cluster_high: 0,
            first_cluster_low: 0,
            write_time: 0,
            write_date: 0,
            file_size: 0,
        };
        entry.set_first_cluster(new_cluster);

        // Find free slot in directory
        let entry_data = entry.serialize();

        if self.is_root && fat_type != FatType::Fat32 {
            // FAT12/16 root directory
            let root_dir_sector =
                bpb.reserved_sector_count as u32 + (bpb.num_fats as u32 * bpb.fat_size());
            let root_dir_size = bpb.root_entry_count as usize * FatDirEntry::SIZE;

            let mut buf = vec![0u8; root_dir_size];
            let offset =
                root_dir_sector as u64 * bpb.bytes_per_sector as u64 + inner.partition_offset;

            inner.device.seek(SeekFrom::Start(offset)).map_err(|e| {
                error!(
                    "FatDirectory: failed to seek for create_entry read: {:?}",
                    e
                );
                FileError::SeekError
            })?;
            inner.device.read(&mut buf).map_err(|e| {
                error!("FatDirectory: failed to read for create_entry: {:?}", e);
                FileError::ReadError
            })?;

            // Find free slot
            for (_i, chunk) in buf.chunks_mut(FatDirEntry::SIZE).enumerate() {
                if chunk[0] == 0x00 || chunk[0] == 0xE5 {
                    chunk.copy_from_slice(&entry_data);
                    inner.device.seek(SeekFrom::Start(offset)).map_err(|e| {
                        error!(
                            "FatDirectory: failed to seek for create_entry write: {:?}",
                            e
                        );
                        FileError::SeekError
                    })?;
                    inner.device.write(&buf).map_err(|e| {
                        error!("FatDirectory: failed to write for create_entry: {:?}", e);
                        FileError::WriteError
                    })?;
                    return Ok(entry);
                }
            }

            Err(FileError::Other(String::from("Root directory full")))
        } else {
            // Cluster-based directory
            let cluster_size = bpb.bytes_per_cluster() as usize;
            let mut buf = vec![0u8; cluster_size];
            let mut cluster = self.cluster;

            loop {
                let po = inner.partition_offset;
                read_cluster(&mut inner.device, &bpb, cluster, &mut buf, po).map_err(|e| {
                    error!(
                        "FatDirectory: failed to read cluster {} for create_entry: {:?}",
                        cluster, e
                    );
                    e
                })?;

                let po = inner.partition_offset;
                for chunk in buf.chunks_mut(FatDirEntry::SIZE) {
                    if chunk[0] == 0x00 || chunk[0] == 0xE5 {
                        chunk.copy_from_slice(&entry_data);
                        write_cluster(&mut inner.device, &bpb, cluster, &buf, po).map_err(|e| {
                            error!(
                                "FatDirectory: failed to write cluster {} for create_entry: {:?}",
                                cluster, e
                            );
                            e
                        })?;
                        return Ok(entry);
                    }
                }

                // Follow or extend cluster chain
                let mut fat = FatTable::new(&mut inner.device, &bpb, po);
                let next = fat.read_entry(cluster)?;
                if fat.is_eoc(next) {
                    // Allocate new cluster for directory
                    let new_dir_cluster = fat.allocate_cluster().map_err(|e| {
                        error!("FatDirectory: failed to allocate cluster for directory extension: {:?}", e);
                        e
                    })?;
                    fat.write_entry(cluster, new_dir_cluster)?;

                    // Zero out new cluster and write entry
                    let mut new_buf = vec![0u8; cluster_size];
                    new_buf[0..FatDirEntry::SIZE].copy_from_slice(&entry_data);
                    write_cluster(&mut inner.device, &bpb, new_dir_cluster, &new_buf, po)?;
                    debug!(
                        "FatDirectory: extended directory with new cluster {}",
                        new_dir_cluster
                    );
                    return Ok(entry);
                }
                cluster = next;
            }
        }
    }

    fn create_and_cache_inode(
        &self,
        inner: &mut FatFileSystemInner<D>, // done on purpose... makes locking easier. see uses.
        key: InodeKey,
        entry: FatDirEntry,
        idx: usize,
    ) -> Result<Arc<Mutex<Inode>>, FileError> {
        let inode_ops: Box<dyn InodeOps + Send + Sync> = if entry.is_directory() {
            Box::new(FatDirectory {
                inner: Arc::clone(&self.inner),
                cluster: entry.first_cluster(),
                is_root: false,
            })
        } else {
            Box::new(FatFile {
                inner: Arc::clone(&self.inner),
                entry: entry.clone(),
                cursor: 0,
                dir_cluster: self.cluster,
                entry_index: idx,
                in_root: self.is_root,
            })
        };

        let perms = if entry.attr & FatDirEntry::ATTR_READ_ONLY != 0 {
            Permissions::all()
                & !(Permissions::OWNER_WRITE | Permissions::GROUP_WRITE | Permissions::OTHER_WRITE)
        } else {
            Permissions::all()
        };

        let inode = Arc::new(Mutex::new(Inode {
            num: ((self.cluster as u64) << 32) | idx as u64,
            kind: if entry.is_directory() {
                FileType::Directory
            } else {
                FileType::File
            },
            size: AtomicU64::new(entry.file_size as u64),
            perms,
            links: AtomicU64::new(1),
            data: inode_ops,
        }));

        inner.ino_cache.insert(key, Arc::downgrade(&inode));
        Ok(inode)
    }
}

impl<D: BlockDevice + 'static> InodeOps for FatDirectory<D> {
    fn read(&mut self, _offset: u64, _buf: &mut [u8]) -> Result<usize, FileError> {
        Err(FileError::IsADirectory)
    }

    fn write(&mut self, _offset: u64, _buf: &[u8]) -> Result<usize, FileError> {
        Err(FileError::IsADirectory)
    }

    fn unlink(&mut self, name: &str) -> Result<(), FileError> {
        trace!("FatDirectory: unlinking '{}'", name);

        // Find the entry to unlink
        let (entry, entry_index) = self.find_entry(name)?.ok_or(FileError::NotFound)?;

        // Don't allow unlinking directories (use rmdir for that)
        if entry.is_directory() {
            return Err(FileError::IsADirectory);
        }

        let mut inner = self.inner.lock();
        let bpb = inner.bpb.clone();
        let fat_type = inner.fat_type;
        let po = inner.partition_offset;

        // Free cluster chain if file has clusters
        if entry.first_cluster() >= 2 {
            let mut fat = FatTable::new(&mut inner.device, &bpb, po);
            let mut cluster = entry.first_cluster();

            loop {
                let next = fat.read_entry(cluster)?;
                fat.write_entry(cluster, 0)?; // Mark cluster as free

                if fat.is_eoc(next) {
                    break;
                }
                cluster = next;
            }
            debug!(
                "FatDirectory: freed cluster chain starting at {}",
                entry.first_cluster()
            );
        }

        // Mark directory entry as deleted
        if self.is_root && fat_type != FatType::Fat32 {
            // FAT12/16 root directory
            let root_dir_sector =
                bpb.reserved_sector_count as u32 + (bpb.num_fats as u32 * bpb.fat_size());
            let offset = root_dir_sector as u64 * bpb.bytes_per_sector as u64
                + (entry_index * FatDirEntry::SIZE) as u64
                + inner.partition_offset;

            let mut entry_data = [0u8; FatDirEntry::SIZE];
            inner.device.seek(SeekFrom::Start(offset)).map_err(|e| {
                error!("FatDirectory: failed to seek for unlink: {:?}", e);
                FileError::SeekError
            })?;
            inner.device.read(&mut entry_data).map_err(|e| {
                error!("FatDirectory: failed to read for unlink: {:?}", e);
                FileError::ReadError
            })?;

            entry_data[0] = 0xE5; // Mark as deleted

            inner.device.seek(SeekFrom::Start(offset)).map_err(|e| {
                error!("FatDirectory: failed to seek for unlink write: {:?}", e);
                FileError::SeekError
            })?;
            inner.device.write(&entry_data).map_err(|e| {
                error!("FatDirectory: failed to write for unlink: {:?}", e);
                FileError::WriteError
            })?;
        } else {
            // Cluster-based directory
            let cluster_size = bpb.bytes_per_cluster() as usize;
            let entries_per_cluster = cluster_size / FatDirEntry::SIZE;
            let target_cluster_idx = entry_index / entries_per_cluster;
            let offset_in_cluster = (entry_index % entries_per_cluster) * FatDirEntry::SIZE;

            // Navigate to correct cluster
            let mut cluster = self.cluster;
            for _ in 0..target_cluster_idx {
                let mut fat = FatTable::new(&mut inner.device, &bpb, po);
                cluster = fat.read_entry(cluster)?;
            }

            let mut buf = vec![0u8; cluster_size];
            read_cluster(&mut inner.device, &bpb, cluster, &mut buf, po).map_err(|e| {
                error!("FatDirectory: failed to read cluster for unlink: {:?}", e);
                e
            })?;

            buf[offset_in_cluster] = 0xE5; // Mark as deleted

            write_cluster(&mut inner.device, &bpb, cluster, &buf, po).map_err(|e| {
                error!("FatDirectory: failed to write cluster for unlink: {:?}", e);
                e
            })?;
        }

        // Remove from inode cache if present
        let key = InodeKey {
            dir_cluster: self.cluster,
            entry_index,
            in_root: self.is_root,
        };
        inner.ino_cache.remove(&key);

        debug!("FatDirectory: unlink '{}' complete", name);
        Ok(())
    }

    fn truncate(&mut self, _size: u64) -> Result<(), FileError> {
        Err(FileError::IsADirectory)
    }

    fn sync(&mut self) -> Result<(), FileError> {
        Ok(())
    }
    fn lookup(&mut self, name: &str) -> Result<Arc<Mutex<Inode>>, FileError> {
        let (entry, idx) = self.find_entry(name)?.ok_or(FileError::NotFound)?;

        let key = InodeKey {
            dir_cluster: self.cluster,
            entry_index: idx,
            in_root: self.is_root,
        };

        let mut inner = self.inner.lock();

        if let Some(weak) = inner.ino_cache.get(&key) {
            if let Some(existing) = weak.upgrade() {
                debug!(
                    "FatDirectory: cache hit for entry '{}' at index {}",
                    name, idx
                );
                return Ok(existing);
            } else {
                // Clean up dead Weak
                inner.ino_cache.remove(&key);
            }
        }

        let inode = self.create_and_cache_inode(&mut inner, key, entry, idx)?;
        Ok(inode)
    }

    fn create(
        &mut self,
        name: &str,
        kind: FileType,
        _perms: Permissions,
    ) -> Result<Arc<Mutex<Inode>>, FileError> {
        let is_dir = matches!(kind, FileType::Directory);
        self.create_entry(name, is_dir)?;
        Ok(self.lookup(name)?)
    }

    fn read_dir(&mut self) -> Result<Vec<DirEntry>, FileError> {
        let entries = self.read_entries()?;
        let mut result = Vec::new();

        for (entry, idx) in entries {
            let key = InodeKey {
                dir_cluster: self.cluster,
                entry_index: idx,
                in_root: self.is_root,
            };

            // First try cache
            let inode = {
                let mut inner = self.inner.lock();
                if let Some(weak) = inner.ino_cache.get(&key) {
                    if let Some(existing) = weak.upgrade() {
                        existing
                    } else {
                        // Weak expired, remove it
                        inner.ino_cache.remove(&key);
                        Self::create_and_cache_inode(self, &mut inner, key, entry.clone(), idx)?
                    }
                } else {
                    Self::create_and_cache_inode(self, &mut inner, key, entry.clone(), idx)?
                }
            };

            result.push(DirEntry {
                name: entry.short_name().to_string(),
                inode,
            });
        }

        Ok(result)
    }
}

/// FAT file handle.
pub struct FatFile<D: BlockDevice + 'static> {
    inner: Arc<Mutex<FatFileSystemInner<D>>>,
    entry: FatDirEntry,
    cursor: u64,
    /// Directory cluster where the file's entry lives (0 for FAT12/16 root)
    dir_cluster: u32,
    /// Index of the entry within the directory
    entry_index: usize,
    /// Whether this is in the FAT12/16 root directory
    in_root: bool,
}

unsafe impl<D: BlockDevice + 'static> Send for FatFile<D> {}
unsafe impl<D: BlockDevice + 'static> Sync for FatFile<D> {}

impl<D: BlockDevice + 'static> FatFile<D> {
    /// Write the current entry back to the directory on disk.
    fn sync_entry(&mut self) -> Result<(), FileError> {
        trace!("FatFile: syncing entry to disk");
        let mut inner = self.inner.lock();
        let bpb = inner.bpb.clone();
        let fat_type = inner.fat_type;

        let entry_data = self.entry.serialize();

        if self.in_root && fat_type != FatType::Fat32 {
            // FAT12/16 root directory
            let root_dir_sector =
                bpb.reserved_sector_count as u32 + (bpb.num_fats as u32 * bpb.fat_size());
            let offset = root_dir_sector as u64 * bpb.bytes_per_sector as u64
                + (self.entry_index * FatDirEntry::SIZE) as u64
                + inner.partition_offset;

            inner.device.seek(SeekFrom::Start(offset)).map_err(|e| {
                error!("FatFile: failed to seek for sync_entry: {:?}", e);
                FileError::SeekError
            })?;
            inner.device.write(&entry_data).map_err(|e| {
                error!("FatFile: failed to write for sync_entry: {:?}", e);
                FileError::WriteError
            })?;
        } else {
            // Cluster-based directory
            let cluster_size = bpb.bytes_per_cluster() as usize;
            let entries_per_cluster = cluster_size / FatDirEntry::SIZE;
            let target_cluster_idx = self.entry_index / entries_per_cluster;
            let offset_in_cluster = (self.entry_index % entries_per_cluster) * FatDirEntry::SIZE;

            // Navigate to correct cluster
            let mut cluster = self.dir_cluster;
            let po = inner.partition_offset;
            for _ in 0..target_cluster_idx {
                let mut fat = FatTable::new(&mut inner.device, &bpb, po);
                cluster = fat.read_entry(cluster)?;
            }

            let mut buf = vec![0u8; cluster_size];
            read_cluster(&mut inner.device, &bpb, cluster, &mut buf, po).map_err(|e| {
                error!("FatFile: failed to read cluster for sync_entry: {:?}", e);
                e
            })?;
            buf[offset_in_cluster..offset_in_cluster + FatDirEntry::SIZE]
                .copy_from_slice(&entry_data);
            write_cluster(&mut inner.device, &bpb, cluster, &buf, po).map_err(|e| {
                error!("FatFile: failed to write cluster for sync_entry: {:?}", e);
                e
            })?;
        }

        trace!("FatFile: sync_entry complete");
        Ok(())
    }
}

impl<D: BlockDevice + 'static> InodeOps for FatFile<D> {
    fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, FileError> {
        self.cursor = offset;
        trace!(
            "FatFile: read {} bytes at cursor {}",
            buf.len(),
            self.cursor
        );
        if self.cursor >= self.entry.file_size as u64 {
            trace!("FatFile: cursor at EOF");
            return Ok(0);
        }

        let mut inner = self.inner.lock();
        let bpb = inner.bpb.clone();

        let cluster_size = bpb.bytes_per_cluster() as u64;
        let mut bytes_read = 0;
        let mut cluster = self.entry.first_cluster();

        // Skip to the cluster containing the cursor
        let po = inner.partition_offset;
        let cluster_idx = self.cursor / cluster_size;
        for _ in 0..cluster_idx {
            if cluster < 2 {
                return Ok(0);
            }
            let mut fat = FatTable::new(&mut inner.device, &bpb, po);
            let next = fat.read_entry(cluster)?;
            if fat.is_eoc(next) {
                return Ok(0);
            }
            cluster = next;
        }

        let mut offset_in_cluster = (self.cursor % cluster_size) as usize;
        let mut cluster_buf = vec![0u8; cluster_size as usize];

        while bytes_read < buf.len()
            && ((self.cursor + bytes_read as u64) < self.entry.file_size as u64)
        {
            if cluster < 2 {
                warn!("FatFile: unexpected end of cluster chain during read");
                break;
            }

            read_cluster(&mut inner.device, &bpb, cluster, &mut cluster_buf, po).map_err(|e| {
                error!(
                    "FatFile: failed to read cluster {} during file read: {:?}",
                    cluster, e
                );
                e
            })?;

            let remaining_in_cluster = cluster_size as usize - offset_in_cluster;
            let remaining_in_file =
                (self.entry.file_size as u64 - self.cursor - bytes_read as u64) as usize;
            let remaining_in_buf = buf.len() - bytes_read;
            let to_copy = remaining_in_cluster
                .min(remaining_in_file)
                .min(remaining_in_buf);

            buf[bytes_read..bytes_read + to_copy]
                .copy_from_slice(&cluster_buf[offset_in_cluster..offset_in_cluster + to_copy]);

            bytes_read += to_copy;
            offset_in_cluster = 0;

            // Move to next cluster
            let mut fat = FatTable::new(&mut inner.device, &bpb, po);
            let next = fat.read_entry(cluster)?;
            if fat.is_eoc(next) {
                break;
            }
            cluster = next;
        }
        drop(inner);

        self.cursor += bytes_read as u64;
        trace!("FatFile: read complete, {} bytes read", bytes_read);
        Ok(bytes_read)
    }

    fn write(&mut self, offset: u64, buf: &[u8]) -> Result<usize, FileError> {
        self.cursor = offset;
        trace!(
            "FatFile: write {} bytes at cursor {}",
            buf.len(),
            self.cursor
        );
        if buf.is_empty() {
            return Ok(0);
        }

        let mut inner = self.inner.lock();
        let bpb = inner.bpb.clone();

        let cluster_size = bpb.bytes_per_cluster() as u64;
        let mut bytes_written = 0;
        let po = inner.partition_offset;

        // Allocate first cluster if needed
        if self.entry.first_cluster() < 2 {
            let mut fat = FatTable::new(&mut inner.device, &bpb, po);
            let new_cluster = fat.allocate_cluster().map_err(|e| {
                error!("FatFile: failed to allocate first cluster: {:?}", e);
                e
            })?;
            self.entry.set_first_cluster(new_cluster);
            debug!("FatFile: allocated first cluster {}", new_cluster);
        }

        let mut cluster = self.entry.first_cluster();

        // Skip to the cluster containing the cursor
        let cluster_idx = self.cursor / cluster_size;
        for _ in 0..cluster_idx {
            let mut fat = FatTable::new(&mut inner.device, &bpb, po);
            let next = fat.read_entry(cluster)?;
            if fat.is_eoc(next) {
                // Allocate new cluster
                let new_cluster = fat.allocate_cluster()?;
                fat.write_entry(cluster, new_cluster)?;
                cluster = new_cluster;
            } else {
                cluster = next;
            }
        }

        let mut offset_in_cluster = (self.cursor % cluster_size) as usize;
        let mut cluster_buf = vec![0u8; cluster_size as usize];

        while bytes_written < buf.len() {
            // Read existing cluster data for partial writes
            if offset_in_cluster != 0 || buf.len() - bytes_written < cluster_size as usize {
                read_cluster(&mut inner.device, &bpb, cluster, &mut cluster_buf, po).map_err(
                    |e| {
                        error!(
                            "FatFile: failed to read cluster {} for partial write: {:?}",
                            cluster, e
                        );
                        e
                    },
                )?;
            }

            let remaining_in_cluster = cluster_size as usize - offset_in_cluster;
            let remaining_in_buf = buf.len() - bytes_written;
            let to_copy = remaining_in_cluster.min(remaining_in_buf);

            cluster_buf[offset_in_cluster..offset_in_cluster + to_copy]
                .copy_from_slice(&buf[bytes_written..bytes_written + to_copy]);

            write_cluster(&mut inner.device, &bpb, cluster, &cluster_buf, po).map_err(|e| {
                error!("FatFile: failed to write cluster {}: {:?}", cluster, e);
                e
            })?;

            bytes_written += to_copy;
            offset_in_cluster = 0;

            if bytes_written < buf.len() {
                // Need more clusters
                let mut fat = FatTable::new(&mut inner.device, &bpb, po);
                let next = fat.read_entry(cluster)?;
                if fat.is_eoc(next) {
                    let new_cluster = fat.allocate_cluster().map_err(|e| {
                        error!(
                            "FatFile: failed to allocate cluster for write extension: {:?}",
                            e
                        );
                        e
                    })?;
                    fat.write_entry(cluster, new_cluster)?;
                    trace!("FatFile: extended file with cluster {}", new_cluster);
                    cluster = new_cluster;
                } else {
                    cluster = next;
                }
            }
        }
        drop(inner);

        self.cursor += bytes_written as u64;

        // Update file size if we wrote past the end
        if self.cursor > self.entry.file_size as u64 {
            self.entry.file_size = self.cursor as u32;
            // Sync the directory entry to disk
            self.sync_entry()?;
        }

        trace!("FatFile: write complete, {} bytes written", bytes_written);
        Ok(bytes_written)
    }

    fn unlink(&mut self, _name: &str) -> Result<(), FileError> {
        Err(FileError::NotADirectory)
    }

    fn truncate(&mut self, size: u64) -> Result<(), FileError> {
        let mut buf = vec![0u8; size as usize];
        self.read(0, &mut buf)?;
        self.cursor = 0;
        self.write(0, &buf)?;
        self.entry.file_size = size as u32;
        self.sync_entry()?;
        Ok(())
    }

    fn sync(&mut self) -> Result<(), FileError> {
        self.sync_entry()
    }

    fn lookup(&mut self, _name: &str) -> Result<Arc<Mutex<Inode>>, FileError> {
        error!("FatFile: lookup called on file, {:#?}", self.entry);
        Err(FileError::NotADirectory)
    }

    fn create(
        &mut self,
        _name: &str,
        _kind: FileType,
        _perms: Permissions,
    ) -> Result<Arc<Mutex<Inode>>, FileError> {
        Err(FileError::NotADirectory)
    }

    fn read_dir(&mut self) -> Result<Vec<DirEntry>, FileError> {
        Err(FileError::NotADirectory)
    }
}
