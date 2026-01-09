extern crate alloc; // only for shutting up cargo check --target x86_64-unknown-zenos.json and co...
use alloc::format;
use clap::Parser;
use core::cfg;
use core::convert::From;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::exit;

#[derive(Parser, Clone, Copy)]
#[command(author, version, about = "Build and run the Zenos Operating System")]
struct Args {
    /// Enable color output in the kernel
    #[arg(long, short = 'c', default_value = "false")]
    color: bool,

    /// Only check/build components without launching QEMU
    #[arg(long, short = 'C', default_value = "false", group = "mode")]
    check: bool,

    /// Build with the test stub enabled
    #[arg(long, short = 's', default_value = "false", group = "mode")]
    test_stub: bool,

    /// Start QEMU in paused mode and open a GDB stub (port 1234)
    #[arg(long, short = 'd', default_value = "false", group = "mode")]
    debugger: bool,

    /// Run kernel unit tests and exit via isa-debug-exit
    #[arg(long, short = 't', default_value = "false", group = "mode")]
    test: bool,

    /// Build and run the kernel fuzzer (replaces init process)
    #[arg(long, short = 'f', default_value = "false")]
    fuzz: bool,
}

fn main() {
    let args = Args::parse();

    // 1. Component Compilation Phase
    if args.check {
        println!("[CHECK] Verification mode: building components...");
        build_kernel(args);
        build_init(args);
        println!("[CHECK] All components compiled successfully.");
        return;
    }

    println!("[BUILD] Starting full system build...");
    let path = PathBuf::from("./iso/bin");
    if path.exists() {
        std::fs::remove_dir_all(&path).expect("Could not clean iso/bin directory");
    }
    build_init(args);
    build_fuzz(args);

    // 2. Image Construction Phase
    let kernel_binding = build_kernel(args);
    let kernel_path = kernel_binding.as_path();

    println!("[DISK] Creating bootable UEFI image...");
    let image_binding = disk_img_builder(kernel_path);
    let uefi_path = image_binding.as_path();

    println!("[INFO] Kernel binary: {}", kernel_path.display());
    println!("[INFO] UEFI Image:    {}", uefi_path.display());

    // 3. Emulation Phase (QEMU)
    println!("[RUN] Launching QEMU...");
    let mut cmd = std::process::Command::new("qemu-system-x86_64");

    // Machine configuration
    cmd.arg("-machine").arg("q35"); // Modern chipset
    cmd.arg("-bios").arg(ovmf_prebuilt::ovmf_pure_efi()); // Use UEFI firmware
    cmd.arg("-m").arg("512M"); // 512 MiB RAM
    cmd.arg("-smp").arg("2"); // 2 CPU cores

    // Debug/Exit behavior
    cmd.arg("-no-reboot")
        .arg("-no-shutdown")
        .arg("-d")
        .arg("cpu_reset"); // Log resets to help find triple faults

    // Storage: AHCI (SATA) controller configuration
    cmd.arg("-device").arg("ahci,id=ahci");
    cmd.arg("-drive").arg(format!(
        "id=disk0,if=none,file={},format=raw",
        uefi_path.to_str().unwrap()
    ));
    cmd.arg("-device").arg("ide-hd,drive=disk0,bus=ahci.0");

    // Debugging and Testing Logic
    if args.debugger {
        cmd.arg("-s"); // Shorthand for -gdb tcp::1234
        cmd.arg("-S"); // Freeze CPU at startup
        println!("[DEBUG] QEMU paused. Attach debugger to port 1234 (target remote :1234)");
    }

    if args.test {
        // Use nographic and isa-debug-exit for CI/automated testing
        cmd.arg("-nographic");
        cmd.arg("-device")
            .arg("isa-debug-exit,iobase=0xf4,iosize=0x04");
    } else {
        // Map serial port to terminal for logging
        cmd.arg("-serial").arg("stdio");
    }

    println!("[RUN] Command: {cmd:#?}");
    std::io::stdout().flush().unwrap();

    let mut child = cmd.spawn().expect("Failed to launch QEMU");
    let status = child.wait().expect("Failed to wait on QEMU process");

    println!("[DONE] QEMU exited with status: {}", status);
}

/// Invokes Cargo to build the core OS kernel.
/// Includes nightly-only flags for building core/alloc from source.
fn build_kernel(args: Args) -> PathBuf {
    println!("[BUILD] Compiling kernel...");
    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("+nightly");
    cmd.arg("build");
    cmd.arg("-p").arg("zenos-kernel");

    #[cfg(not(debug_assertions))]
    {
        println!("[BUILD] Target: Release");
        cmd.arg("--release");
    }

    // Custom JSON target specification for the kernel
    cmd.arg("--target=x86_64-unknown-zenos.json");
    cmd.arg("--bin=zenos-kernel");

    // Recompile core and alloc with kernel-specific settings
    cmd.arg("-Z").arg("build-std=core,alloc");
    cmd.arg("-Z")
        .arg("build-std-features=compiler-builtins-mem");

    if args.color {
        cmd.arg("-F").arg("color");
    }
    if args.test_stub {
        cmd.arg("-F").arg("test_stub");
    }
    if args.test {
        cmd.arg("-F").arg("run-kunittest");
    }

    // Frame pointers are essential for kernel backtraces/debugging
    #[cfg(debug_assertions)]
    cmd.env("RUSTFLAGS", "-Cforce-frame-pointers=yes");

    let status = cmd.status().expect("Cargo execution failed");
    if !status.success() {
        eprintln!("[ERROR] Kernel build failed.");
        exit(1);
    }

    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    Path::new("./target/x86_64-unknown-zenos")
        .join(profile)
        .join("zenos-kernel")
}

