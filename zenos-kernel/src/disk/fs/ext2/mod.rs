use crate::disk::{
    FileError, FsMountError,
    block::BlockDevice,
    fs::ext2::plumbing::{
        BlockGroupDescriptorTable, Ext2Inode, Ext2Superblock, constants::*,
        convert_ext2_inode_to_vfs_inode,
    },
    vfs::{DirEntry, FileSystem, FileType, Inode, InodeOps, Permissions, SeekFrom},
};
use alloc::{boxed::Box, string::ToString, sync::Arc, vec, vec::Vec};
use log::{error, info, warn};
use spin::Mutex;

mod plumbing;

struct Ext2FSInner<D: BlockDevice + 'static> {
    device: D,
    partition_offset: u64,
    superblock: Ext2Superblock,
    block_group_descriptors: Vec<BlockGroupDescriptorTable>,
    read_only: bool,
}

impl<D: BlockDevice + 'static> Ext2FSInner<D> {
    pub fn get_block_offset_for_inode_num(&self, inode_num: u32) -> Result<u64, FileError> {
        let guard = self;
        let superblock = &guard.superblock;
        let group = (inode_num - 1) / superblock.inodes_per_grp;
        let index = (inode_num - 1) % superblock.inodes_per_grp;

        if group as usize >= guard.block_group_descriptors.len() {
            return Err(FileError::Other("Inode number out of range".into()));
        }
        let bgdt = &guard.block_group_descriptors[group as usize];

        let byte_offset = index as u64 * superblock.inode_size as u64;
        let block = bgdt.bg_inode_table as u64 + (byte_offset / superblock.block_size() as u64);
        let offset = byte_offset as u64 % superblock.block_size() as u64;
        Ok(guard.partition_offset + block * superblock.block_size() as u64 + offset)
    }

    /// Allocate a free block from the filesystem. Returns the block number.
    pub fn allocate_block(&mut self) -> Result<u32, FileError> {
        let block_size = self.superblock.block_size() as usize;

        for (group_idx, bgdt) in self.block_group_descriptors.iter_mut().enumerate() {
            if bgdt.bg_free_blocks_count == 0 {
                continue;
            }

            // Read the block bitmap
            let bitmap_offset =
                self.partition_offset + bgdt.bg_block_bitmap as u64 * block_size as u64;
            let mut bitmap = vec![0u8; block_size];

            self.device
                .seek(SeekFrom::Start(bitmap_offset))
                .map_err(|_| FileError::SeekError)?;
            self.device
                .read_exact(&mut bitmap)
                .map_err(|_| FileError::ReadError)?;

            // Find a free bit in the bitmap
            for (byte_idx, byte) in bitmap.iter_mut().enumerate() {
                if *byte == 0xFF {
                    continue;
                }
                for bit in 0..8 {
                    if (*byte & (1 << bit)) == 0 {
                        // Found a free block
                        *byte |= 1 << bit;

                        // Write the updated bitmap back
                        self.device
                            .seek(SeekFrom::Start(bitmap_offset))
                            .map_err(|_| FileError::SeekError)?;
                        self.device
                            .write(&bitmap)
                            .map_err(|_| FileError::WriteError)?;

                        // Update block group descriptor
                        bgdt.bg_free_blocks_count -= 1;

                        // Update superblock free block count
                        self.superblock.free_blocks -= 1;

                        let block_num = self.superblock.first_data_blk
                            + group_idx as u32 * self.superblock.blocks_per_grp
                            + (byte_idx * 8 + bit) as u32;
                        return Ok(block_num);
                    }
                }
            }
        }

        Err(FileError::Other("No free blocks available".into()))
    }

    pub fn free_inode(&mut self, inode_num: u32) -> Result<(), FileError> {
        let block_size = self.superblock.block_size() as usize;

        let group = (inode_num - 1) / self.superblock.inodes_per_grp;
        let index = (inode_num - 1) % self.superblock.inodes_per_grp;

        let bgdt = &mut self.block_group_descriptors[group as usize];

        let bitmap_offset = self.partition_offset + bgdt.bg_inode_bitmap as u64 * block_size as u64;

        let mut bitmap = vec![0u8; block_size];

        self.device
            .seek(SeekFrom::Start(bitmap_offset))
            .map_err(|e| FileError::SeekError)?;
        self.device
            .read_exact(&mut bitmap)
            .map_err(|e| FileError::ReadError)?;

        let byte = (index / 8) as usize;
        let bit = (index % 8) as u8;

        bitmap[byte] &= !(1 << bit);

        self.device
            .seek(SeekFrom::Start(bitmap_offset))
            .map_err(|e| FileError::SeekError)?;
        self.device
            .write(&bitmap)
            .map_err(|e| FileError::WriteError)?;

        bgdt.bg_free_inodes_count += 1;
        self.superblock.free_inodes += 1;

        Ok(())
    }

    /// Free a block back to the filesystem.
    pub fn free_block(&mut self, block_num: u32) -> Result<(), FileError> {
        let block_size = self.superblock.block_size() as usize;
        let group = block_num / self.superblock.blocks_per_grp;
        let index = block_num % self.superblock.blocks_per_grp;

        if group as usize >= self.block_group_descriptors.len() {
            return Err(FileError::Other("Block number out of range".into()));
        }

        let bgdt = &mut self.block_group_descriptors[group as usize];

        // Read the block bitmap
        let bitmap_offset = self.partition_offset + bgdt.bg_block_bitmap as u64 * block_size as u64;
        let mut bitmap = vec![0u8; block_size];

        self.device
            .seek(SeekFrom::Start(bitmap_offset))
            .map_err(|_| FileError::SeekError)?;
        self.device
            .read_exact(&mut bitmap)
            .map_err(|_| FileError::ReadError)?;

        // Clear the bit
        let byte_idx = (index / 8) as usize;
        let bit = (index % 8) as u8;
        bitmap[byte_idx] &= !(1 << bit);

        // Write the updated bitmap back
        self.device
            .seek(SeekFrom::Start(bitmap_offset))
            .map_err(|_| FileError::SeekError)?;
        self.device
            .write(&bitmap)
            .map_err(|_| FileError::WriteError)?;

        // Update block group descriptor
        bgdt.bg_free_blocks_count += 1;

        // Update superblock free block count
        self.superblock.free_blocks += 1;

        Ok(())
    }

    /// Read a block from the device.
    pub fn read_block(&mut self, block_num: u32, buf: &mut [u8]) -> Result<(), FileError> {
        let block_size = self.superblock.block_size() as u64;
        let offset = self.partition_offset + block_num as u64 * block_size;

        self.device
            .seek(SeekFrom::Start(offset))
            .map_err(|_| FileError::SeekError)?;
        self.device
            .read_exact(buf)
            .map_err(|_| FileError::ReadError)?;
        Ok(())
    }

    /// Write a block to the device.
    pub fn write_block(&mut self, block_num: u32, buf: &[u8]) -> Result<(), FileError> {
        let block_size = self.superblock.block_size() as u64;
        let offset = self.partition_offset + block_num as u64 * block_size;

        self.device
            .seek(SeekFrom::Start(offset))
            .map_err(|_| FileError::SeekError)?;
        self.device.write(buf).map_err(|_| FileError::WriteError)?;
        Ok(())
    }

    /// Check if a block group has a superblock backup.
    /// For sparse superblock filesystems, only groups 0, 1, and powers of 3, 5, 7 have backups.
    fn has_superblock_backup(&self, group: u32) -> bool {
        if group == 0 || group == 1 {
            return true;
        }
        // Check if group is a power of 3, 5, or 7
        for base in [3u32, 5, 7] {
            let mut power = base;
            while power < group {
                power = match power.checked_mul(base) {
                    Some(p) => p,
                    None => break,
                };
            }
            if power == group {
                return true;
            }
        }
        false
    }

    /// Get all block group numbers that contain superblock copies.
    fn get_superblock_groups(&self) -> Vec<u32> {
        let total_groups = (self.superblock.blk_count + self.superblock.blocks_per_grp - 1)
            / self.superblock.blocks_per_grp;
        let is_sparse =
            (self.superblock.feature_ro_compat & EXT2_FEATURE_RO_COMPAT_SPARSE_SUPER) != 0;

        let mut groups = Vec::new();
        for group in 0..total_groups {
            if !is_sparse || self.has_superblock_backup(group) {
                groups.push(group);
            }
        }
        groups
    }

    /// Sync superblock to disk, updating write time and all backup copies.
    pub fn sync_superblock(&mut self) -> Result<(), FileError> {
        // Update write time with magic value 0xDEADBEEFCAFEBABE (truncated to u32)
        self.superblock.wrt_time = 0xCAFEBABE;

        let block_size = self.superblock.block_size() as u64;
        let groups = self.get_superblock_groups();

        for group in groups {
            // Update block_group_nr for this backup
            self.superblock.block_group_nr = group as u16;
            let buf = self.superblock.serialize();

            // Calculate superblock offset for this group
            let group_start_block = group as u64 * self.superblock.blocks_per_grp as u64;
            let superblock_offset = if group == 0 {
                // Group 0: superblock is at byte offset 1024 (after boot sector)
                self.partition_offset + SUPERBLOCK_OFFSET
            } else {
                // Other groups: superblock is at the start of the group's first block
                self.partition_offset + group_start_block * block_size
            };

            self.device
                .seek(SeekFrom::Start(superblock_offset))
                .map_err(|_| FileError::SeekError)?;
            self.device.write(&buf).map_err(|_| FileError::WriteError)?;
        }

        // Restore block_group_nr to 0 for the primary superblock in memory
        self.superblock.block_group_nr = 0;

        Ok(())
    }
}

