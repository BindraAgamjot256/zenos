// #![allow(dead_code)]

use crate::disk::FsMountError;
use crate::disk::vfs::{FileType, Inode, InodeOps, Permissions};
use alloc::boxed::Box;
use alloc::string::{String, ToString};
use core::fmt;
use core::fmt::Debug;
use core::sync::atomic::AtomicU64;
use log::error;

pub(super) mod constants {
    pub const SUPERBLOCK_OFFSET: u64 = 1024;
    pub const SUPERBLOCK_SIZE: usize = 1024;
    pub const BLOCK_GROUP_DESCRIPTOR_SIZE: usize = 32;
    pub const ROOT_INODE_NUM: u32 = 2;
    pub const MIN_BLOCK_SIZE: u64 = 1024;
    pub const BGDT_BLOCK_1K: u64 = 2;
    pub const BGDT_BLOCK_DEFAULT: u64 = 1;
    pub const BGDT_ENTRY_SIZE: usize = 32;
    pub const EXT2_FEATURE_RO_COMPAT_SPARSE_SUPER: u32 = 0x0001;
    pub const EXT2_SUPPORTED_RO_COMPAT_FEATURES: u32 = EXT2_FEATURE_RO_COMPAT_SPARSE_SUPER;
}

/// Represents the superblock structure of an EXT2 file system.
/// The superblock stores global metadata required to interpret
/// the layout and capabilities of the file system.
#[derive(Clone)]
pub struct Ext2Superblock {
    /// Total number of inodes in the file system.
    pub(crate) ino_count: u32,
    /// Total number of blocks in the file system.
    pub(crate) blk_count: u32,
    /// Number of blocks reserved for the superuser.
    pub(crate) reserved_blk_count: u32,
    /// Number of currently free blocks.
    pub(crate) free_blocks: u32,
    /// Number of currently free inodes.
    pub(crate) free_inodes: u32,
    /// Block number of the first data block.
    pub(crate) first_data_blk: u32,
    /// Logarithmic block size indicator.
    /// Actual block size = `1024 << log_blk_size`.
    pub(crate) log_blk_size: u32,
    /// Logarithmic fragment size indicator.
    /// Actual fragment size = `1024 << log_frag_size`.
    pub(crate) log_frag_size: u32,
    /// Number of blocks contained in each block group.
    pub(crate) blocks_per_grp: u32,
    /// Number of fragments contained in each block group.
    pub(crate) frags_per_grp: u32,
    /// Number of inodes contained in each block group.
    pub(crate) inodes_per_grp: u32,
    /// Timestamp of the last mount operation (seconds since UNIX epoch).
    pub(crate) mnt_time: u32,
    /// Timestamp of the last write operation (seconds since UNIX epoch).
    pub(crate) wrt_time: u32,
    /// Number of mounts since the last consistency check.
    pub(crate) mnt_count: u16,
    /// Maximum mount count before a consistency check is required.
    pub(crate) max_mnt_count: u16,
    /// Magic signature identifying the file system as EXT2 (`0xEF53`).
    pub(crate) magic: u16,
    /// File system state.
    /// 1 = cleanly unmounted, 2 = errors detected.
    pub(crate) state: u16,
    /// Behavior when file system errors are detected.
    /// 1 = continue, 2 = remount read-only, 3 = kernel panic.
    pub(crate) errors: u16,
    /// Minor revision level of the file system format.
    pub(crate) minor_rev_level: u16,
    /// Time of the last consistency check (seconds since UNIX epoch).
    pub(crate) lastcheck: u32,
    /// Maximum interval between consistency checks (seconds).
    pub(crate) checkinterval: u32,
    /// Identifier of the operating system that created the file system.
    /// 0 = Linux, 1 = GNU Hurd, 2 = MASIX, 3 = FreeBSD, 4 = Other.
    pub(crate) creator_os: u32,
    /// Revision level of the file system.
    /// 0 = original format, 1 = dynamic revision.
    pub(crate) rev_level: u32,
    /// User ID allowed to use reserved blocks.
    pub(crate) res_uid: u16,
    /// Group ID allowed to use reserved blocks.
    pub(crate) res_gid: u16,
    /// First non-reserved inode.
    /// For revision 0 file systems this value is always 11.
    pub(crate) first_ino: u32,
    /// Size of each inode structure in bytes.
    pub(crate) inode_size: u16,
    /// Block group number containing this superblock.
    /// Used when dealing with backup superblocks.
    pub(crate) block_group_nr: u16,
    /// Compatible feature flags.
    /// Unsupported implementations may safely ignore these.
    pub(crate) feature_compat: u32,
    /// Incompatible feature flags.
    /// Implementations must support these to read/write the file system.
    pub(crate) feature_incompat: u32,
    /// Read-only compatible feature flags.
    /// File system may be mounted read-only if unsupported.
    pub(crate) feature_ro_compat: u32,
    /// File system UUID.
    pub(crate) uuid: [u8; 16],
    /// Human-readable volume name (NUL-terminated).
    pub(crate) volume_name: [u8; 16],
    /// Path where the file system was last mounted (NUL-terminated).
    pub(crate) last_mounted: [u8; 64],
    /// Bitmap indicating compression algorithms used by the file system.
    pub(crate) algo_bitmap: u32,
}

