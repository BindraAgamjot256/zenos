//! FAT filesystem implementation (FAT12/FAT16/FAT32).
//!
//! This module provides a native FAT filesystem implementation that integrates
//! with the VFS traits defined in `disk::vfs`.
//!
//! # Usage Example
//!
//! ```rust,ignore
//! use crate::disk::fs::fat::FatFileSystem;
//! use crate::disk::block::ahci::AhciBlockDevice;
//! use crate::disk::vfs::{FileSystem, Directory, File, SeekFrom};
//! use alloc::boxed::Box;
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
//! // Open a file
//! let mut file = root.open_file("README.TXT").expect("File not found");
//!
//! // Read file contents
//! let mut buf = [0u8; 256];
//! let bytes_read = file.read(&mut buf).expect("Read failed");
//!
//! // Create a new file
//! let mut new_file = root.create_file("NEWFILE.TXT").expect("Create failed");
//! new_file.write(b"Hello, FAT!").expect("Write failed");
//! new_file.flush().expect("Flush failed");
//! ```

mod plumbing;

use crate::disk::block::BlockDevice;
use crate::disk::vfs::{self, DirEntry, Metadata, SeekFrom};
use crate::disk::FileError;
use alloc::boxed::Box;
use alloc::string::String;
use alloc::sync::Arc;
use alloc::vec;
use alloc::vec::Vec;
use plumbing::{
    name_to_8_3, read_cluster, write_cluster, BiosParameterBlock, FatDirEntry, FatTable, FatType,
};
use spin::Mutex;

/// Shared filesystem state wrapped in Arc<Mutex<...>>.
struct FatFileSystemInner<D: BlockDevice> {
    device: D,
    bpb: BiosParameterBlock,
    fat_type: FatType,
}

/// FAT filesystem instance.
pub struct FatFileSystem<D: BlockDevice + 'static> {
    inner: Arc<Mutex<FatFileSystemInner<D>>>,
}

impl<D: BlockDevice + 'static> FatFileSystem<D> {
    /// Mount a FAT filesystem from the given block device.
    pub fn mount(mut device: D) -> Result<Self, FileError> {
        // Read boot sector
        let mut boot_sector = [0u8; 512];
        device
            .seek(SeekFrom::Start(0))
            .map_err(|_| FileError::SeekError)?;
        device
            .read(&mut boot_sector)
            .map_err(|_| FileError::ReadError)?;

        // Parse BPB
        let bpb = BiosParameterBlock::parse(&boot_sector)?;
        let fat_type = bpb.fat_type();

        Ok(Self {
            inner: Arc::new(Mutex::new(FatFileSystemInner {
                device,
                bpb,
                fat_type,
            })),
        })
    }

    /// Get the FAT type (FAT12, FAT16, or FAT32).
    pub fn fat_type(&self) -> FatType {
        self.inner.lock().fat_type
    }
}

