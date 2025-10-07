use std::{
    env,
    path::{Path, PathBuf},
};

fn main() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    let kernel_path_str = env::var_os("CARGO_BIN_FILE_ZENOS_KERNEL_zenos-kernel")
        .or_else(|| env::var_os("CARGO_BIN_FILE_ZENOS_KERNEL_ZENOS_KERNEL"))
        .expect("Kernel binary environment variable not found. Ensure 'zenos-kernel' is configured correctly in Cargo.toml.");
    let kernel_path_str = kernel_path_str.to_string_lossy().to_string();

    println!("cargo:rustc-env=KERNEL_PATH={kernel_path_str}");

    let kernel_path = PathBuf::from(&kernel_path_str);

    let uefi_out_path = out_dir.join("uefi.img");

    let mut builder = zenos_bootloader::DiskImageBuilder::new(kernel_path.clone());

    let iso_dir = Path::new("iso");
    if iso_dir.exists() && iso_dir.is_dir() {
        add_files_recursively(&mut builder, iso_dir, iso_dir);
    } else {
        panic!("iso directory does not exist!");
    }

    builder
        .create_uefi_image(&uefi_out_path)
        .expect("UEFI image could not be created");

    println!("cargo:rustc-env=UEFI_PATH={}", uefi_out_path.display());
}

// Recursive function to add all files, keeping paths relative to iso_root
fn add_files_recursively(
    builder: &mut zenos_bootloader::DiskImageBuilder,
    dir: &Path,
    iso_root: &Path,
) {
    for entry in std::fs::read_dir(dir).expect("Failed to read directory") {
        let entry = entry.expect("Failed to read entry");
        let path = entry.path();
        if path.is_dir() {
            // recurse into subdirectory
            add_files_recursively(builder, &path, iso_root);
        } else if path.is_file() {
            // compute relative path
            let relative_path = path.strip_prefix(iso_root).unwrap();
            let relative_path_str = relative_path
                .to_str()
                .expect("Failed to convert path to str");
            builder.set_file(relative_path_str.to_string(), path);
        }
    }
}
