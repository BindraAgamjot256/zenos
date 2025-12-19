extern crate alloc; // only for shutting up cargo check --target x86_64-unknown-zenos.json and co...
use alloc::format;
use clap::Parser;
use core::cfg;
use core::convert::From;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::exit;

#[derive(Parser, Clone, Copy)]
struct Args {
    /// Enable color output
    #[arg(long, short = 'c', default_value = "false")]
    color: bool,

    /// cCheck the build
    #[arg(long, short = 'C', default_value = "false", group = "mode")]
    check: bool,
    #[arg(long, short = 's', default_value = "false", group = "mode")]
    test_stub: bool,
    #[arg(long, short = 'd', default_value = "false", group = "mode")]
    debugger: bool,
    #[arg(long, short = 't', default_value = "false", group = "mode")]
    test: bool,
}

fn main() {
    // read env variables that were set in the build script
    let args = Args::parse();

    if args.check {
        build_kernel(args);
        build_init(args);
        return;
    }

    build_init(args);
    let binding = build_kernel(args);
    let kernel_path = binding.as_path();
    let binding = disk_img_builder(kernel_path);
    let uefi_path = binding.as_path();

    println!("kernel path: {}", kernel_path.to_str().unwrap());

    let mut cmd = std::process::Command::new("qemu-system-x86_64");
    cmd.arg("-machine").arg("q35");
    cmd.arg("-bios").arg(ovmf_prebuilt::ovmf_pure_efi());
    cmd.arg("-m").arg("2048M");
    cmd.arg("-smp").arg("2");
    cmd.arg("-no-reboot")
        .arg("-no-shutdown")
        .arg("-d")
        .arg("cpu_reset");

    // AHCI controller (no bus specified)
    cmd.arg("-device").arg("ahci,id=ahci");

    // Disk attached to AHCI bus
    cmd.arg("-drive").arg(format!(
        "id=disk0,if=none,file={},format=raw",
        uefi_path.to_str().unwrap()
    ));
    cmd.arg("-device").arg("ide-hd,drive=disk0,bus=ahci.0");

    if args.debugger {
        cmd.arg("-s");
        cmd.arg("-S");
        println!("remember to attach the debugger.")
    }
    if args.test {
        cmd.arg("-nographic");
        cmd.arg("-device")
            .arg("isa-debug-exit,iobase=0xf4,iosize=0x04");
    } else {
        cmd.arg("-serial").arg("stdio");
    }

    print!("running command: {cmd:#?}");
    std::io::stdout().flush().unwrap();
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

    if args.test_stub {
        cmd.arg("-F").arg("test_stub");
    }
    if args.test {
        cmd.arg("-F").arg("run-kunittest");
    }
    #[cfg(debug_assertions)]
    cmd.env("RUSTFLAGS", "-Cforce-frame-pointers=yes");
    let status = cmd.status().expect("failed to build kernel");
    if !status.success() {
        eprintln!("kernel exited with status: {}", status);
        exit(0);
    }

    // Construct the path to the compiled kernel binary
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };

    let kernel_path = Path::new("./target/x86_64-unknown-zenos")
        .join(profile)
        .as_path()
        .join("zenos-kernel");

    kernel_path
}

fn build_init(_args: Args) {
    // Build the userland init process and place the resulting ELF into iso/bin/init.elf
    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("+nightly");
    cmd.arg("build");
    cmd.arg("-p").arg("zenos-init");
    #[cfg(not(debug_assertions))]
    cmd.arg("--release");

    cmd.arg("--target=x86_64-unknown-zenos-user.json");
    cmd.arg("--bin=zenos-init");
    cmd.arg("-Z").arg("build-std=core,alloc");
    cmd.arg("-Z")
        .arg("build-std-features=compiler-builtins-mem");

    #[cfg(debug_assertions)]
    cmd.env("RUSTFLAGS", "-Cforce-frame-pointers=yes");

    let status = cmd.status().expect("failed to build init");
    if !status.success() {
        eprintln!("kernel exited with status: {}", status);
        exit(0);
    }

    // Determine the profile directory
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };

    // Path to built init binary
    let init_bin = Path::new("./target/x86_64-unknown-zenos-user")
        .join(profile)
        .as_path()
        .join("zenos-init");

    // Ensure iso/bin exists and copy the file as init.elf
    let binding = Path::new("iso").join("bin");
    let out_dir = binding.as_path();
    std::fs::create_dir_all(out_dir).expect("failed to create iso/bin directory");
    let out_path = out_dir.join("init.elf");
    std::fs::copy(&init_bin, &out_path).expect("failed to copy init.elf into iso/bin");
}

fn disk_img_builder(kernel_path: &Path) -> PathBuf {
    let mut builder = zenos_bootloader::DiskImageBuilder::new(kernel_path.to_path_buf());
    let uefi_out_path = PathBuf::from("uefi.img");
    let iso_dir = Path::new("iso");
    if iso_dir.exists() && iso_dir.is_dir() {
        add_files_recursively(&mut builder, iso_dir, iso_dir);
    } else {
        panic!("iso directory does not exist!");
    }

    builder
        .create_uefi_image(uefi_out_path.as_path())
        .expect("UEFI image could not be created");
    println!("uefi path: {}", uefi_out_path.as_path().display());
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
                .expect("Failed to convert path to str")
                .replace("\\", "/");
            builder.set_file(relative_path_str, path);
        }
    }
}
