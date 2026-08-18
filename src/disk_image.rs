use std::{
    fs,
    path::{Path, PathBuf},
};

use crate::Result;

pub(crate) const DISK_IMAGE: &str = "uefi.img";
const ISO_DIR: &str = "iso";
const SPLASH: &str = "splash.bmp";
const BOOT_CFG: &str = "config";

pub(crate) fn build(kernel: &Path, initrd: Option<&Path>, use_fat: bool) -> Result<PathBuf> {
    println!("[DISK] Creating bootable UEFI image...");

    let mut builder = zenos_bootloader::DiskImageBuilder::new(kernel.to_path_buf());
    builder.set_splash(SPLASH.into());
    builder.set_boot_cfg(BOOT_CFG.into());

    let image = PathBuf::from(DISK_IMAGE);
    let iso_dir = Path::new(ISO_DIR);

    if !iso_dir.is_dir() {
        return Err("Staging directory 'iso/' is missing.".into());
    }

    println!("[DISK] Traversing staging directory: {}", iso_dir.display());

    add_files_recursively(&mut builder, iso_dir, iso_dir)?;

    match initrd {
        Some(initrd) => {
            if !initrd.is_file() {
                return Err(format!(
                    "Initial ramdisk does not exist or is not a file: {}",
                    initrd.display()
                )
                .into());
            }

            println!("[DISK] Using initial ramdisk: {}", initrd.display());

            builder.set_ramdisk(initrd.to_path_buf());
        }
        None => {
            println!("[DISK] No initial ramdisk supplied.");
            println!("[DISK] Initial ramdisk generation is not implemented yet.");
        }
    }

    builder.create_uefi_image(&image, use_fat)?;

    Ok(image)
}

fn add_files_recursively(
    builder: &mut zenos_bootloader::DiskImageBuilder,
    directory: &Path,
    root: &Path,
) -> Result<()> {
    for entry in fs::read_dir(directory)? {
        let path = entry?.path();

        if path.is_dir() {
            add_files_recursively(builder, &path, root)?;
        } else if path.is_file() {
            let relative = path
                .strip_prefix(root)?
                .to_string_lossy()
                .replace('\\', "/");

            println!("[DISK] Mapping: {} -> /{}", path.display(), relative);

            builder.set_file(relative, path);
        }
    }

    Ok(())
}