pub(crate) struct Ext2<D: BlockDevice + 'static> {
    inner: Arc<Mutex<Ext2FSInner<D>>>,
}

impl<D: BlockDevice + 'static> Ext2<D> {
    pub fn mount(mut device: D, partition_offset: u64) -> Result<Self, FsMountError> {
        // -------------------------
        // Read superblock
        // -------------------------

        let mut superblock_buf = [0u8; SUPERBLOCK_SIZE];

        device
            .seek(SeekFrom::Start(partition_offset + SUPERBLOCK_OFFSET))
            .map_err(|_| FsMountError::ReadError)?;

        let n = device
            .read_exact(&mut superblock_buf)
            .map_err(|_| FsMountError::ReadError)?;

        info!("read superblock: {n} bytes");
        info!(
            "magic raw: {:02x} {:02x}",
            superblock_buf[56], superblock_buf[57]
        );

        let superblock =
            Ext2Superblock::deserialize(&superblock_buf).ok_or(FsMountError::ParseError)?;

        superblock.validate()?;

        info!("mounted ext2: {:#?}", superblock);

        // -------------------------
        // Compute block groups
        // -------------------------

        let groups =
            (superblock.blk_count + superblock.blocks_per_grp - 1) / superblock.blocks_per_grp;

        info!("block groups: {}", groups);

        // -------------------------
        // Locate BGDT
        // -------------------------

        let block_size = superblock.block_size() as u64;

        // EXT2 rule:
        // if block_size == 1024 → BGDT starts at block 2
        // otherwise → block 1
        let bgdt_block = if block_size == MIN_BLOCK_SIZE {
            BGDT_BLOCK_1K
        } else {
            BGDT_BLOCK_DEFAULT
        };

        let bgdt_offset = partition_offset + (bgdt_block * block_size);

        // -------------------------
        // Read BGDT
        // -------------------------

        let bgdt_size = groups as usize * BGDT_ENTRY_SIZE;

        let mut bgdt_buf = vec![0u8; bgdt_size];

        device
            .seek(SeekFrom::Start(bgdt_offset))
            .map_err(|_| FsMountError::ReadError)?;

        device
            .read_exact(&mut bgdt_buf)
            .map_err(|_| FsMountError::ReadError)?;

        // -------------------------
        // Deserialize BGDT entries
        // -------------------------

        let mut descriptors = Vec::with_capacity(groups as usize);

        for chunk in bgdt_buf.chunks(BGDT_ENTRY_SIZE) {
            let desc =
                BlockGroupDescriptorTable::deserialize(chunk).ok_or(FsMountError::ParseError)?;

            info!("BGDT entry: {:#?}", desc);

            descriptors.push(desc);
        }

        // -------------------------
        // Construct FS
        // -------------------------

        // Mount read-only if superblock has unsupported ro_compat features
        let read_only = superblock.feature_ro_compat & !EXT2_SUPPORTED_RO_COMPAT_FEATURES != 0;
        if read_only {
            warn!(
                "ext2: mounting read-only due to ro_compat features: {:08x}",
                superblock.feature_ro_compat
            );
        }

        Ok(Self {
            inner: Arc::new(Mutex::new(Ext2FSInner {
                device,
                partition_offset,
                superblock,
                block_group_descriptors: descriptors,
                read_only,
            })),
        })
    }
}

