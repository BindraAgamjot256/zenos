#![allow(unexpected_cfgs)]

use std::path::PathBuf;
use std::process::Command;

fn main() {
    uefi_main();
}

#[cfg(feature = "uefi")]
fn uefi_main() {
    let uefi_path = build_uefi_bootloader();
    println!(
        "cargo:rustc-env=UEFI_BOOTLOADER_PATH={}",
        uefi_path.display()
    );
}

#[cfg(not(feature = "uefi"))]
fn uefi_main() {}

#[cfg(not(docsrs_dummy_build))]
#[cfg(feature = "uefi")]
fn build_uefi_bootloader() -> PathBuf {
    let cargo = std::env::var("CARGO").unwrap_or_else(|_| "cargo".into());
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));

    // 1. Create an isolated target directory for the inner cargo process
    let inner_target_dir = manifest_dir.join("bootloader_build");

    let mut cmd = Command::new(cargo);
    cmd.arg("build");
    cmd.arg("-p").arg("bootloader-x86_64-uefi");
    cmd.arg("--target").arg("x86_64-unknown-uefi");
    cmd.arg("-Zbuild-std=core,alloc,compiler_builtins")
        .arg("-Zbuild-std-features=compiler-builtins-mem");
    cmd.arg("--locked");

    // 2. Break the deadlock by directing the build to your isolated directory
    cmd.env("CARGO_TARGET_DIR", &inner_target_dir);

    // 3. Clear outer Cargo flags that force dependency inheritance loops
    cmd.env_remove("RUSTFLAGS");
    cmd.env_remove("CARGO_ENCODED_RUSTFLAGS");

    println!("cargo:rerun-if-changed=uefi");
    println!("cargo:rerun-if-changed=common");
    println!("cargo:rerun-if-changed={}", inner_target_dir.display());
    // Explicitly set the workspace directory context to avoid locking root
    cmd.current_dir(&manifest_dir);

    let status = cmd
        .status()
        .expect("failed to run cargo for uefi bootloader");

    if status.success() {
        // 4. Calculate the real path where Cargo puts custom target cross-compilations
        // Adjust "debug" to "release" if your outer script forces profile changes
        let profile = std::env::var("PROFILE").unwrap_or_else(|_| "debug".into());
        let path = inner_target_dir
            .join("x86_64-unknown-uefi")
            .join(profile)
            .join("bootloader-x86_64-uefi.efi");

        assert!(
            path.exists(),
            "uefi bootloader executable does not exist at expected path: {}",
            path.display()
        );

        println!("build successful, uefi bootloader at {}", path.display());
        path
    } else {
        println!("build failed with status: {}", status);
        panic!("failed to build uefi bootloader");
    }
}

// dummy implementation because docsrs builds have no network access.
// This will put an empty file in out_dir and return its path.
#[cfg(docsrs_dummy_build)]
#[cfg(feature = "uefi")]
fn build_uefi_bootloader() -> PathBuf {
    use std::fs::File;

    let out_dir = PathBuf::from(std::env::var("OUT_DIR").unwrap());
    let path = out_dir.join("bootloader-dummy-bootloader-uefi");

    if File::create(&path).is_err() {
        panic!("Failed to create dummy uefi bootloader");
    }
    assert!(
        path.exists(),
        "uefi bootloader fake file does not exist after file creation"
    );

    path
}
