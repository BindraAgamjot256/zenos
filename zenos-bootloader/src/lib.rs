/*!
An experimental x86_64 bootloader that runs on UEFI systems.
*/

#![warn(missing_docs)]

extern crate alloc;

#[cfg(feature = "uefi")]
mod gpt;

mod ext;
mod fat;
mod file_data_source;

use std::{
    borrow::Cow,
    collections::BTreeMap,
    path::{Path, PathBuf},
};

use anyhow::Context;

use tempfile::NamedTempFile;

use crate::file_data_source::FileDataSource;
pub use bootloader_boot_config::BootConfig;

const KERNEL_FILE_NAME: &str = "zenos_kernel";
const RAMDISK_FILE_NAME: &str = "initrd";
const BOOT_CONFIG_FILE_NAME: &str = "boot_config";

#[cfg(feature = "uefi")]
const UEFI_BOOTLOADER: &[u8] = include_bytes!(env!("UEFI_BOOTLOADER_PATH"));

/// Allows creating disk images for a specified set of files.
///
/// It can currently create `GPT` (UEFI), and `TFTP` (UEFI) images.
pub struct DiskImageBuilder {
    /// Files for the boot partition (FAT): kernel, ramdisk
    boot_files: BTreeMap<Cow<'static, str>, FileDataSource>,
    /// Files for the data partition (ext2): everything added via set_file()
    data_files: BTreeMap<Cow<'static, str>, FileDataSource>,
}

impl DiskImageBuilder {
    /// Create a new instance of DiskImageBuilder, with the specified kernel.
    pub fn new(kernel: PathBuf) -> Self {
        let mut obj = Self::empty();
        obj.set_kernel(kernel);
        obj
    }

    /// Create a new, empty instance of DiskImageBuilder
    pub fn empty() -> Self {
        Self {
            boot_files: BTreeMap::new(),
            data_files: BTreeMap::new(),
        }
    }

    /// Add or replace a kernel to be included in the final image.
    pub fn set_kernel(&mut self, path: PathBuf) -> &mut Self {
        self.boot_files
            .insert(KERNEL_FILE_NAME.into(), FileDataSource::File(path));
        self
    }

    /// Set the boot configuration file to be included in the final image.
    pub fn set_boot_cfg(&mut self, path: PathBuf) -> &mut Self {
        self.boot_files
            .insert(BOOT_CONFIG_FILE_NAME.into(), FileDataSource::File(path));
        self
    }

    /// Add or replace a ramdisk to be included in the final image.
    pub fn set_ramdisk(&mut self, path: PathBuf) -> &mut Self {
        self.boot_files
            .insert(RAMDISK_FILE_NAME.into(), FileDataSource::File(path));
        self
    }

    /// Add a file with the specified source file to the disk image (ext2 data partition).
    ///
    /// Note that the bootloader only loads the kernel and ramdisk files into memory on boot.
    /// Other files need to be loaded manually by the kernel from the ext2 data partition.
    pub fn set_file(&mut self, destination: String, file_path: PathBuf) -> &mut Self {
        self.data_files
            .insert(destination.into(), FileDataSource::File(file_path));
        self
    }

    #[cfg(feature = "uefi")]
    /// Create a GPT disk image for booting on UEFI systems.
    ///
    /// Creates a FAT partition with kernel/ramdisk/bootloader, and if any data files
    /// were added via `set_file()`, creates a data partition containing them.
    ///
    /// If `use_fat` is true, uses FAT for the data partition; otherwise uses ext2.
    /// For ext2 creation: tries genext2fs first (macOS), falls back to mkfs.ext2,
    /// or panics if neither is available.
    pub fn create_uefi_image(&self, image_path: &Path, use_fat: bool) -> anyhow::Result<()> {
        const UEFI_BOOT_FILENAME: &str = "efi/boot/bootx64.efi";

        let mut internal_files = BTreeMap::new();
        internal_files.insert(UEFI_BOOT_FILENAME, FileDataSource::Bytes(UEFI_BOOTLOADER));
        let fat_partition = self
            .create_fat_filesystem_image(internal_files)
            .context("failed to create FAT partition")?;

        // Create data partition if there are data files
        let data_partition = if !self.data_files.is_empty() {
            Some(self.create_data_filesystem_image(use_fat)?)
        } else {
            None
        };

        gpt::create_gpt_disk_with_partitions(
            fat_partition.path(),
            data_partition.as_ref().map(|f| f.path()),
            image_path,
        )
        .context("failed to create UEFI GPT disk image")?;

        fat_partition
            .close()
            .context("failed to delete FAT partition after disk image creation")?;
        if let Some(data) = data_partition {
            data.close()
                .context("failed to delete data partition after disk image creation")?;
        }

        Ok(())
    }

    fn create_fat_filesystem_image(
        &self,
        internal_files: BTreeMap<&str, FileDataSource>,
    ) -> anyhow::Result<NamedTempFile> {
        let mut local_map: BTreeMap<&str, _> = BTreeMap::new();

        for (name, source) in &self.boot_files {
            local_map.insert(name.as_ref(), source);
        }

        for k in &internal_files {
            if local_map.insert(k.0, k.1).is_some() {
                return Err(anyhow::Error::msg(format!(
                    "Attempted to overwrite internal file: {}",
                    k.0
                )));
            }
        }

        let out_file = NamedTempFile::new().context("failed to create temp file")?;
        fat::create_fat_filesystem(local_map, out_file.path())
            .context("failed to create FAT filesystem")?;

        Ok(out_file)
    }

    fn create_ext2_filesystem_image(&self) -> anyhow::Result<NamedTempFile> {
        let mut local_map: BTreeMap<&str, _> = BTreeMap::new();

        for (name, source) in &self.data_files {
            local_map.insert(name.as_ref(), source);
        }

        let out_file = NamedTempFile::new().context("failed to create temp file")?;
        ext::create_ext2_filesystem(local_map, out_file.path())
            .context("failed to create ext2 filesystem")?;

        Ok(out_file)
    }

    fn create_fat_data_filesystem_image(&self) -> anyhow::Result<NamedTempFile> {
        let mut local_map: BTreeMap<&str, _> = BTreeMap::new();

        for (name, source) in &self.data_files {
            local_map.insert(name.as_ref(), source);
        }

        let out_file = NamedTempFile::new().context("failed to create temp file")?;
        fat::create_fat_filesystem(local_map, out_file.path())
            .context("failed to create FAT data filesystem")?;

        Ok(out_file)
    }

    /// Create the data partition filesystem image.
    /// If `use_fat` is true, creates a FAT filesystem; otherwise creates ext2.
    fn create_data_filesystem_image(&self, use_fat: bool) -> anyhow::Result<NamedTempFile> {
        if use_fat {
            self.create_fat_data_filesystem_image()
                .context("failed to create FAT data partition")
        } else {
            self.create_ext2_filesystem_image()
                .context("failed to create ext2 data partition")
        }
    }
}
