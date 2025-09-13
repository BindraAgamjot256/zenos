use std::{env, path::PathBuf};

fn main() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    let kernel_path_str = env::var_os("CARGO_BIN_FILE_ZENOS_KERNEL_zenos-kernel")
        .or_else(|| env::var_os("CARGO_BIN_FILE_ZENOS_KERNEL_ZENOS_KERNEL"))
        .expect("Kernel binary environment variable not found. Ensure 'zenos-kernel' is configured correctly in Cargo.toml.");
    let kernel_path_str = kernel_path_str.to_string_lossy().to_string();

    println!("cargo:rustc-env=KERNEL_PATH={kernel_path_str}");
    let kernel_path = PathBuf::from(&kernel_path_str);
    let stripped_kernel_path = kernel_path;

    let uefi_out_path = out_dir.join("uefi.img");

    zenos_bootloader::UefiBoot::new(&stripped_kernel_path)
        .create_disk_image(&uefi_out_path)
        .expect("Failed to create UEFI disk image");

    println!("cargo:rustc-env=UEFI_PATH={}", uefi_out_path.display());
    println!(
        "cargo:rustc-env=KERNEL_PATH={}",
        stripped_kernel_path.display()
    );
}