/// Builds the 'init' process (the first userspace program).
/// Copies the resulting ELF to the 'iso/bin' staging directory.
fn build_init(_args: Args) {
    println!("[BUILD] Compiling userspace init...");
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

    let status = cmd.status().expect("Failed to build init");
    if !status.success() {
        eprintln!("[ERROR] Init process build failed.");
        exit(1);
    }

    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let init_bin = Path::new("./target/x86_64-unknown-zenos-user")
        .join(profile)
        .join("zenos-init");

    // Prepare staging directory for disk image creation
    let out_dir = Path::new("iso").join("bin");
    std::fs::create_dir_all(&out_dir).expect("Could not create iso/bin directory");

    let out_path = out_dir.join("init.elf");
    std::fs::copy(&init_bin, &out_path).expect("Failed to stage init.elf");
}

/// Packages the kernel and the 'iso' directory into a UEFI-bootable disk image.
fn disk_img_builder(kernel_path: &Path) -> PathBuf {
    let mut builder = zenos_bootloader::DiskImageBuilder::new(kernel_path.to_path_buf());
    let uefi_out_path = PathBuf::from("uefi.img");
    let iso_dir = Path::new("iso");

    if iso_dir.exists() && iso_dir.is_dir() {
        println!("[DISK] Traversing staging directory: {}", iso_dir.display());
        add_files_recursively(&mut builder, iso_dir, iso_dir);
    } else {
        panic!("[FATAL] Staging directory 'iso/' is missing! Run build_init first.");
    }

    builder
        .create_uefi_image(uefi_out_path.as_path())
        .expect("Failed to generate UEFI disk image");

    uefi_out_path
}

/// Walk the local 'iso' directory and map files into the virtual disk image.
fn add_files_recursively(
    builder: &mut zenos_bootloader::DiskImageBuilder,
    dir: &Path,
    iso_root: &Path,
) {
    for entry in std::fs::read_dir(dir).expect("Could not read directory") {
        let entry = entry.expect("Entry error");
        let path = entry.path();

        if path.is_dir() {
            add_files_recursively(builder, &path, iso_root);
        } else if path.is_file() {
            let relative_path = path.strip_prefix(iso_root).unwrap();
            let relative_path_str = relative_path
                .to_str()
                .expect("Non-UTF8 path")
                .replace("\\", "/");

            println!(
                "[DISK] Mapping: {} -> /{}",
                path.display(),
                relative_path_str
            );
            builder.set_file(relative_path_str, path);
        }
    }
}

/// Builds the fuzzer. If --fuzz is enabled, this binary is renamed to 'init.elf'
/// to hijack the boot sequence and start fuzzing immediately.
fn build_fuzz(args: Args) {
    println!("[BUILD] Compiling fuzzer...");
    let mut cmd = std::process::Command::new("cargo");
    cmd.arg("+nightly");
    cmd.arg("build");
    cmd.arg("-p").arg("zenos-fuzz");

    #[cfg(not(debug_assertions))]
    cmd.arg("--release");

    cmd.arg("--target=x86_64-unknown-zenos-user.json");
    cmd.arg("--bin=zenos-fuzz");
    cmd.arg("-Z").arg("build-std=core,alloc");
    cmd.arg("-Z")
        .arg("build-std-features=compiler-builtins-mem");

    #[cfg(debug_assertions)]
    cmd.env("RUSTFLAGS", "-Cforce-frame-pointers=yes");

    let status = cmd.status().expect("Fuzzer build failed");
    if !status.success() {
        eprintln!("[ERROR] Fuzzer build failed.");
        exit(1);
    }

    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };
    let fuzz_bin = Path::new("./target/x86_64-unknown-zenos-user")
        .join(profile)
        .join("zenos-fuzz");

    let out_dir = Path::new("iso").join("bin");
    std::fs::create_dir_all(&out_dir).unwrap();

    // If fuzzing mode is on, we replace the standard init process with the fuzzer.
    let out_path = if args.fuzz {
        println!("[INFO] Fuzzing mode active: Fuzzer will act as init.");
        out_dir.join("init.elf")
    } else {
        out_dir.join("fuzz.elf")
    };

    std::fs::copy(&fuzz_bin, &out_path).expect("Failed to stage fuzzer binary");
}
