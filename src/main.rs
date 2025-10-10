use clap::Parser;
use std::path::{Path, PathBuf};

#[derive(Parser, Clone, Copy)]
struct Args {
    /// Enable color output
    #[arg(long, short = 'c', default_value = "false")]
    color: bool,

    /// cCheck the build
    #[arg(long, short = 'C', default_value = "false")]
    check: bool,
}

fn main() {
    // read env variables that were set in the build script
    let args = Args::parse();

    if args.check {
        build_kernel(args);
        return;
    }

    let binding = build_kernel(args);
    let kernel_path = binding.as_path();
    let binding = disk_img_builder(kernel_path);
    let uefi_path = binding.as_path();

    println!("uefi path: {}", uefi_path.to_str().unwrap());
    println!("kernel path: {}", kernel_path.to_str().unwrap());

    let mut cmd = std::process::Command::new("qemu-system-x86_64");
    cmd.arg("-machine").arg("q35");
    cmd.arg("-bios").arg(ovmf_prebuilt::ovmf_pure_efi());
    cmd.arg("-m").arg("2048M");
    cmd.arg("-smp").arg("2");
    cmd.arg("-serial").arg("stdio");
    cmd.arg("-no-reboot").arg("-no-shutdown");

    // AHCI controller (no bus specified)
    cmd.arg("-device").arg("ahci,id=ahci");

    // Disk attached to AHCI bus
    cmd.arg("-drive").arg(format!(
        "id=disk0,if=none,file={},format=raw",
        uefi_path.to_str().unwrap()
    ));
    cmd.arg("-device").arg("ide-hd,drive=disk0,bus=ahci.0");

    print!("running command: {cmd:#?}");
    let mut child = cmd.spawn().unwrap();
    child.wait().expect("failed to wait on child");
}

fn build_kernel(args: Args) -> PathBuf {
    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("+nightly");
    cmd.arg("build");
    cmd.arg("-p").arg("zenos-kernel");
    #[cfg(not(debug_assertions))]
    cmd.arg("--release");

    cmd.arg("--target=x86_64-unknown-zenos.json");
    cmd.arg("--bin=zenos-kernel");
    cmd.arg("-Z").arg("build-std=core,alloc");
    cmd.arg("-Z")
        .arg("build-std-features=compiler-builtins-mem");

    // cmd.arg("--");

    if args.color {
        cmd.arg("-F").arg("color");
    }

    let status = cmd.status().expect("failed to build kernel");
    if !status.success() {
        panic!("kernel build failed");
    }

    // Construct the path to the compiled kernel binary
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };

    let kernel_path = Path::new("./target/x86_64-unknown-zenos")
        .join(profile)
        .join("zenos-kernel");

    kernel_path
}

fn disk_img_builder(kernel_path: &Path) -> PathBuf {
    let mut builder = zenos_bootloader::DiskImageBuilder::new(PathBuf::from(kernel_path));
    let uefi_out_path = PathBuf::from("uefi.img");
    let iso_dir = Path::new("iso");
    if iso_dir.exists() && iso_dir.is_dir() {
        add_files_recursively(&mut builder, iso_dir, iso_dir);
    } else {
        panic!("iso directory does not exist!");
    }

    builder
        .create_uefi_image(&uefi_out_path)
        .expect("UEFI image could not be created");
    println!("uefi path: {}", uefi_out_path.display());
    uefi_out_path
}

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
