/*!
An experimental x86_64 bootloader that runs on UEFI systems.
*/

#![warn(missing_docs)]

extern crate alloc;

#[cfg(feature = "uefi")]
mod gpt;

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

const KERNEL_FILE_NAME: &str = "kernel-x86_64";
const RAMDISK_FILE_NAME: &str = "ramdisk";

#[cfg(feature = "uefi")]
const UEFI_BOOTLOADER: &[u8] = include_bytes!(env!("UEFI_BOOTLOADER_PATH"));

/// Allows creating disk images for a specified set of files.
///
/// It can currently create `GPT` (UEFI), and `TFTP` (UEFI) images.
pub struct DiskImageBuilder {
    files: BTreeMap<Cow<'static, str>, FileDataSource>,
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
            files: BTreeMap::new(),
        }
    }

    /// Add or replace a kernel to be included in the final image.
    pub fn set_kernel(&mut self, path: PathBuf) -> &mut Self {
        self.set_file_source(KERNEL_FILE_NAME.into(), FileDataSource::File(path))
    }

    /// Add or replace a ramdisk to be included in the final image.
    pub fn set_ramdisk(&mut self, path: PathBuf) -> &mut Self {
        self.set_file_source(RAMDISK_FILE_NAME.into(), FileDataSource::File(path))
    }

    /// Add a file with the specified source file to the disk image
    ///
    /// Note that the bootloader only loads the kernel and ramdisk files into memory on boot.
    /// Other files need to be loaded manually by the kernel.
    pub fn set_file(&mut self, destination: String, file_path: PathBuf) -> &mut Self {
        self.set_file_source(destination.into(), FileDataSource::File(file_path))
    }

    #[cfg(feature = "uefi")]
    /// Create a GPT disk image for booting on UEFI systems.
    pub fn create_uefi_image(&self, image_path: &Path) -> anyhow::Result<()> {
        const UEFI_BOOT_FILENAME: &str = "efi/boot/bootx64.efi";

        let mut internal_files = BTreeMap::new();
        internal_files.insert(UEFI_BOOT_FILENAME, FileDataSource::Bytes(UEFI_BOOTLOADER));
        let fat_partition = self
            .create_fat_filesystem_image(internal_files)
            .context("failed to create FAT partition")?;
        gpt::create_gpt_disk(fat_partition.path(), image_path)
            .context("failed to create UEFI GPT disk image")?;
        fat_partition
            .close()
            .context("failed to delete FAT partition after disk image creation")?;

        Ok(())
    }

    /// Add a file source to the disk image
    fn set_file_source(
        &mut self,
        destination: Cow<'static, str>,
        source: FileDataSource,
    ) -> &mut Self {
        self.files.insert(destination, source);
        self
    }

    fn create_fat_filesystem_image(
        &self,
        internal_files: BTreeMap<&str, FileDataSource>,
    ) -> anyhow::Result<NamedTempFile> {
        let mut local_map: BTreeMap<&str, _> = BTreeMap::new();

        for (name, source) in &self.files {
            local_map.insert(name, source);
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
}
