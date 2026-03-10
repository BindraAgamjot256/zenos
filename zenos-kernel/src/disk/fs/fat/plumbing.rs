//! FAT filesystem internals — low-level structures and operations.
//!
//! This module contains the internal implementation details for FAT12/16/32
//! filesystems. External code should use the public API in [`super`].
//!
//! # Contents
//!
//! - [`BiosParameterBlock`]: Boot sector parsing and filesystem geometry
//! - [`FatDirEntry`]: 32-byte directory entry structure (8.3 filenames)
//! - [`FatTable`]: FAT cluster chain operations (read, write, allocate, free)
//! - [`FatType`]: Filesystem variant detection (FAT12/16/32)
//!
//! # Cluster Chains
//!
//! Files and directories are stored as linked lists of clusters. The FAT
//! (File Allocation Table) maps each cluster to the next cluster in the chain,
//! or to a special end-of-chain marker.
//!
//! ```text
//! FAT Entry Values:
//! - 0x00000000: Free cluster
//! - 0x00000002 - 0x0FFFFFEF: Next cluster in chain
//! - 0x0FFFFFF8 - 0x0FFFFFFF: End of chain (EOC)
//! ```

use crate::disk::block::BlockDevice;
use crate::disk::vfs::SeekFrom;
use crate::disk::{FileError, FsMountError};
use alloc::string::{String, ToString};
use alloc::vec;
use log::{error, info, trace};

/// FAT filesystem type variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FatType {
    Fat12,
    Fat16,
    Fat32,
}

/// Special FAT entry values.
pub const FAT_FREE: u32 = 0x00000000;

/// BIOS Parameter Block (common fields).
#[derive(Debug, Clone)]
pub struct BiosParameterBlock {
    pub bytes_per_sector: u16,
    pub sectors_per_cluster: u8,
    pub reserved_sector_count: u16,
    pub num_fats: u8,
    pub root_entry_count: u16,
    pub total_sectors_16: u16,
    pub _media_type: u8,
    pub fat_size_16: u16,
    pub _sectors_per_track: u16,
    pub _num_heads: u16,
    pub _hidden_sectors: u32,
    pub total_sectors_32: u32,
    // FAT32 extended fields
    pub fat_size_32: u32,
    pub root_cluster: u32,
}