impl Ext2Superblock {
    pub const EXT2_FEATURE_INCOMPAT_FILETYPE: u32 = 0x2;

    pub fn deserialize(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 204 {
            return None;
        }
        let ino_count = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let blk_count = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let reserved_blk_count = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let free_blk_count = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let free_inodes = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        let first_data_blk = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
        let log_blk_size = u32::from_le_bytes(bytes[24..28].try_into().unwrap());
        let log_frag_size = u32::from_le_bytes(bytes[28..32].try_into().unwrap());
        let blocks_per_grp = u32::from_le_bytes(bytes[32..36].try_into().unwrap());
        let frags_per_grp = u32::from_le_bytes(bytes[36..40].try_into().unwrap());
        let inodes_per_grp = u32::from_le_bytes(bytes[40..44].try_into().unwrap());
        let mnt_time = u32::from_le_bytes(bytes[44..48].try_into().unwrap());
        let wrt_time = u32::from_le_bytes(bytes[48..52].try_into().unwrap());
        let mnt_count = u16::from_le_bytes(bytes[52..54].try_into().unwrap());
        let max_mnt_count = u16::from_le_bytes(bytes[54..56].try_into().unwrap());
        let magic = u16::from_le_bytes(bytes[56..58].try_into().unwrap());
        let state = u16::from_le_bytes(bytes[58..60].try_into().unwrap());
        let errors = u16::from_le_bytes(bytes[60..62].try_into().unwrap());
        let minor_rev_level = u16::from_le_bytes(bytes[62..64].try_into().unwrap());
        let lastcheck = u32::from_le_bytes(bytes[64..68].try_into().unwrap());
        let checkinterval = u32::from_le_bytes(bytes[68..72].try_into().unwrap());
        let creator_os = u32::from_le_bytes(bytes[72..76].try_into().unwrap());
        let rev_level = u32::from_le_bytes(bytes[76..80].try_into().unwrap());
        let res_uid = u16::from_le_bytes(bytes[80..82].try_into().unwrap());
        let res_gid = u16::from_le_bytes(bytes[82..84].try_into().unwrap());

        let first_ino = u32::from_le_bytes(bytes[84..88].try_into().unwrap());
        let inode_size = u16::from_le_bytes(bytes[88..90].try_into().unwrap());
        let block_group_nr = u16::from_le_bytes(bytes[90..92].try_into().unwrap());
        let feature_compat = u32::from_le_bytes(bytes[92..96].try_into().unwrap());
        let feature_incompat = u32::from_le_bytes(bytes[96..100].try_into().unwrap());
        let feature_ro_compat = u32::from_le_bytes(bytes[100..104].try_into().unwrap());

        let uuid = bytes[104..120].try_into().unwrap();
        let volume_name = bytes[120..136].try_into().unwrap();
        let last_mounted = bytes[136..200].try_into().unwrap();

        let algo_bitmap = u32::from_le_bytes(bytes[200..204].try_into().unwrap());

        Some(Ext2Superblock {
            ino_count,
            blk_count,
            reserved_blk_count,
            free_blocks: free_blk_count,
            free_inodes,
            first_data_blk,
            log_blk_size,
            log_frag_size,
            blocks_per_grp,
            frags_per_grp,
            inodes_per_grp,
            mnt_time,
            wrt_time,
            mnt_count,
            max_mnt_count,
            magic,
            state,
            errors,
            minor_rev_level,
            lastcheck,
            checkinterval,
            creator_os,
            rev_level,
            res_uid,
            res_gid,

            first_ino,
            inode_size,
            block_group_nr,
            feature_compat,
            feature_incompat,
            feature_ro_compat,
            uuid,
            volume_name,
            last_mounted,
            algo_bitmap,
        })
    }

