use anyhow::Context;
use std::{collections::BTreeMap, fs, path::Path, process::Command};
use tempfile::TempDir;

use crate::file_data_source::FileDataSource;

/// Create an ext2 filesystem image containing the specified files.
///
/// Tries genext2fs first (preferred on macOS), falls back to mkfs.ext2,
/// and panics if neither is available.
pub fn create_ext2_filesystem(
    files: BTreeMap<&str, &FileDataSource>,
    out_path: &Path,
) -> anyhow::Result<()> {
    // Create a temporary directory to stage files
    let temp_dir = TempDir::new().context("failed to create temp directory")?;
    let staging_path = temp_dir.path();

    // Copy all files to the staging directory
    for (target_path_raw, source) in &files {
        let target_path = staging_path.join(target_path_raw);

        // Create parent directories
        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("failed to create directory `{}`", parent.display()))?;
        }

        // Write the file
        source
            .write_to_path(&target_path)
            .with_context(|| format!("failed to write file to `{}`", target_path.display()))?;
    }

    // Calculate needed size (add padding for ext2 overhead)
    let mut needed_size: u64 = 0;
    for source in files.values() {
        needed_size += source.len()?;
    }
    // Add overhead for ext2 metadata and round up to MB
    let size_mb = (needed_size.div_ceil(1024 * 1024) + 4).max(8);

    // Reserve extra inodes for OS use (at least 1024 free inodes)
    let file_count = files.len() as u64;
    let inode_count = (file_count + 1024).max(2048);

    // Try genext2fs first (works on macOS via Homebrew)
    if try_genext2fs(staging_path, out_path, size_mb, inode_count)? {
        return Ok(());
    }

    // Fall back to mkfs.ext2
    if try_mkfs_ext2(staging_path, out_path, size_mb, inode_count)? {
        return Ok(());
    }

    // Neither tool is available
    #[cfg(target_os = "macos")]
    panic!(
        "Neither genext2fs nor mkfs.ext2 is available. \
         Install genext2fs via: brew install genext2fs"
    );

    #[cfg(target_os = "windows")]
    panic!(
        "Neither genext2fs nor mkfs.ext2 is available. \
         Install WSL and run: sudo apt install e2fsprogs"
    );

    #[cfg(target_os = "linux")]
    panic!(
        "Neither genext2fs nor mkfs.ext2 is available. \
         Install e2fsprogs via your package manager (e.g., apt install e2fsprogs)"
    );

    #[cfg(any(target_os = "freebsd", target_os = "openbsd", target_os = "netbsd"))]
    panic!(
        "Neither genext2fs nor mkfs.ext2 is available. \
         Install e2fsprogs via your package manager (e.g., pkg install e2fsprogs)"
    );

    #[cfg(not(any(
        target_os = "macos",
        target_os = "windows",
        target_os = "linux",
        target_os = "freebsd",
        target_os = "openbsd",
        target_os = "netbsd"
    )))]
    panic!("Neither genext2fs nor mkfs.ext2 is available.");
}

/// Try to create ext2 filesystem using genext2fs.
/// Returns Ok(true) if successful, Ok(false) if genext2fs is not available.
fn try_genext2fs(
    staging_path: &Path,
    out_path: &Path,
    size_mb: u64,
    inode_count: u64,
) -> anyhow::Result<bool> {
    let status = Command::new("genext2fs")
        .arg("-b")
        .arg((size_mb * 1024).to_string()) // size in 1K blocks
        .arg("-N")
        .arg(inode_count.to_string())
        .arg("-L")
        .arg("data") // ← volume name
        .arg("-d")
        .arg(staging_path)
        .arg(out_path)
        .status();

    match status {
        Ok(s) if s.success() => Ok(true),
        Ok(s) => Err(anyhow::anyhow!("genext2fs failed with status: {}", s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e).context("failed to run genext2fs"),
    }
}

/// Try to create ext2 filesystem using mkfs.ext2.
/// Returns Ok(true) if successful, Ok(false) if mkfs.ext2 is not available.
fn try_mkfs_ext2(
    staging_path: &Path,
    out_path: &Path,
    size_mb: u64,
    inode_count: u64,
) -> anyhow::Result<bool> {
    // Create an empty file of the required size
    let file = fs::OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(true)
        .open(out_path)
        .context("failed to create output file")?;
    file.set_len(size_mb * 1024 * 1024)
        .context("failed to set file size")?;
    drop(file);

    // Format it as ext2
    let mut cmd = Command::new("mkfs.ext2");

    if cfg!(target_os = "macos") {
        // on macos, mkfs.ext2 is not added to path when installed by homebrew, so we need to specify the full path
        let brew_prefix = Command::new("brew")
            .arg("--prefix")
            .arg("e2fsprogs")
            .output()
            .context("failed to run brew --prefix")?;

        let brew_prefix_str = String::from_utf8(brew_prefix.stdout)
            .context("brew --prefix output is not valid UTF-8")?
            .trim()
            .to_string();
        let brew_prefix = format!("{brew_prefix_str}/sbin/mkfs.ext2");
        cmd = Command::new(brew_prefix);
    }

    let status = cmd
        .arg("-L")
        .arg("data") // ← volume name
        .arg("-N")
        .arg(inode_count.to_string())
        .arg("-d")
        .arg(staging_path)
        .arg(out_path)
        .status();

    match status {
        Ok(s) if s.success() => Ok(true),
        Ok(s) => Err(anyhow::anyhow!("mkfs.ext2 failed with status: {}", s)),
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(e) => Err(e).context("failed to run mkfs.ext2"),
    }
}