impl BiosParameterBlock {
    /// Parse BPB from the first 512 bytes of the volume.
    pub fn parse(boot_sector: &[u8]) -> Result<Self, FileError> {
        // todo: check if this is a valid fat boot sector, reading bytes 510/511 either here or
        //  in BiosParameterBlock::validate(); currently we just assume the caller has already verified this.
        if boot_sector.len() < 512 {
            error!(
                "BiosParameterBlock: boot sector too small ({} bytes)",
                boot_sector.len()
            );
            return Err(FileError::ReadError);
        }

        let bytes_per_sector = u16::from_le_bytes([boot_sector[11], boot_sector[12]]);
        let sectors_per_cluster = boot_sector[13];
        let reserved_sector_count = u16::from_le_bytes([boot_sector[14], boot_sector[15]]);
        let num_fats = boot_sector[16];
        let root_entry_count = u16::from_le_bytes([boot_sector[17], boot_sector[18]]);
        let total_sectors_16 = u16::from_le_bytes([boot_sector[19], boot_sector[20]]);
        let media_type = boot_sector[21];
        let fat_size_16 = u16::from_le_bytes([boot_sector[22], boot_sector[23]]);
        let sectors_per_track = u16::from_le_bytes([boot_sector[24], boot_sector[25]]);
        let num_heads = u16::from_le_bytes([boot_sector[26], boot_sector[27]]);
        let hidden_sectors = u32::from_le_bytes([
            boot_sector[28],
            boot_sector[29],
            boot_sector[30],
            boot_sector[31],
        ]);
        let total_sectors_32 = u32::from_le_bytes([
            boot_sector[32],
            boot_sector[33],
            boot_sector[34],
            boot_sector[35],
        ]);

        // FAT32 extended fields
        let fat_size_32 = u32::from_le_bytes([
            boot_sector[36],
            boot_sector[37],
            boot_sector[38],
            boot_sector[39],
        ]);
        let root_cluster = u32::from_le_bytes([
            boot_sector[44],
            boot_sector[45],
            boot_sector[46],
            boot_sector[47],
        ]);

        trace!(
            "BiosParameterBlock: parsed BPB (bytes_per_sector={}, sectors_per_cluster={})",
            bytes_per_sector, sectors_per_cluster
        );

        info!(
            "BPB: bytes_per_sector={}, sectors_per_cluster={}, reserved_sector_count={}, num_fats={}, root_entry_count={}, total_sectors_16={}, media_type={:#x}, fat_size_16={}, sectors_per_track={}, num_heads={}, hidden_sectors={}, total_sectors_32={}, fat_size_32={}, root_cluster={}",
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sector_count,
            num_fats,
            root_entry_count,
            total_sectors_16,
            media_type,
            fat_size_16,
            sectors_per_track,
            num_heads,
            hidden_sectors,
            total_sectors_32,
            fat_size_32,
            root_cluster
        );

        Ok(Self {
            bytes_per_sector,
            sectors_per_cluster,
            reserved_sector_count,
            num_fats,
            root_entry_count,
            total_sectors_16,
            _media_type: media_type,
            fat_size_16,
            _sectors_per_track: sectors_per_track,
            _num_heads: num_heads,
            _hidden_sectors: hidden_sectors,
            total_sectors_32,
            fat_size_32,
            root_cluster,
        })
    }
    pub fn validate(&self) -> Result<(), FsMountError> {
        // Bytes per sector must be valid
        match self.bytes_per_sector {
            512 | 1024 | 2048 | 4096 => {}
            _ => return Err(FsMountError::InvalidSuperblock),
        }

        // Sectors per cluster must be power of two (1–128)
        if self.sectors_per_cluster == 0
            || self.sectors_per_cluster > 128
            || !self.sectors_per_cluster.is_power_of_two()
        {
            return Err(FsMountError::InvalidSuperblock);
        }

        // Reserved sector count must exist
        if self.reserved_sector_count == 0 {
            return Err(FsMountError::InvalidSuperblock);
        }

        // Must have at least one FAT
        if self.num_fats == 0 {
            return Err(FsMountError::InvalidSuperblock);
        }

        // Total sectors must exist
        if self.total_sectors() == 0 {
            return Err(FsMountError::InvalidSuperblock);
        }

        // FAT size must exist
        if self.fat_size() == 0 {
            return Err(FsMountError::InvalidSuperblock);
        }

        let fat_type = self.fat_type();

        match fat_type {
            FatType::Fat12 | FatType::Fat16 => {
                if self.root_entry_count == 0 {
                    return Err(FsMountError::Corrupted);
                }
            }

            FatType::Fat32 => {
                if self.root_entry_count != 0 {
                    return Err(FsMountError::Corrupted);
                }

                if self.root_cluster < 2 {
                    return Err(FsMountError::Corrupted);
                }

                if self.fat_size_32 == 0 {
                    return Err(FsMountError::Corrupted);
                }
            }
        }

        // Layout sanity check
        let required_sectors = self.reserved_sector_count as u32
            + (self.num_fats as u32 * self.fat_size())
            + self.root_dir_sectors();

        if required_sectors >= self.total_sectors() {
            return Err(FsMountError::Corrupted);
        }

        Ok(())
    }

    /// Determine the FAT type based on cluster count.
    pub fn fat_type(&self) -> FatType {
        let root_dir_sectors = ((self.root_entry_count as u32 * 32)
            + (self.bytes_per_sector as u32 - 1))
            / self.bytes_per_sector as u32;

        let fat_size = if self.fat_size_16 != 0 {
            self.fat_size_16 as u32
        } else {
            self.fat_size_32
        };

        let total_sectors = if self.total_sectors_16 != 0 {
            self.total_sectors_16 as u32
        } else {
            self.total_sectors_32
        };

        let data_sectors = total_sectors
            - (self.reserved_sector_count as u32
                + (self.num_fats as u32 * fat_size)
                + root_dir_sectors);

        let cluster_count = data_sectors / self.sectors_per_cluster as u32;

        if cluster_count < 4085 {
            FatType::Fat12
        } else if cluster_count < 65525 {
            FatType::Fat16
        } else {
            FatType::Fat32
        }
    }