    pub fn validate(&self) -> Result<(), FsMountError> {
        // EXT2 magic check
        if self.magic != 0xEF53 {
            error!("invalid ext2 magic: {:04x}", self.magic);
            return Err(FsMountError::InvalidSuperblock);
        }

        // Block size sanity check
        let block_size = 1024u32
            .checked_shl(self.log_blk_size)
            .ok_or(FsMountError::Corrupted)?;

        if block_size < 1024 || block_size > 65536 {
            return Err(FsMountError::Corrupted);
        }

        // inode size check
        if self.rev_level >= 1 {
            if self.inode_size < 128 || self.inode_size % 4 != 0 {
                return Err(FsMountError::Corrupted);
            }
        }

        // group layout sanity
        if self.blocks_per_grp == 0 || self.inodes_per_grp == 0 {
            return Err(FsMountError::Corrupted);
        }

        // count checks
        if self.free_blocks > self.blk_count {
            return Err(FsMountError::Corrupted);
        }

        if self.free_inodes > self.ino_count {
            return Err(FsMountError::Corrupted);
        }

        // first inode sanity
        if self.first_ino == 0 {
            return Err(FsMountError::Corrupted);
        }

        // unsupported incompatible features
        // (basic ext2 driver usually supports none)
        let unsupported = self.feature_incompat & !Self::EXT2_FEATURE_INCOMPAT_FILETYPE;

        if unsupported != 0 {
            error!(
                "unsupported ext2 incompatible features: {:08x}",
                unsupported
            );
            return Err(FsMountError::Unsupported);
        }
        Ok(())
    }

    fn volume_name_string(&self) -> String {
        String::from_utf8_lossy(&self.volume_name)
            .trim_end_matches('\0')
            .to_string()
    }

    fn last_mounted_string(&self) -> String {
        String::from_utf8_lossy(&self.last_mounted)
            .trim_end_matches('\0')
            .to_string()
    }

    pub(crate) fn block_size(&self) -> u32 {
        1024 << self.log_blk_size
    }

