use anyhow::Context;
use std::{
    fs::{self, File},
    io::{self, Seek},
    path::Path,
};

pub fn create_gpt_disk_with_partitions(
    fat_image: &Path,
    ext2_image: Option<&Path>,
    out_gpt_path: &Path,
) -> anyhow::Result<()> {
    // create new file
    let mut disk = fs::OpenOptions::new()
        .create(true)
        .truncate(true)
        .read(true)
        .write(true)
        .open(out_gpt_path)
        .with_context(|| format!("failed to create GPT file at `{}`", out_gpt_path.display()))?;

    // calculate total disk size
    let fat_partition_size: u64 = fs::metadata(fat_image)
        .context("failed to read metadata of fat image")?
        .len();
    let ext2_partition_size: u64 = ext2_image
        .map(|p| fs::metadata(p).map(|m| m.len()))
        .transpose()
        .context("failed to read metadata of ext2 image")?
        .unwrap_or(0);

    let disk_size = fat_partition_size + ext2_partition_size + 1024 * 64; // for GPT headers
    disk.set_len(disk_size)
        .context("failed to set GPT image file length")?;

    // create a protective MBR at LBA0 so that disk is not considered
    // unformatted on BIOS systems
    let mbr = gpt::mbr::ProtectiveMBR::with_lb_size(
        u32::try_from((disk_size / 512) - 1).unwrap_or(0xFF_FF_FF_FF),
    );
    mbr.overwrite_lba0(&mut disk)
        .context("failed to write protective MBR")?;

    // create new GPT structure
    let block_size = gpt::disk::LogicalBlockSize::Lb512;
    let mut gpt = gpt::GptConfig::new()
        .writable(true)
        .initialized(false)
        .logical_block_size(block_size)
        .create_from_device(Box::new(&mut disk), None)
        .context("failed to create GPT structure in file")?;
    gpt.update_partitions(Default::default())
        .context("failed to update GPT partitions")?;

    // add new EFI system partition and get its byte offset in the file
    let fat_partition_id = gpt
        .add_partition(
            "boot",
            fat_partition_size,
            gpt::partition_types::EFI,
            0,
            None,
        )
        .context("failed to add boot EFI partition")?;
    let fat_partition = gpt
        .partitions()
        .get(&fat_partition_id)
        .context("failed to open boot partition after creation")?;
    let fat_start_offset = fat_partition
        .bytes_start(block_size)
        .context("failed to get start offset of boot partition")?;

    // add ext2 data partition if provided
    let ext2_start_offset = if let Some(_ext2_path) = ext2_image {
        let ext2_partition_id = gpt
            .add_partition(
                "data",
                ext2_partition_size,
                gpt::partition_types::LINUX_FS,
                0,
                None,
            )
            .context("failed to add data ext2 partition")?;
        let ext2_partition = gpt
            .partitions()
            .get(&ext2_partition_id)
            .context("failed to open data partition after creation")?;
        Some(
            ext2_partition
                .bytes_start(block_size)
                .context("failed to get start offset of data partition")?,
        )
    } else {
        None
    };

    // close the GPT structure and write out changes
    gpt.write().context("failed to write out GPT changes")?;

    // place the FAT filesystem in the newly created partition
    disk.seek(io::SeekFrom::Start(fat_start_offset))
        .context("failed to seek to FAT partition start offset")?;
    io::copy(
        &mut File::open(fat_image).context("failed to open FAT image")?,
        &mut disk,
    )
    .context("failed to copy FAT image to GPT disk")?;

    // place the ext2 filesystem in the data partition if provided
    if let (Some(ext2_path), Some(ext2_offset)) = (ext2_image, ext2_start_offset) {
        disk.seek(io::SeekFrom::Start(ext2_offset))
            .context("failed to seek to ext2 partition start offset")?;
        io::copy(
            &mut File::open(ext2_path).context("failed to open ext2 image")?,
            &mut disk,
        )
        .context("failed to copy ext2 image to GPT disk")?;
    }

    Ok(())
}