    pub fn fat_size(&self) -> u32 {
        if self.fat_size_16 != 0 {
            self.fat_size_16 as u32
        } else {
            self.fat_size_32
        }
    }

    pub fn total_sectors(&self) -> u32 {
        if self.total_sectors_16 != 0 {
            self.total_sectors_16 as u32
        } else {
            self.total_sectors_32
        }
    }

    pub fn root_dir_sectors(&self) -> u32 {
        ((self.root_entry_count as u32 * 32) + (self.bytes_per_sector as u32 - 1))
            / self.bytes_per_sector as u32
    }

    pub fn first_data_sector(&self) -> u32 {
        self.reserved_sector_count as u32
            + (self.num_fats as u32 * self.fat_size())
            + self.root_dir_sectors()
    }

    pub fn first_fat_sector(&self) -> u32 {
        self.reserved_sector_count as u32
    }

    pub fn cluster_to_sector(&self, cluster: u32) -> u32 {
        ((cluster - 2) * self.sectors_per_cluster as u32) + self.first_data_sector()
    }

    pub fn bytes_per_cluster(&self) -> u32 {
        self.bytes_per_sector as u32 * self.sectors_per_cluster as u32
    }
}

/// A FAT directory entry (32 bytes).
#[derive(Debug, Clone)]
pub struct FatDirEntry {
    pub name: [u8; 11],
    pub attr: u8,
    pub nt_reserved: u8,
    pub create_time_tenth: u8,
    pub create_time: u16,
    pub create_date: u16,
    pub last_access_date: u16,
    pub first_cluster_high: u16,
    pub write_time: u16,
    pub write_date: u16,
    pub first_cluster_low: u16,
    pub file_size: u32,
}

impl FatDirEntry {
    pub const SIZE: usize = 32;
    pub const ATTR_READ_ONLY: u8 = 0x01;
    pub const _ATTR_HIDDEN: u8 = 0x02;
    pub const _ATTR_SYSTEM: u8 = 0x04;
    pub const ATTR_VOLUME_ID: u8 = 0x08;
    pub const ATTR_DIRECTORY: u8 = 0x10;
    pub const ATTR_ARCHIVE: u8 = 0x20;
    pub const ATTR_LONG_NAME: u8 = 0x0F;

    pub fn parse(data: &[u8]) -> Option<Self> {
        if data.len() < Self::SIZE {
            return None;
        }

        // First byte 0x00 means end of directory
        if data[0] == 0x00 {
            return None;
        }

        // First byte 0xE5 means deleted entry
        if data[0] == 0xE5 {
            return None;
        }

        let mut name = [0u8; 11];
        name.copy_from_slice(&data[0..11]);

        Some(Self {
            name,
            attr: data[11],
            nt_reserved: data[12],
            create_time_tenth: data[13],
            create_time: u16::from_le_bytes([data[14], data[15]]),
            create_date: u16::from_le_bytes([data[16], data[17]]),
            last_access_date: u16::from_le_bytes([data[18], data[19]]),
            first_cluster_high: u16::from_le_bytes([data[20], data[21]]),
            write_time: u16::from_le_bytes([data[22], data[23]]),
            write_date: u16::from_le_bytes([data[24], data[25]]),
            first_cluster_low: u16::from_le_bytes([data[26], data[27]]),
            file_size: u32::from_le_bytes([data[28], data[29], data[30], data[31]]),
        })
    }