    pub fn serialize(&self) -> [u8; 1024] {
        let mut buf = [0u8; 1024];
        buf[0..4].copy_from_slice(&self.ino_count.to_le_bytes());
        buf[4..8].copy_from_slice(&self.blk_count.to_le_bytes());
        buf[8..12].copy_from_slice(&self.reserved_blk_count.to_le_bytes());
        buf[12..16].copy_from_slice(&self.free_blocks.to_le_bytes());
        buf[16..20].copy_from_slice(&self.free_inodes.to_le_bytes());
        buf[20..24].copy_from_slice(&self.first_data_blk.to_le_bytes());
        buf[24..28].copy_from_slice(&self.log_blk_size.to_le_bytes());
        buf[28..32].copy_from_slice(&self.log_frag_size.to_le_bytes());
        buf[32..36].copy_from_slice(&self.blocks_per_grp.to_le_bytes());
        buf[36..40].copy_from_slice(&self.frags_per_grp.to_le_bytes());
        buf[40..44].copy_from_slice(&self.inodes_per_grp.to_le_bytes());
        buf[44..48].copy_from_slice(&self.mnt_time.to_le_bytes());
        buf[48..52].copy_from_slice(&self.wrt_time.to_le_bytes());
        buf[52..54].copy_from_slice(&self.mnt_count.to_le_bytes());
        buf[54..56].copy_from_slice(&self.max_mnt_count.to_le_bytes());
        buf[56..58].copy_from_slice(&self.magic.to_le_bytes());
        buf[58..60].copy_from_slice(&self.state.to_le_bytes());
        buf[60..62].copy_from_slice(&self.errors.to_le_bytes());
        buf[62..64].copy_from_slice(&self.minor_rev_level.to_le_bytes());
        buf[64..68].copy_from_slice(&self.lastcheck.to_le_bytes());
        buf[68..72].copy_from_slice(&self.checkinterval.to_le_bytes());
        buf[72..76].copy_from_slice(&self.creator_os.to_le_bytes());
        buf[76..80].copy_from_slice(&self.rev_level.to_le_bytes());
        buf[80..82].copy_from_slice(&self.res_uid.to_le_bytes());
        buf[82..84].copy_from_slice(&self.res_gid.to_le_bytes());
        buf[84..88].copy_from_slice(&self.first_ino.to_le_bytes());
        buf[88..90].copy_from_slice(&self.inode_size.to_le_bytes());
        buf[90..92].copy_from_slice(&self.block_group_nr.to_le_bytes());
        buf[92..96].copy_from_slice(&self.feature_compat.to_le_bytes());
        buf[96..100].copy_from_slice(&self.feature_incompat.to_le_bytes());
        buf[100..104].copy_from_slice(&self.feature_ro_compat.to_le_bytes());
        buf[104..120].copy_from_slice(&self.uuid);
        buf[120..136].copy_from_slice(&self.volume_name);
        buf[136..200].copy_from_slice(&self.last_mounted);
        buf[200..204].copy_from_slice(&self.algo_bitmap.to_le_bytes());
        buf
    }
}

impl fmt::Debug for Ext2Superblock {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Superblock")
            .field("volume_name", &self.volume_name_string())
            .field("last_mounted", &self.last_mounted_string()) // technically, this path is never used, but sure... why not
            .field("block_size", &self.block_size())
            .field("inode_size", &self.inode_size)
            .field("inode_count", &self.ino_count)
            .field("block_count", &self.blk_count)
            .field("free_blocks", &self.free_blocks)
            .field("free_inodes", &self.free_inodes)
            .field("blocks_per_group", &self.blocks_per_grp)
            .field("inodes_per_group", &self.inodes_per_grp)
            .field("mount_count", &self.mnt_count)
            .field("max_mount_count", &self.max_mnt_count)
            .field("rev_level", &self.rev_level)
            .field("creator_os", &self.creator_os)
            .finish()
    }
}

/// Represents a block group descriptor in the EXT2 file system.
/// Each block group has a descriptor that contains metadata about the group,
/// including the locations of block and inode bitmaps, the inode table, and counts of free blocks and inodes.
#[derive(Clone)]
pub(crate) struct BlockGroupDescriptorTable {
    /// Block number of the block bitmap for this group.
    pub(crate) bg_block_bitmap: u32,
    /// Block number of the inode bitmap for this group.
    pub(crate) bg_inode_bitmap: u32,
    /// Block number of the inode table for this group.
    pub(crate) bg_inode_table: u32,
    /// Number of free blocks in this group.
    pub(crate) bg_free_blocks_count: u16,
    /// Number of free inodes in this group.
    pub(crate) bg_free_inodes_count: u16,
    /// Number of directories in this group.
    pub(crate) bg_used_dirs_count: u16,
    /// Padding to make the structure32 bytes \(EXT2 block group descriptor size\).
    pub(crate) _padding: [u8; 14],
}