impl<D> FileSystem for Ext2<D>
where
    D: BlockDevice + 'static,
{
    fn root_dir(&self) -> Result<Arc<Mutex<Inode>>, FileError> {
        let inode_offset = self
            .inner
            .lock()
            .get_block_offset_for_inode_num(ROOT_INODE_NUM)?;

        let mut guard = self.inner.lock();
        let mut buf = vec![0u8; guard.superblock.inode_size as usize];

        guard
            .device
            .seek(SeekFrom::Start(inode_offset))
            .map_err(|_| FileError::SeekError)?;

        guard
            .device
            .read_exact(&mut buf)
            .map_err(|_| FileError::ReadError)?;

        let inode =
            Ext2Inode::deserialize(&buf).ok_or(FileError::Other("Error Parsing Inode".into()))?;

        Ok(Arc::new(Mutex::new(convert_ext2_inode_to_vfs_inode(
            inode.clone(),
            ROOT_INODE_NUM as u64,
            Box::new(ExtDirInodeOps {
                inner: self.inner.clone(),
                ext2inode: inode,
            }),
        ))))
    }
}

struct ExtDirInodeOps<D: BlockDevice + 'static> {
    inner: Arc<Mutex<Ext2FSInner<D>>>,
    ext2inode: Ext2Inode,
}