impl<D: BlockDevice + 'static> vfs::FileSystem for FatFileSystem<D> {
    fn root_dir(&self) -> Result<Box<dyn vfs::Directory>, FileError> {
        let inner = self.inner.lock();
        let root_cluster = if inner.fat_type == FatType::Fat32 {
            inner.bpb.root_cluster
        } else {
            0 // FAT12/16 use fixed root directory
        };
        drop(inner);

        Ok(Box::new(FatDirectory::<D> {
            inner: Arc::clone(&self.inner),
            cluster: root_cluster,
            is_root: true,
        }))
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
        let mut inner = self.inner.lock();
        let mut entries = Vec::new();
        let bpb = inner.bpb.clone();
        let fat_type = inner.fat_type;

        if self.is_root && fat_type != FatType::Fat32 {
            // FAT12/16 root directory is at a fixed location
            let root_dir_sector =
                bpb.reserved_sector_count as u32 + (bpb.num_fats as u32 * bpb.fat_size());
            let root_dir_size = bpb.root_entry_count as usize * FatDirEntry::SIZE;

            let mut buf = vec![0u8; root_dir_size];
            let offset = root_dir_sector as u64 * bpb.bytes_per_sector as u64;

            inner
                .device
                .seek(SeekFrom::Start(offset))
                .map_err(|_| FileError::SeekError)?;
            inner
                .device
                .read(&mut buf)
                .map_err(|_| FileError::ReadError)?;

            for (i, chunk) in buf.chunks(FatDirEntry::SIZE).enumerate() {
                if chunk[0] == 0x00 {
                    break;
                }
                if let Some(entry) = FatDirEntry::parse(chunk) {
                    if !entry.is_long_name() && !entry.is_volume_id() {
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
                read_cluster(&mut inner.device, &bpb, cluster, &mut buf)?;

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
                let mut fat = FatTable::new(&mut inner.device, &bpb);
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
        let entries = self.read_entries()?;
        let target = name.to_uppercase();

        for (entry, idx) in entries {
            if entry.short_name().to_uppercase() == target {
                return Ok(Some((entry, idx)));
            }
        }

        Ok(None)
    }

    fn create_entry(&mut self, name: &str, is_dir: bool) -> Result<FatDirEntry, FileError> {
        let mut inner = self.inner.lock();
        let bpb = inner.bpb.clone();
        let fat_type = inner.fat_type;

        // Allocate a cluster for the new file/directory
        let new_cluster = if is_dir {
            let mut fat = FatTable::new(&mut inner.device, &bpb);
            let cluster = fat.allocate_cluster()?;
            // Zero out the new directory cluster
            let cluster_size = bpb.bytes_per_cluster() as usize;
            let zeros = vec![0u8; cluster_size];
            write_cluster(&mut inner.device, &bpb, cluster, &zeros)?;
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
            let offset = root_dir_sector as u64 * bpb.bytes_per_sector as u64;

            inner
                .device
                .seek(SeekFrom::Start(offset))
                .map_err(|_| FileError::SeekError)?;
            inner
                .device
                .read(&mut buf)
                .map_err(|_| FileError::ReadError)?;

            // Find free slot
            for (_i, chunk) in buf.chunks_mut(FatDirEntry::SIZE).enumerate() {
                if chunk[0] == 0x00 || chunk[0] == 0xE5 {
                    chunk.copy_from_slice(&entry_data);
                    inner
                        .device
                        .seek(SeekFrom::Start(offset))
                        .map_err(|_| FileError::SeekError)?;
                    inner
                        .device
                        .write(&buf)
                        .map_err(|_| FileError::WriteError)?;
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
                read_cluster(&mut inner.device, &bpb, cluster, &mut buf)?;

                for chunk in buf.chunks_mut(FatDirEntry::SIZE) {
                    if chunk[0] == 0x00 || chunk[0] == 0xE5 {
                        chunk.copy_from_slice(&entry_data);
                        write_cluster(&mut inner.device, &bpb, cluster, &buf)?;
                        return Ok(entry);
                    }
                }

                // Follow or extend cluster chain
                let mut fat = FatTable::new(&mut inner.device, &bpb);
                let next = fat.read_entry(cluster)?;
                if fat.is_eoc(next) {
                    // Allocate new cluster for directory
                    let new_dir_cluster = fat.allocate_cluster()?;
                    fat.write_entry(cluster, new_dir_cluster)?;

                    // Zero out new cluster and write entry
                    let mut new_buf = vec![0u8; cluster_size];
                    new_buf[0..FatDirEntry::SIZE].copy_from_slice(&entry_data);
                    write_cluster(&mut inner.device, &bpb, new_dir_cluster, &new_buf)?;
                    return Ok(entry);
                }
                cluster = next;
            }
        }
    }
}

impl<D: BlockDevice + 'static> vfs::Directory for FatDirectory<D> {
    fn open_file(&mut self, name: &str) -> Result<Box<dyn vfs::File>, FileError> {
        let (entry, entry_idx) = self.find_entry(name)?.ok_or(FileError::NotFound)?;

        if entry.is_directory() {
            return Err(FileError::InvalidDescriptor);
        }

        Ok(Box::new(FatFile::<D> {
            inner: Arc::clone(&self.inner),
            entry,
            cursor: 0,
            dir_cluster: self.cluster,
            entry_index: entry_idx,
            in_root: self.is_root,
        }))
    }

    fn create_file(&mut self, name: &str) -> Result<Box<dyn vfs::File>, FileError> {
        // Check if already exists
        if self.find_entry(name)?.is_some() {
            return Err(FileError::AlreadyExists);
        }

        let entry = self.create_entry(name, false)?;
        // Find the entry index we just created
        let (_, entry_idx) = self.find_entry(name)?.ok_or(FileError::NotFound)?;

        Ok(Box::new(FatFile::<D> {
            inner: Arc::clone(&self.inner),
            entry,
            cursor: 0,
            dir_cluster: self.cluster,
            entry_index: entry_idx,
            in_root: self.is_root,
        }))
    }

    fn open_dir(&mut self, name: &str) -> Result<Box<dyn vfs::Directory>, FileError> {
        let (entry, _) = self.find_entry(name)?.ok_or(FileError::NotFound)?;

        if !entry.is_directory() {
            return Err(FileError::InvalidDescriptor);
        }

        Ok(Box::new(FatDirectory::<D> {
            inner: Arc::clone(&self.inner),
            cluster: entry.first_cluster(),
            is_root: false,
        }))
    }

    fn create_dir(&mut self, name: &str) -> Result<Box<dyn vfs::Directory>, FileError> {
        // Check if already exists
        if self.find_entry(name)?.is_some() {
            return Err(FileError::AlreadyExists);
        }

        let entry = self.create_entry(name, true)?;

        Ok(Box::new(FatDirectory::<D> {
            inner: Arc::clone(&self.inner),
            cluster: entry.first_cluster(),
            is_root: false,
        }))
    }

    fn remove(&mut self, name: &str) -> Result<(), FileError> {
        let (entry, entry_idx) = self.find_entry(name)?.ok_or(FileError::NotFound)?;

        // If directory, check if empty
        if entry.is_directory() {
            let mut subdir = FatDirectory::<D> {
                inner: Arc::clone(&self.inner),
                cluster: entry.first_cluster(),
                is_root: false,
            };
            let sub_entries = subdir.read_entries()?;
            // Filter out . and .. entries
            let real_entries: Vec<_> = sub_entries
                .iter()
                .filter(|(e, _)| {
                    let n = e.short_name();
                    n != "." && n != ".."
                })
                .collect();
            if !real_entries.is_empty() {
                return Err(FileError::DirectoryNotEmpty);
            }
        }

        // Free cluster chain
        if entry.first_cluster() >= 2 {
            let mut inner = self.inner.lock();
            let bpb = inner.bpb.clone();
            let mut fat = FatTable::new(&mut inner.device, &bpb);
            fat.free_chain(entry.first_cluster())?;
        }

        // Mark directory entry as deleted
        let mut inner = self.inner.lock();
        let bpb = inner.bpb.clone();
        let fat_type = inner.fat_type;

        if self.is_root && fat_type != FatType::Fat32 {
            let root_dir_sector =
                bpb.reserved_sector_count as u32 + (bpb.num_fats as u32 * bpb.fat_size());
            let offset = root_dir_sector as u64 * bpb.bytes_per_sector as u64
                + (entry_idx * FatDirEntry::SIZE) as u64;

            inner
                .device
                .seek(SeekFrom::Start(offset))
                .map_err(|_| FileError::SeekError)?;
            inner
                .device
                .write(&[0xE5])
                .map_err(|_| FileError::WriteError)?;
        } else {
            let cluster_size = bpb.bytes_per_cluster() as usize;
            let entries_per_cluster = cluster_size / FatDirEntry::SIZE;
            let target_cluster_idx = entry_idx / entries_per_cluster;
            let offset_in_cluster = (entry_idx % entries_per_cluster) * FatDirEntry::SIZE;

            // Navigate to correct cluster
            let mut cluster = self.cluster;
            for _ in 0..target_cluster_idx {
                let mut fat = FatTable::new(&mut inner.device, &bpb);
                cluster = fat.read_entry(cluster)?;
            }

            let mut buf = vec![0u8; cluster_size];
            read_cluster(&mut inner.device, &bpb, cluster, &mut buf)?;
            buf[offset_in_cluster] = 0xE5;
            write_cluster(&mut inner.device, &bpb, cluster, &buf)?;
        }

        Ok(())
    }

    fn read_dir(&mut self) -> Result<Vec<DirEntry>, FileError> {
        let entries = self.read_entries()?;
        let mut result = Vec::new();

        for (entry, _) in entries {
            let name = entry.short_name();
            if name == "." || name == ".." {
                continue;
            }

            result.push(DirEntry {
                name,
                metadata: Metadata {
                    size: entry.file_size as u64,
                    is_dir: entry.is_directory(),
                    is_file: !entry.is_directory(),
                    created: 0,
                    modified: 0,
                    accessed: 0,
                },
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
        let mut inner = self.inner.lock();
        let bpb = inner.bpb.clone();
        let fat_type = inner.fat_type;

        let entry_data = self.entry.serialize();

        if self.in_root && fat_type != FatType::Fat32 {
            // FAT12/16 root directory
            let root_dir_sector =
                bpb.reserved_sector_count as u32 + (bpb.num_fats as u32 * bpb.fat_size());
            let offset = root_dir_sector as u64 * bpb.bytes_per_sector as u64
                + (self.entry_index * FatDirEntry::SIZE) as u64;

            inner
                .device
                .seek(SeekFrom::Start(offset))
                .map_err(|_| FileError::SeekError)?;
            inner
                .device
                .write(&entry_data)
                .map_err(|_| FileError::WriteError)?;
        } else {
            // Cluster-based directory
            let cluster_size = bpb.bytes_per_cluster() as usize;
            let entries_per_cluster = cluster_size / FatDirEntry::SIZE;
            let target_cluster_idx = self.entry_index / entries_per_cluster;
            let offset_in_cluster = (self.entry_index % entries_per_cluster) * FatDirEntry::SIZE;

            // Navigate to correct cluster
            let mut cluster = self.dir_cluster;
            for _ in 0..target_cluster_idx {
                let mut fat = FatTable::new(&mut inner.device, &bpb);
                cluster = fat.read_entry(cluster)?;
            }

            let mut buf = vec![0u8; cluster_size];
            read_cluster(&mut inner.device, &bpb, cluster, &mut buf)?;
            buf[offset_in_cluster..offset_in_cluster + FatDirEntry::SIZE]
                .copy_from_slice(&entry_data);
            write_cluster(&mut inner.device, &bpb, cluster, &buf)?;
        }

        Ok(())
    }
}
impl<D: BlockDevice + 'static> vfs::File for FatFile<D> {
    fn read(&mut self, buf: &mut [u8]) -> Result<usize, FileError> {
        if self.cursor >= self.entry.file_size as u64 {
            return Ok(0);
        }

        let mut inner = self.inner.lock();
        let bpb = inner.bpb.clone();

        let cluster_size = bpb.bytes_per_cluster() as u64;
        let mut bytes_read = 0;
        let mut cluster = self.entry.first_cluster();

        // Skip to the cluster containing the cursor
        let cluster_idx = self.cursor / cluster_size;
        for _ in 0..cluster_idx {
            if cluster < 2 {
                return Ok(0);
            }
            let mut fat = FatTable::new(&mut inner.device, &bpb);
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
                break;
            }

            read_cluster(&mut inner.device, &bpb, cluster, &mut cluster_buf)?;

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
            let mut fat = FatTable::new(&mut inner.device, &bpb);
            let next = fat.read_entry(cluster)?;
            if fat.is_eoc(next) {
                break;
            }
            cluster = next;
        }
        drop(inner);

        self.cursor += bytes_read as u64;
        Ok(bytes_read)
    }

    fn write(&mut self, buf: &[u8]) -> Result<usize, FileError> {
        if buf.is_empty() {
            return Ok(0);
        }

        let mut inner = self.inner.lock();
        let bpb = inner.bpb.clone();

        let cluster_size = bpb.bytes_per_cluster() as u64;
        let mut bytes_written = 0;

        // Allocate first cluster if needed
        if self.entry.first_cluster() < 2 {
            let mut fat = FatTable::new(&mut inner.device, &bpb);
            let new_cluster = fat.allocate_cluster()?;
            self.entry.set_first_cluster(new_cluster);
        }

        let mut cluster = self.entry.first_cluster();

        // Skip to the cluster containing the cursor
        let cluster_idx = self.cursor / cluster_size;
        for _ in 0..cluster_idx {
            let mut fat = FatTable::new(&mut inner.device, &bpb);
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
                read_cluster(&mut inner.device, &bpb, cluster, &mut cluster_buf)?;
            }

            let remaining_in_cluster = cluster_size as usize - offset_in_cluster;
            let remaining_in_buf = buf.len() - bytes_written;
            let to_copy = remaining_in_cluster.min(remaining_in_buf);

            cluster_buf[offset_in_cluster..offset_in_cluster + to_copy]
                .copy_from_slice(&buf[bytes_written..bytes_written + to_copy]);

            write_cluster(&mut inner.device, &bpb, cluster, &cluster_buf)?;

            bytes_written += to_copy;
            offset_in_cluster = 0;

            if bytes_written < buf.len() {
                // Need more clusters
                let mut fat = FatTable::new(&mut inner.device, &bpb);
                let next = fat.read_entry(cluster)?;
                if fat.is_eoc(next) {
                    let new_cluster = fat.allocate_cluster()?;
                    fat.write_entry(cluster, new_cluster)?;
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

        Ok(bytes_written)
    }

    fn seek(&mut self, pos: SeekFrom) -> Result<u64, FileError> {
        self.cursor = match pos {
            SeekFrom::Start(offset) => offset,
            SeekFrom::End(offset) => {
                if offset >= 0 {
                    self.entry.file_size as u64 + offset as u64
                } else {
                    (self.entry.file_size as i64 + offset)
                        .try_into()
                        .map_err(|_| FileError::SeekError)?
                }
            }
            SeekFrom::Current(offset) => {
                if offset >= 0 {
                    self.cursor + offset as u64
                } else {
                    self.cursor
                        .checked_sub(offset.unsigned_abs())
                        .ok_or(FileError::SeekError)?
                }
            }
        };
        Ok(self.cursor)
    }

    fn flush(&mut self) -> Result<(), FileError> {
        // Sync directory entry to disk
        self.sync_entry()?;
        let mut inner = self.inner.lock();
        inner.device.flush().map_err(|_| FileError::WriteError)
    }

    fn metadata(&self) -> Result<Metadata, FileError> {
        Ok(Metadata {
            size: self.entry.file_size as u64,
            is_dir: false,
            is_file: true,
            created: 0,
            modified: 0,
            accessed: 0,
        })
    }
}