impl BlockGroupDescriptorTable {
    pub(crate) fn deserialize(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 32 {
            error!(
                "block group descriptor table entry too short: {} bytes",
                bytes.len()
            );
            return None;
        }
        let bg_block_bitmap = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let bg_inode_bitmap = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let bg_inode_table = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let bg_free_blocks_count = u16::from_le_bytes(bytes[12..14].try_into().unwrap());
        let bg_free_inodes_count = u16::from_le_bytes(bytes[14..16].try_into().unwrap());
        let bg_used_dirs_count = u16::from_le_bytes(bytes[16..18].try_into().unwrap());

        Some(BlockGroupDescriptorTable {
            bg_block_bitmap,
            bg_inode_bitmap,
            bg_inode_table,
            bg_free_blocks_count,
            bg_free_inodes_count,
            bg_used_dirs_count,
            _padding: bytes[18..32].try_into().unwrap(), // just store the padding as-is, we don't care about it
        })
    }
}

impl Debug for BlockGroupDescriptorTable {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("BlockGroupDescriptor")
            .field("block_bitmap", &self.bg_block_bitmap)
            .field("inode_bitmap", &self.bg_inode_bitmap)
            .field("inode_table", &self.bg_inode_table)
            .field("free_blocks_count", &self.bg_free_blocks_count)
            .field("free_inodes_count", &self.bg_free_inodes_count)
            .field("used_dirs_count", &self.bg_used_dirs_count)
            .finish()
    }
}

#[derive(Clone)]
pub(crate) struct Ext2Inode {
    pub(crate) i_mode: u16,
    pub(crate) i_uid: u16,
    pub(crate) i_size: u32,
    pub(crate) i_atime: u32,
    pub(crate) i_ctime: u32,
    pub(crate) i_mtime: u32,
    pub(crate) i_dtime: u32,
    pub(crate) i_gid: u16,
    pub(crate) i_links_count: u16,
    pub(crate) i_blocks: u32,
    pub(crate) i_flags: u32,
    pub(crate) _i_osd1: u32,
    pub(crate) i_block: [u32; 15],
    pub(crate) i_generation: u32,
    pub(crate) i_file_acl: u32,
    pub(crate) i_dir_acl: u32,
    pub(crate) i_faddr: u32,
    pub(crate) _i_osd2: [u8; 12],
}

impl Ext2Inode {
    pub fn deserialize(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 128 {
            return None;
        }
        let i_mode = u16::from_le_bytes(bytes[0..2].try_into().unwrap());
        let i_uid = u16::from_le_bytes(bytes[2..4].try_into().unwrap());
        let i_size = u32::from_le_bytes(bytes[4..8].try_into().unwrap());
        let i_atime = u32::from_le_bytes(bytes[8..12].try_into().unwrap());
        let i_ctime = u32::from_le_bytes(bytes[12..16].try_into().unwrap());
        let i_mtime = u32::from_le_bytes(bytes[16..20].try_into().unwrap());
        let i_dtime = u32::from_le_bytes(bytes[20..24].try_into().unwrap());
        let i_gid = u16::from_le_bytes(bytes[24..26].try_into().unwrap());
        let i_links_count = u16::from_le_bytes(bytes[26..28].try_into().unwrap());
        let i_blocks = u32::from_le_bytes(bytes[28..32].try_into().unwrap());
        let i_flags = u32::from_le_bytes(bytes[32..36].try_into().unwrap());
        let i_osd1 = u32::from_le_bytes(bytes[36..40].try_into().unwrap());

        let mut i_block = [0u32; 15];
        for j in 0..15 {
            i_block[j] = u32::from_le_bytes(bytes[(40 + j * 4)..(44 + j * 4)].try_into().unwrap());
        }

        let i_generation = u32::from_le_bytes(bytes[100..104].try_into().unwrap());
        let i_file_acl = u32::from_le_bytes(bytes[104..108].try_into().unwrap());
        let i_dir_acl = u32::from_le_bytes(bytes[108..112].try_into().unwrap());
        let i_faddr = u32::from_le_bytes(bytes[112..116].try_into().unwrap());

        Some(Ext2Inode {
            i_mode,
            i_uid,
            i_size,
            i_atime,
            i_ctime,
            i_mtime,
            i_dtime,
            i_gid,
            i_links_count,
            i_blocks,
            i_flags,
            _i_osd1: i_osd1,
            i_block,
            i_generation,
            i_file_acl,
            i_dir_acl,
            i_faddr,
            _i_osd2: bytes[116..128].try_into().unwrap(),
        })
    }