impl<D: BlockDevice + 'static> ExtDirInodeOps<D> {
    fn read_block_u32_array(guard: &mut Ext2FSInner<D>, block: u32) -> Result<Vec<u32>, FileError> {
        let block_size = guard.superblock.block_size() as u64;

        let offset = guard.partition_offset + block as u64 * block_size;

        let mut buf = vec![0u8; block_size as usize];

        guard
            .device
            .seek(SeekFrom::Start(offset))
            .map_err(|_| FileError::SeekError)?;

        guard
            .device
            .read_exact(&mut buf)
            .map_err(|_| FileError::ReadError)?;

        let mut ptrs = Vec::new();

        for chunk in buf.chunks_exact(4) {
            let val = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            if val != 0 {
                ptrs.push(val);
            }
        }

        Ok(ptrs)
    }

    fn collect_data_blocks(&self, guard: &mut Ext2FSInner<D>) -> Result<Vec<u32>, FileError> {
        let mut blocks = Vec::new();

        for &b in &self.ext2inode.i_block[0..12] {
            if b != 0 {
                blocks.push(b);
            }
        }

        if self.ext2inode.i_block[12] != 0 {
            let ptrs = Self::read_block_u32_array(guard, self.ext2inode.i_block[12])?;
            blocks.extend(ptrs);
        }

        if self.ext2inode.i_block[13] != 0 {
            let l1 = Self::read_block_u32_array(guard, self.ext2inode.i_block[13])?;

            for b in l1 {
                let l2 = Self::read_block_u32_array(guard, b)?;
                blocks.extend(l2);
            }
        }

        if self.ext2inode.i_block[14] != 0 {
            let l1 = Self::read_block_u32_array(guard, self.ext2inode.i_block[14])?;

            for b1 in l1 {
                let l2 = Self::read_block_u32_array(guard, b1)?;

                for b2 in l2 {
                    let l3 = Self::read_block_u32_array(guard, b2)?;
                    blocks.extend(l3);
                }
            }
        }

        Ok(blocks)
    }
    fn get_dentries(&mut self) -> Result<Vec<plumbing::DirEntry>, FileError> {
        let mut entries = Vec::new();
        let mut guard = self.inner.lock();

        let block_size = guard.superblock.block_size() as u64;

        // Read through direct block pointers
        let data_blocks = self.collect_data_blocks(&mut guard)?;

        let dir_size = self.ext2inode.i_size as usize;
        let mut bytes_read = 0;

        for block_ptr in data_blocks {
            if block_ptr == 0 {
                break;
            }
            if bytes_read >= dir_size {
                break;
            }

            let block_offset = guard.partition_offset + (block_ptr as u64 * block_size);
            let mut block_buf = vec![0u8; block_size as usize];

            guard
                .device
                .seek(SeekFrom::Start(block_offset))
                .map_err(|_| FileError::SeekError)?;
            guard
                .device
                .read_exact(&mut block_buf)
                .map_err(|_| FileError::ReadError)?;

            // Parse directory entries in this block
            let mut offset = 0usize;

            while offset < block_buf.len() {
                let dentry = plumbing::DirEntry::deserialize(&block_buf[offset..]);

                if let Some(dentry) = dentry {
                    let rec_len = dentry.rec_len as usize;

                    if rec_len == 0 || offset + rec_len > block_buf.len() {
                        error!("Invalid rec_len {} at offset {}", rec_len, offset);
                        break;
                    }

                    if dentry.inode != 0 {
                        entries.push(dentry);
                    }

                    offset += rec_len;
                    bytes_read += rec_len;
                } else {
                    error!("Error parsing directory entry at offset {}", offset);
                    break;
                }
            }
        }

        Ok(entries)
    }
}