    pub fn serialize(&self) -> [u8; 32] {
        let mut data = [0u8; 32];
        data[0..11].copy_from_slice(&self.name);
        data[11] = self.attr;
        data[12] = self.nt_reserved;
        data[13] = self.create_time_tenth;
        data[14..16].copy_from_slice(&self.create_time.to_le_bytes());
        data[16..18].copy_from_slice(&self.create_date.to_le_bytes());
        data[18..20].copy_from_slice(&self.last_access_date.to_le_bytes());
        data[20..22].copy_from_slice(&self.first_cluster_high.to_le_bytes());
        data[22..24].copy_from_slice(&self.write_time.to_le_bytes());
        data[24..26].copy_from_slice(&self.write_date.to_le_bytes());
        data[26..28].copy_from_slice(&self.first_cluster_low.to_le_bytes());
        data[28..32].copy_from_slice(&self.file_size.to_le_bytes());
        data
    }

    pub fn first_cluster(&self) -> u32 {
        ((self.first_cluster_high as u32) << 16) | (self.first_cluster_low as u32)
    }

    pub fn set_first_cluster(&mut self, cluster: u32) {
        self.first_cluster_low = (cluster & 0xFFFF) as u16;
        self.first_cluster_high = ((cluster >> 16) & 0xFFFF) as u16;
    }

    pub fn is_directory(&self) -> bool {
        self.attr & Self::ATTR_DIRECTORY != 0
    }

    pub fn is_volume_id(&self) -> bool {
        self.attr & Self::ATTR_VOLUME_ID != 0
    }

    pub fn is_long_name(&self) -> bool {
        (self.attr & Self::ATTR_LONG_NAME) == Self::ATTR_LONG_NAME
    }

    /// Convert 8.3 name to a string, trimming spaces.
    pub fn short_name(&self) -> String {
        let name_part: String = self.name[0..8]
            .iter()
            .map(|&b| b as char)
            .collect::<String>()
            .trim_end()
            .to_string();

        let ext_part: String = self.name[8..11]
            .iter()
            .map(|&b| b as char)
            .collect::<String>()
            .trim_end()
            .to_string();

        if ext_part.is_empty() {
            name_part
        } else {
            alloc::format!("{}.{}", name_part, ext_part)
        }
    }
}

/// Convert a filename to 8.3 format.
pub fn name_to_8_3(name: &str) -> [u8; 11] {
    let mut result = [0x20u8; 11]; // Space-padded
    let upper = name.to_uppercase();

    let (base, ext) = if let Some(dot_pos) = upper.rfind('.') {
        (&upper[..dot_pos], &upper[dot_pos + 1..])
    } else {
        (upper.as_str(), "")
    };

    for (i, c) in base.chars().take(8).enumerate() {
        result[i] = c as u8;
    }

    for (i, c) in ext.chars().take(3).enumerate() {
        result[8 + i] = c as u8;
    }

    result
}

/// Low-level FAT table operations.
pub struct FatTable<'a, D: BlockDevice> {
    device: &'a mut D,
    bpb: &'a BiosParameterBlock,
    fat_type: FatType,
    partition_offset: u64,
}

impl<'a, D: BlockDevice> FatTable<'a, D> {
    pub fn new(device: &'a mut D, bpb: &'a BiosParameterBlock, partition_offset: u64) -> Self {
        Self {
            device,
            bpb,
            fat_type: bpb.fat_type(),
            partition_offset,
        }
    }