    pub fn serialize(&self) -> [u8; 128] {
        let mut buf = [0u8; 128];
        buf[0..2].copy_from_slice(&self.i_mode.to_le_bytes());
        buf[2..4].copy_from_slice(&self.i_uid.to_le_bytes());
        buf[4..8].copy_from_slice(&self.i_size.to_le_bytes());
        buf[8..12].copy_from_slice(&self.i_atime.to_le_bytes());
        buf[12..16].copy_from_slice(&self.i_ctime.to_le_bytes());
        buf[16..20].copy_from_slice(&self.i_mtime.to_le_bytes());
        buf[20..24].copy_from_slice(&self.i_dtime.to_le_bytes());
        buf[24..26].copy_from_slice(&self.i_gid.to_le_bytes());
        buf[26..28].copy_from_slice(&self.i_links_count.to_le_bytes());
        buf[28..32].copy_from_slice(&self.i_blocks.to_le_bytes());
        buf[32..36].copy_from_slice(&self.i_flags.to_le_bytes());
        buf[36..40].copy_from_slice(&self._i_osd1.to_le_bytes());
        for j in 0..15 {
            buf[(40 + j * 4)..(44 + j * 4)].copy_from_slice(&self.i_block[j].to_le_bytes());
        }
        buf[100..104].copy_from_slice(&self.i_generation.to_le_bytes());
        buf[104..108].copy_from_slice(&self.i_file_acl.to_le_bytes());
        buf[108..112].copy_from_slice(&self.i_dir_acl.to_le_bytes());
        buf[112..116].copy_from_slice(&self.i_faddr.to_le_bytes());
        buf[116..128].copy_from_slice(&self._i_osd2);
        buf
    }
}

pub(super) fn convert_ext2_inode_to_vfs_inode(
    ino: Ext2Inode,
    ino_num: u64,
    data: Box<dyn InodeOps + Send + Sync>,
) -> Inode {
    let filetype = match ino.i_mode & 0xF000 {
        0x4000 => FileType::Directory, // directory
        0x8000 => FileType::File,      // regular file
        0xA000 => FileType::Symlink,   // symbolic link
        0xC000 => FileType::Socket,
        0x2000 => FileType::CharDevice,
        0x6000 => FileType::BlockDevice,
        0x1000 => FileType::Fifo,
        _ => panic!("invalid file type"), // treat unknown types as invalid.
    };
    let perms = Permissions::from_bits_truncate(ino.i_mode & 0x0FFF);

    Inode {
        num: ino_num,
        kind: filetype,
        perms,
        size: AtomicU64::new(ino.i_size as u64),
        links: AtomicU64::new(ino.i_links_count as u64),
        data,
    }
}

pub struct DirEntry {
    pub(crate) inode: u32,
    pub(crate) rec_len: u16,
    pub(crate) name_len: u8,
    pub(crate) file_type: u8,
    pub(crate) name: String,
}

impl DirEntry {
    pub fn deserialize(bytes: &[u8]) -> Option<Self> {
        if bytes.len() < 8 {
            return None;
        }
        let inode = u32::from_le_bytes(bytes[0..4].try_into().unwrap());
        let rec_len = u16::from_le_bytes(bytes[4..6].try_into().unwrap());
        let name_len = bytes[6];
        let file_type = bytes[7];
        if bytes.len() < rec_len as usize || name_len as usize > (rec_len as usize - 8) {
            return None;
        }
        let name_bytes = &bytes[8..(8 + name_len as usize)];
        let name = String::from_utf8_lossy(name_bytes).to_string();
        Some(DirEntry {
            inode,
            rec_len,
            name_len,
            file_type,
            name,
        })
    }
}