impl<D: BlockDevice + 'static> InodeOps for ExtDirInodeOps<D> {
    fn read(&mut self, _offset: u64, _buf: &mut [u8]) -> Result<usize, FileError> {
        Err(FileError::IsADirectory)
    }

    fn write(&mut self, _offset: u64, _buf: &[u8]) -> Result<usize, FileError> {
        Err(FileError::IsADirectory)
    }

    fn truncate(&mut self, _size: u64) -> Result<(), FileError> {
        Err(FileError::IsADirectory)
    }

    fn sync(&mut self) -> Result<(), FileError> {
        // Directories don't need explicit sync in our implementation
        Ok(())
    }

    fn unlink(&mut self, _name: &str) -> Result<(), FileError> {
        let mut guard = self.inner.lock();

        if guard.read_only {
            return Err(FileError::ReadOnlyFilesystem);
        }

        todo!();
    }

    fn lookup(&mut self, name: &str) -> Result<Arc<Mutex<Inode>>, FileError> {
        let mut entries = self.get_dentries()?;
        let entry = entries
            .iter_mut()
            .find(|e| e.name == name)
            .ok_or(FileError::NotFound)?;
        let mut guard = self.inner.lock();
        let inode_offset = guard.get_block_offset_for_inode_num(entry.inode)?;
        let mut buf = vec![0u8; guard.superblock.inode_size as usize];

        guard
            .device
            .seek(SeekFrom::Start(inode_offset))
            .map_err(|_| FileError::SeekError)?;
        guard
            .device
            .read_exact(&mut buf)
            .map_err(|_| FileError::ReadError)?;

        let ext2_inode =
            Ext2Inode::deserialize(&buf).ok_or(FileError::Other("Error parsing inode".into()))?;

        let data = match ext2_inode.i_mode & 0xF000 {
            0x4000 => Box::new(ExtDirInodeOps {
                inner: self.inner.clone(),
                ext2inode: ext2_inode.clone(),
            }) as Box<dyn InodeOps + Send + Sync>,

            0x8000 => Box::new(ExtFileInodeOps {
                inner: self.inner.clone(),
                ext2inode: ext2_inode.clone(),
                inode_num: entry.inode,
            }) as Box<dyn InodeOps + Send + Sync>,

            0xA000 => {
                todo!("Symlink InodeOps not implemented yet")
            }
            _ => {
                return Err(FileError::Other("Unsupported file type".into()));
            }
        };

        return Ok(Arc::new(Mutex::new(convert_ext2_inode_to_vfs_inode(
            ext2_inode.clone(),
            entry.inode as u64,
            data,
        ))));
    }

    fn create(
        &mut self,
        _name: &str,
        _kind: FileType,
        _perms: Permissions,
    ) -> Result<Arc<Mutex<Inode>>, FileError> {
        let guard = self.inner.lock();
        if guard.read_only {
            return Err(FileError::ReadOnlyFilesystem);
        }
        drop(guard);
        // TODO: implement actual create support
        Err(FileError::ReadOnlyFilesystem)
    }

    fn read_dir(&mut self) -> Result<Vec<DirEntry>, FileError> {
        let dentries = self.get_dentries()?;

        let mut vec = Vec::new();

        for i in dentries {
            let ino = i.inode as u64;
            let inode_offset = self
                .inner
                .lock()
                .get_block_offset_for_inode_num(ino as u32)?;

            let mut guard = self.inner.lock();
            let mut buf = vec![0u8; guard.superblock.inode_size as usize];

            guard
                .device
                .seek(SeekFrom::Start(inode_offset))
                .map_err(|_| FileError::SeekError)?;

            guard
                .device
                .read_exact(&mut buf)
                .map_err(|_| FileError::ReadError)?;

            let inode = Ext2Inode::deserialize(&buf)
                .ok_or(FileError::Other("Error Parsing Inode".into()))?;

            let ops: Box<dyn InodeOps + Send + Sync> = match inode.i_mode & 0xF000 {
                0x4000 => Box::new(ExtDirInodeOps {
                    inner: self.inner.clone(),
                    ext2inode: inode.clone(),
                }),

                0x8000 => Box::new(ExtFileInodeOps {
                    inner: self.inner.clone(),
                    ext2inode: inode.clone(),
                    inode_num: i.inode,
                }),

                _ => return Err(FileError::Other("Unsupported file type".into())),
            };

            let ino = Arc::new(Mutex::new(convert_ext2_inode_to_vfs_inode(
                inode.clone(),
                i.inode as u64,
                ops,
            )));
            vec.push(DirEntry {
                name: i.name,
                inode: ino,
            })
        }

        Ok(vec)
    }
}

struct ExtFileInodeOps<D: BlockDevice + 'static> {
    inner: Arc<Mutex<Ext2FSInner<D>>>,
    ext2inode: Ext2Inode,
    inode_num: u32,
}