    /// Read a FAT entry for the given cluster.
    pub fn read_entry(&mut self, cluster: u32) -> Result<u32, FileError> {
        trace!("FatTable: reading entry for cluster {}", cluster);
        let fat_offset = match self.fat_type {
            FatType::Fat12 => cluster + (cluster / 2),
            FatType::Fat16 => cluster * 2,
            FatType::Fat32 => cluster * 4,
        };

        let fat_sector =
            self.bpb.first_fat_sector() + (fat_offset / self.bpb.bytes_per_sector as u32);
        let offset_in_sector = (fat_offset % self.bpb.bytes_per_sector as u32) as usize;

        let mut sector_buf = vec![0u8; self.bpb.bytes_per_sector as usize];
        self.read_sector(fat_sector, &mut sector_buf)?;

        let entry = match self.fat_type {
            FatType::Fat12 => {
                let val = if offset_in_sector == self.bpb.bytes_per_sector as usize - 1 {
                    // Entry spans two sectors
                    let mut next_sector_buf = vec![0u8; self.bpb.bytes_per_sector as usize];
                    self.read_sector(fat_sector + 1, &mut next_sector_buf)?;
                    (sector_buf[offset_in_sector] as u16) | ((next_sector_buf[0] as u16) << 8)
                } else {
                    u16::from_le_bytes([
                        sector_buf[offset_in_sector],
                        sector_buf[offset_in_sector + 1],
                    ])
                };

                if cluster & 1 != 0 {
                    (val >> 4) as u32
                } else {
                    (val & 0x0FFF) as u32
                }
            }
            FatType::Fat16 => u16::from_le_bytes([
                sector_buf[offset_in_sector],
                sector_buf[offset_in_sector + 1],
            ]) as u32,
            FatType::Fat32 => {
                u32::from_le_bytes([
                    sector_buf[offset_in_sector],
                    sector_buf[offset_in_sector + 1],
                    sector_buf[offset_in_sector + 2],
                    sector_buf[offset_in_sector + 3],
                ]) & 0x0FFFFFFF
            }
        };

        trace!("FatTable: cluster {} -> entry {:#x}", cluster, entry);
        Ok(entry)
    }

    /// Write a FAT entry for the given cluster.
    pub fn write_entry(&mut self, cluster: u32, value: u32) -> Result<(), FileError> {
        trace!(
            "FatTable: writing entry {:#x} for cluster {}",
            value, cluster
        );
        let fat_offset = match self.fat_type {
            FatType::Fat12 => cluster + (cluster / 2),
            FatType::Fat16 => cluster * 2,
            FatType::Fat32 => cluster * 4,
        };

        let fat_sector =
            self.bpb.first_fat_sector() + (fat_offset / self.bpb.bytes_per_sector as u32);
        let offset_in_sector = (fat_offset % self.bpb.bytes_per_sector as u32) as usize;

        let mut sector_buf = vec![0u8; self.bpb.bytes_per_sector as usize];
        self.read_sector(fat_sector, &mut sector_buf)?;

        match self.fat_type {
            FatType::Fat12 => {
                let existing = u16::from_le_bytes([
                    sector_buf[offset_in_sector],
                    sector_buf.get(offset_in_sector + 1).copied().unwrap_or(0),
                ]);

                let new_val = if cluster & 1 != 0 {
                    (existing & 0x000F) | ((value as u16) << 4)
                } else {
                    (existing & 0xF000) | ((value as u16) & 0x0FFF)
                };

                sector_buf[offset_in_sector] = new_val as u8;
                if offset_in_sector + 1 < sector_buf.len() {
                    sector_buf[offset_in_sector + 1] = (new_val >> 8) as u8;
                }
            }
            FatType::Fat16 => {
                let bytes = (value as u16).to_le_bytes();
                sector_buf[offset_in_sector] = bytes[0];
                sector_buf[offset_in_sector + 1] = bytes[1];
            }
            FatType::Fat32 => {
                let existing = u32::from_le_bytes([
                    sector_buf[offset_in_sector],
                    sector_buf[offset_in_sector + 1],
                    sector_buf[offset_in_sector + 2],
                    sector_buf[offset_in_sector + 3],
                ]);
                let new_val = (existing & 0xF0000000) | (value & 0x0FFFFFFF);
                let bytes = new_val.to_le_bytes();
                sector_buf[offset_in_sector..offset_in_sector + 4].copy_from_slice(&bytes);
            }
        }

        self.write_sector(fat_sector, &sector_buf)?;

        // Write to all FAT copies
        for i in 1..self.bpb.num_fats {
            let mirror_sector = fat_sector + (i as u32 * self.bpb.fat_size());
            self.write_sector(mirror_sector, &sector_buf)?;
        }

        Ok(())
    }