impl<D: BlockDevice + 'static> ExtFileInodeOps<D> {
    /// Reads a block of u32 pointers from the device.
    fn read_block_u32_array(guard: &mut Ext2FSInner<D>, block: u32) -> Result<Vec<u32>, FileError> {
        let block_size = guard.superblock.block_size() as u64;
        let offset = guard.partition_offset + block as u64 * block_size;

        let mut buf = vec![0u8; block_size as usize];

        guard
            .device
            .seek(SeekFrom::Start(offset))
            .map_err(|_| FileError::SeekError)?;

        guard
            .device
            .read_exact(&mut buf)
            .map_err(|_| FileError::ReadError)?;

        let mut ptrs = Vec::new();

        for chunk in buf.chunks_exact(4) {
            let val = u32::from_le_bytes([chunk[0], chunk[1], chunk[2], chunk[3]]);
            ptrs.push(val);
        }

        Ok(ptrs)
    }

    /// Write a block of u32 pointers to the device.
    fn write_block_u32_array(
        guard: &mut Ext2FSInner<D>,
        block: u32,
        ptrs: &[u32],
    ) -> Result<(), FileError> {
        let block_size = guard.superblock.block_size() as usize;
        let mut buf = vec![0u8; block_size];

        for (i, &ptr) in ptrs.iter().enumerate() {
            if i * 4 + 4 > block_size {
                break;
            }
            buf[i * 4..(i + 1) * 4].copy_from_slice(&ptr.to_le_bytes());
        }

        guard.write_block(block, &buf)
    }

    /// Collects all data block numbers for this inode (direct + indirect).
    fn collect_data_blocks(&self, guard: &mut Ext2FSInner<D>) -> Result<Vec<u32>, FileError> {
        let mut blocks = Vec::new();

        // Direct blocks (0-11)
        for &b in &self.ext2inode.i_block[0..12] {
            blocks.push(b);
        }

        // Singly indirect (12)
        if self.ext2inode.i_block[12] != 0 {
            let ptrs = Self::read_block_u32_array(guard, self.ext2inode.i_block[12])?;
            blocks.extend(ptrs);
        }

        // Doubly indirect (13)
        if self.ext2inode.i_block[13] != 0 {
            let l1 = Self::read_block_u32_array(guard, self.ext2inode.i_block[13])?;

            for b in l1 {
                if b != 0 {
                    let l2 = Self::read_block_u32_array(guard, b)?;
                    blocks.extend(l2);
                }
            }
        }

        // Triply indirect (14)
        if self.ext2inode.i_block[14] != 0 {
            let l1 = Self::read_block_u32_array(guard, self.ext2inode.i_block[14])?;

            for b1 in l1 {
                if b1 != 0 {
                    let l2 = Self::read_block_u32_array(guard, b1)?;

                    for b2 in l2 {
                        if b2 != 0 {
                            let l3 = Self::read_block_u32_array(guard, b2)?;
                            blocks.extend(l3);
                        }
                    }
                }
            }
        }

        Ok(blocks)
    }

    /// Get or allocate the block at the given logical block index.
    fn get_or_alloc_block(
        ext2inode: &mut Ext2Inode,
        guard: &mut Ext2FSInner<D>,
        block_index: usize,
    ) -> Result<u32, FileError> {
        let ptrs_per_block = guard.superblock.block_size() as usize / 4;

        // Direct blocks (0-11)
        if block_index < 12 {
            if ext2inode.i_block[block_index] == 0 {
                let new_block = guard.allocate_block()?;
                ext2inode.i_block[block_index] = new_block;
                ext2inode.i_blocks += guard.superblock.block_size() / 512;
            }
            return Ok(ext2inode.i_block[block_index]);
        }

        // Singly indirect (12)
        let index = block_index - 12;
        if index < ptrs_per_block {
            if ext2inode.i_block[12] == 0 {
                let new_block = guard.allocate_block()?;
                ext2inode.i_block[12] = new_block;
                ext2inode.i_blocks += guard.superblock.block_size() / 512;
                // Zero out the new indirect block
                let zeros = vec![0u8; guard.superblock.block_size() as usize];
                guard.write_block(new_block, &zeros)?;
            }

            let mut ptrs = Self::read_block_u32_array(guard, ext2inode.i_block[12])?;
            while ptrs.len() <= index {
                ptrs.push(0);
            }

            if ptrs[index] == 0 {
                let new_block = guard.allocate_block()?;
                ptrs[index] = new_block;
                ext2inode.i_blocks += guard.superblock.block_size() / 512;
                Self::write_block_u32_array(guard, ext2inode.i_block[12], &ptrs)?;
            }
            return Ok(ptrs[index]);
        }

        // Doubly indirect (13)
        let index = index - ptrs_per_block;
        if index < ptrs_per_block * ptrs_per_block {
            if ext2inode.i_block[13] == 0 {
                let new_block = guard.allocate_block()?;
                ext2inode.i_block[13] = new_block;
                ext2inode.i_blocks += guard.superblock.block_size() / 512;
                let zeros = vec![0u8; guard.superblock.block_size() as usize];
                guard.write_block(new_block, &zeros)?;
            }

            let l1_index = index / ptrs_per_block;
            let l2_index = index % ptrs_per_block;

            let mut l1_ptrs = Self::read_block_u32_array(guard, ext2inode.i_block[13])?;
            while l1_ptrs.len() <= l1_index {
                l1_ptrs.push(0);
            }

            if l1_ptrs[l1_index] == 0 {
                let new_block = guard.allocate_block()?;
                l1_ptrs[l1_index] = new_block;
                ext2inode.i_blocks += guard.superblock.block_size() / 512;
                let zeros = vec![0u8; guard.superblock.block_size() as usize];
                guard.write_block(new_block, &zeros)?;
                Self::write_block_u32_array(guard, ext2inode.i_block[13], &l1_ptrs)?;
            }

            let mut l2_ptrs = Self::read_block_u32_array(guard, l1_ptrs[l1_index])?;
            while l2_ptrs.len() <= l2_index {
                l2_ptrs.push(0);
            }

            if l2_ptrs[l2_index] == 0 {
                let new_block = guard.allocate_block()?;
                l2_ptrs[l2_index] = new_block;
                ext2inode.i_blocks += guard.superblock.block_size() / 512;
                Self::write_block_u32_array(guard, l1_ptrs[l1_index], &l2_ptrs)?;
            }
            return Ok(l2_ptrs[l2_index]);
        }

        // Triply indirect not implemented for simplicity
        Err(FileError::Other(
            "File too large (triply indirect not supported)".into(),
        ))
    }

    /// Sync the inode metadata to disk, and update superblock write time.
    fn sync_inode(
        ext2inode: &Ext2Inode,
        inode_num: u32,
        guard: &mut Ext2FSInner<D>,
    ) -> Result<(), FileError> {
        let inode_offset = guard.get_block_offset_for_inode_num(inode_num)?;

        guard
            .device
            .seek(SeekFrom::Start(inode_offset))
            .map_err(|_| FileError::SeekError)?;

        let buf = ext2inode.serialize();
        guard
            .device
            .write(&buf)
            .map_err(|_| FileError::WriteError)?;

        // Sync superblock with updated write time
        guard.sync_superblock()?;

        Ok(())
    }
}