    /// Allocate a new cluster, returning its number.
    pub fn allocate_cluster(&mut self) -> Result<u32, FileError> {
        trace!("FatTable: allocating new cluster");
        let total_clusters = (self.bpb.total_sectors() - self.bpb.first_data_sector())
            / self.bpb.sectors_per_cluster as u32;

        for cluster in 2..total_clusters + 2 {
            let entry = self.read_entry(cluster)?;
            if entry == FAT_FREE {
                // Mark as end of chain
                let eoc = match self.fat_type {
                    FatType::Fat12 => 0x0FFF,
                    FatType::Fat16 => 0xFFFF,
                    FatType::Fat32 => 0x0FFFFFFF,
                };
                self.write_entry(cluster, eoc)?;
                trace!("FatTable: allocated cluster {}", cluster);
                return Ok(cluster);
            }
        }

        Err(FileError::Other(String::from("No free clusters")))
    }

    /// Check if entry is end of chain.
    pub fn is_eoc(&self, entry: u32) -> bool {
        match self.fat_type {
            FatType::Fat12 => entry >= 0x0FF8,
            FatType::Fat16 => entry >= 0xFFF8,
            FatType::Fat32 => entry >= 0x0FFFFFF8,
        }
    }

    fn read_sector(&mut self, sector: u32, buf: &mut [u8]) -> Result<(), FileError> {
        let offset = sector as u64 * self.bpb.bytes_per_sector as u64 + self.partition_offset;
        self.device.seek(SeekFrom::Start(offset)).map_err(|e| {
            error!("FatTable: failed to seek to sector {}: {:?}", sector, e);
            FileError::SeekError
        })?;
        self.device.read(buf).map_err(|e| {
            error!("FatTable: failed to read sector {}: {:?}", sector, e);
            FileError::ReadError
        })?;
        Ok(())
    }

    fn write_sector(&mut self, sector: u32, buf: &[u8]) -> Result<(), FileError> {
        let offset = sector as u64 * self.bpb.bytes_per_sector as u64 + self.partition_offset;
        self.device.seek(SeekFrom::Start(offset)).map_err(|e| {
            error!(
                "FatTable: failed to seek to sector {} for write: {:?}",
                sector, e
            );
            FileError::SeekError
        })?;
        self.device.write(buf).map_err(|e| {
            error!("FatTable: failed to write sector {}: {:?}", sector, e);
            FileError::WriteError
        })?;
        Ok(())
    }
}

/// Read a cluster's data.
pub fn read_cluster<D: BlockDevice>(
    device: &mut D,
    bpb: &BiosParameterBlock,
    cluster: u32,
    buf: &mut [u8],
    partition_offset: u64,
) -> Result<(), FileError> {
    trace!("read_cluster: reading cluster {}", cluster);
    let sector = bpb.cluster_to_sector(cluster);
    let offset = (sector as u64 * bpb.bytes_per_sector as u64) + partition_offset;

    device.seek(SeekFrom::Start(offset)).map_err(|e| {
        error!(
            "read_cluster: failed to seek to cluster {}: {:?}",
            cluster, e
        );
        FileError::SeekError
    })?;
    device.read(buf).map_err(|e| {
        error!("read_cluster: failed to read cluster {}: {:?}", cluster, e);
        FileError::ReadError
    })?;
    Ok(())
}

/// Write a cluster's data.
pub fn write_cluster<D: BlockDevice>(
    device: &mut D,
    bpb: &BiosParameterBlock,
    cluster: u32,
    buf: &[u8],
    partition_offset: u64,
) -> Result<(), FileError> {
    trace!("write_cluster: writing cluster {}", cluster);
    let sector = bpb.cluster_to_sector(cluster);
    let offset = sector as u64 * bpb.bytes_per_sector as u64 + partition_offset;

    device.seek(SeekFrom::Start(offset)).map_err(|e| {
        error!(
            "write_cluster: failed to seek to cluster {}: {:?}",
            cluster, e
        );
        FileError::SeekError
    })?;
    device.write(buf).map_err(|e| {
        error!(
            "write_cluster: failed to write cluster {}: {:?}",
            cluster, e
        );
        FileError::WriteError
    })?;
    Ok(())
}