impl<D: BlockDevice + 'static> InodeOps for ExtFileInodeOps<D> {
    fn read(&mut self, offset: u64, buf: &mut [u8]) -> Result<usize, FileError> {
        let file_size = self.ext2inode.i_size as u64;

        // Check if we're reading past the end of the file
        if offset >= file_size {
            return Ok(0);
        }

        let mut guard = self.inner.lock();
        let block_size = guard.superblock.block_size() as u64;

        // Calculate how much we can actually read
        let bytes_to_read = core::cmp::min(buf.len() as u64, file_size - offset) as usize;

        if bytes_to_read == 0 {
            return Ok(0);
        }

        // Collect all data blocks for this file
        let data_blocks = self.collect_data_blocks(&mut guard)?;

        let mut bytes_read = 0usize;
        let mut current_offset = offset;

        while bytes_read < bytes_to_read {
            let block_index = (current_offset / block_size) as usize;
            let offset_in_block = (current_offset % block_size) as usize;

            // Check if we've run out of blocks
            if block_index >= data_blocks.len() {
                break;
            }

            let block_num = data_blocks[block_index];
            if block_num == 0 {
                // Sparse file: fill with zeros
                let bytes_from_block = core::cmp::min(
                    block_size as usize - offset_in_block,
                    bytes_to_read - bytes_read,
                );
                buf[bytes_read..bytes_read + bytes_from_block].fill(0);
                bytes_read += bytes_from_block;
                current_offset += bytes_from_block as u64;
                continue;
            }

            // Read the block from disk
            let block_offset = guard.partition_offset + block_num as u64 * block_size;

            guard
                .device
                .seek(SeekFrom::Start(block_offset + offset_in_block as u64))
                .map_err(|_| FileError::SeekError)?;

            // Calculate how many bytes to read from this block
            let bytes_from_block = core::cmp::min(
                block_size as usize - offset_in_block,
                bytes_to_read - bytes_read,
            );

            guard
                .device
                .read_exact(&mut buf[bytes_read..bytes_read + bytes_from_block])
                .map_err(|_| FileError::ReadError)?;

            bytes_read += bytes_from_block;
            current_offset += bytes_from_block as u64;
        }

        Ok(bytes_read)
    }

    fn write(&mut self, offset: u64, buf: &[u8]) -> Result<usize, FileError> {
        let mut guard = self.inner.lock();
        if guard.read_only {
            return Err(FileError::ReadOnlyFilesystem);
        }

        if buf.is_empty() {
            return Ok(0);
        }

        let block_size = guard.superblock.block_size() as u64;
        let mut bytes_written = 0usize;
        let mut current_offset = offset;

        while bytes_written < buf.len() {
            let block_index = (current_offset / block_size) as usize;
            let offset_in_block = (current_offset % block_size) as usize;

            // Get or allocate the block
            let block_num = Self::get_or_alloc_block(&mut self.ext2inode, &mut guard, block_index)?;

            // Read the existing block for partial writes
            let mut block_buf = vec![0u8; block_size as usize];
            if offset_in_block != 0 || buf.len() - bytes_written < block_size as usize {
                guard.read_block(block_num, &mut block_buf)?;
            }

            // Calculate how many bytes to write to this block
            let bytes_to_block = core::cmp::min(
                block_size as usize - offset_in_block,
                buf.len() - bytes_written,
            );

            // Copy data into the block buffer
            block_buf[offset_in_block..offset_in_block + bytes_to_block]
                .copy_from_slice(&buf[bytes_written..bytes_written + bytes_to_block]);

            // Write the block back to disk
            guard.write_block(block_num, &block_buf)?;

            bytes_written += bytes_to_block;
            current_offset += bytes_to_block as u64;
        }

        // Update file size if we wrote past the end
        let new_end = offset + bytes_written as u64;
        if new_end > self.ext2inode.i_size as u64 {
            self.ext2inode.i_size = new_end as u32;
        }

        // Sync inode metadata
        Self::sync_inode(&self.ext2inode, self.inode_num, &mut guard)?;

        Ok(bytes_written)
    }

    fn unlink(&mut self, _name: &str) -> Result<(), FileError> {
        Err(FileError::NotADirectory)
    }

    fn truncate(&mut self, size: u64) -> Result<(), FileError> {
        let mut guard = self.inner.lock();
        if guard.read_only {
            return Err(FileError::ReadOnlyFilesystem);
        }

        let block_size = guard.superblock.block_size() as u64;
        let current_size = self.ext2inode.i_size as u64;

        if size >= current_size {
            // Extending the file - just update size, blocks allocated lazily on write
            self.ext2inode.i_size = size as u32;
            Self::sync_inode(&self.ext2inode, self.inode_num, &mut guard)?;
            return Ok(());
        }

        // Shrinking the file - free blocks beyond new size
        let new_block_count = (size + block_size - 1) / block_size;
        let data_blocks = self.collect_data_blocks(&mut guard)?;

        for (i, &block) in data_blocks.iter().enumerate() {
            if i as u64 >= new_block_count && block != 0 {
                guard.free_block(block)?;
                self.ext2inode.i_blocks -= (block_size / 512) as u32;
            }
        }

        // Update direct block pointers
        for i in new_block_count as usize..12 {
            self.ext2inode.i_block[i] = 0;
        }

        // Handle indirect blocks if needed
        let ptrs_per_block = block_size as usize / 4;
        if new_block_count <= 12 {
            // Free singly indirect
            if self.ext2inode.i_block[12] != 0 {
                guard.free_block(self.ext2inode.i_block[12])?;
                self.ext2inode.i_block[12] = 0;
            }
            // Free doubly indirect
            if self.ext2inode.i_block[13] != 0 {
                let l1 = Self::read_block_u32_array(&mut guard, self.ext2inode.i_block[13])?;
                for b in l1 {
                    if b != 0 {
                        guard.free_block(b)?;
                    }
                }
                guard.free_block(self.ext2inode.i_block[13])?;
                self.ext2inode.i_block[13] = 0;
            }
        } else if new_block_count as usize <= 12 + ptrs_per_block {
            // Free doubly indirect
            if self.ext2inode.i_block[13] != 0 {
                let l1 = Self::read_block_u32_array(&mut guard, self.ext2inode.i_block[13])?;
                for b in l1 {
                    if b != 0 {
                        guard.free_block(b)?;
                    }
                }
                guard.free_block(self.ext2inode.i_block[13])?;
                self.ext2inode.i_block[13] = 0;
            }
        }

        self.ext2inode.i_size = size as u32;
        Self::sync_inode(&self.ext2inode, self.inode_num, &mut guard)?;

        Ok(())
    }

    fn sync(&mut self) -> Result<(), FileError> {
        let mut guard = self.inner.lock();
        Self::sync_inode(&self.ext2inode, self.inode_num, &mut guard)
    }

    fn lookup(&mut self, _name: &str) -> Result<Arc<Mutex<Inode>>, FileError> {
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
