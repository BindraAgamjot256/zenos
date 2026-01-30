extern crate alloc; // only for shutting up cargo check --target x86_64-unknown-zenos.json and co...
use alloc::format;
use clap::{Parser, Subcommand};
use core::cfg;
use core::convert::From;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::exit;

#[derive(Parser)]
#[command(author, version, about = "Build and run the Zenos Operating System")]
struct Cli {
    #[command(subcommand)]
    command: Option<Command>,

    /// Enable color output in the kernel
    #[arg(long, short = 'c', global = true)]
    color: bool,

    /// Build and run the kernel fuzzer (replaces init process)
    #[arg(long, short = 'f', global = true)]
    fuzz: bool,

    /// Run using Bochs instead of QEMU
    #[arg(long, global = true)]
    bochs: bool,
}

#[derive(Subcommand, Clone, Copy, Default)]
enum Command {
    /// Build and run the kernel (default)
    #[default]
    Run,
    /// Only check/build components without launching QEMU
    Check,
    /// Build with the test stub enabled
    Stub,
    /// Start QEMU in paused mode and open a GDB stub (port 1234)
    Debug,
    /// Run kernel unit tests and exit via isa-debug-exit
    Test,
    /// Rerun the previous kernel build without rebuilding (skips disk image too)
    Rerun,
    /// Run stress tests instead of shell
    Stress,
}

/// Internal args used by build functions
#[derive(Clone, Copy)]
struct BuildArgs {
    color: bool,
    fuzz: bool,
    test_stub: bool,
    test: bool,
    stress: bool,
}

fn main() {
    let cli = Cli::parse();
    let command = cli.command.unwrap_or_default();

    let build_args = BuildArgs {
        color: cli.color,
        fuzz: cli.fuzz,
        test_stub: matches!(command, Command::Stub),
        test: matches!(command, Command::Test),
        stress: matches!(command, Command::Stress),
    };

    match command {
        Command::Check => {
            println!("[CHECK] Verification mode: building components...");
            build_kernel(build_args);
            build_init(build_args);
            println!("[CHECK] All components compiled successfully.");
            return;
        }
        Command::Rerun => {
            println!("[RERUN] Skipping build, using existing uefi.img...");
            let uefi_path = PathBuf::from("uefi.img");
            if !uefi_path.exists() {
                eprintln!("[ERROR] No existing uefi.img found. Run a full build first.");
                exit(1);
            }
            if cli.bochs {
                run_bochs(&uefi_path, false, false);
            } else {
                run_qemu(&uefi_path, false, false);
            }
            return;
        }
        _ => {}
    }

    println!("[BUILD] Starting full system build...");
    let path = PathBuf::from("./iso/bin");
    if path.exists() {
        std::fs::remove_dir_all(&path).expect("Could not clean iso/bin directory");
    }
    build_init(build_args);
    if build_args.stress {
        build_stress_tests(build_args);
    } else if cli.fuzz {
        build_fuzz(build_args);
    } else {
        build_shell(build_args);
    }
    // 2. Image Construction Phase
    let kernel_binding = build_kernel(build_args);
    let kernel_path = kernel_binding.as_path();

    println!("[DISK] Creating bootable UEFI image...");
    let image_binding = disk_img_builder(kernel_path);
    let uefi_path = image_binding.as_path();

    println!("[INFO] Kernel binary: {}", kernel_path.display());
    println!("[INFO] UEFI Image:    {}", uefi_path.display());

    let debugger = matches!(command, Command::Debug);
    let test = matches!(command, Command::Test);
    if cli.bochs {
        run_bochs(uefi_path, debugger, test);
    } else {
        run_qemu(uefi_path, debugger, test);
    }
}

fn run_qemu(uefi_path: &Path, debugger: bool, test: bool) {
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
    if debugger {
        cmd.arg("-s"); // Shorthand for -gdb tcp::1234
        cmd.arg("-S"); // Freeze CPU at startup
        println!("[DEBUG] QEMU paused. Attach debugger to port 1234 (target remote :1234)");
    }

    if test {
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
    cmd.stdout(std::io::stdout());
    let mut child = cmd.spawn().expect("Failed to launch QEMU");
    let status = child.wait().expect("Failed to wait on QEMU process");

    println!("[DONE] QEMU exited with status: {}", status);
}

fn run_bochs(uefi_path: &Path, debugger: bool, _test: bool) {
    println!("[RUN] Launching Bochs...");

    // Use OVMF firmware (same as QEMU) to enable UEFI booting in Bochs
    let ovmf_src = ovmf_prebuilt::ovmf_pure_efi();

    // Copy OVMF firmware locally so Bochs can open it reliably
    let ovmf_dir = std::path::Path::new(".ovmf");
    if !ovmf_dir.exists() {
        std::fs::create_dir_all(ovmf_dir).expect("Failed to create .ovmf directory");
    }
    let ovmf_dst = ovmf_dir.join("OVMF-pure-efi.fd");
    if !ovmf_dst.exists() {
        if ovmf_src.exists() {
            std::fs::copy(&ovmf_src, &ovmf_dst).expect("Failed to copy OVMF firmware to .ovmf/");
        } else {
            eprintln!(
                "[WARN] OVMF firmware not found at {}. Bochs may fail.",
                ovmf_src.display()
            );
        }
    }

    // Generate a .bochsrc that points to the generated uefi image and local OVMF firmware
    let bochsrc = format!(
        r#"# Auto-generated .bochsrc to boot uefi.img with OVMF
megs: 512
boot: disk
ata0-master: type=disk, path="{}", mode=flat
romimage: file="{}"
vga: extension=cirrus
pci: enabled=1, chipset=i440fx, slot1=cirrus
# Force ACPI and Power Management logic
clock: sync=realtime, time0=local
log: bochs.log
display_library: sdl2
com1: enabled=1, mode=file, dev=serial.log
"#,
        uefi_path.to_str().unwrap(),
        ovmf_dst.to_str().unwrap()
    );

    std::fs::write(".bochsrc", bochsrc).expect("Failed to write .bochsrc for Bochs");

    let mut cmd = std::process::Command::new("bochs");
    cmd.arg("-f").arg(".bochsrc");

    // Skip interactive menus unless debugger requested
    if !debugger {
        cmd.arg("-q");
    }

    println!("[RUN] Command: {cmd:#?}");
    std::io::stdout().flush().unwrap();
    cmd.stdout(std::io::stdout());
    let mut child = cmd.spawn().expect("Failed to launch Bochs");
    let status = child.wait().expect("Failed to wait on Bochs process");

    println!("[DONE] Bochs exited with status: {}", status);
}

/// Invokes Cargo to build the core OS kernel.
/// Includes nightly-only flags for building core/alloc from source.
fn build_kernel(args: BuildArgs) -> PathBuf {
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
fn build_init(args: BuildArgs) {
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

    // Enable stress feature if running stress tests
    if args.stress {
        cmd.arg("-F").arg("stress");
    }

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
fn build_fuzz(args: BuildArgs) {
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

/// Builds the shell and copies it to iso/bin/
fn build_shell(_args: BuildArgs) {
    println!("[BUILD] Compiling shell...");

    let shell_dir = Path::new("zenos-shell");
    let build_dir = shell_dir.join("build");

    // Build shell (libc should already be built from build_test_1)
    let compile_status = std::process::Command::new("make")
        .current_dir(shell_dir)
        .env("CC", "clang --target=x86_64-unknown-none-elf")
        .env("LD", "ld.lld")
        .status()
        .expect("Failed to run make for shell");

    if !compile_status.success() {
        eprintln!("[ERROR] shell build failed.");
        exit(1);
    }

    // Copy shell binary to iso/bin/
    let out_dir = Path::new("iso").join("bin");
    std::fs::create_dir_all(&out_dir).unwrap();

    let src = build_dir.join("shell");
    let dst = out_dir.join("shell");
    if src.exists() {
        std::fs::copy(&src, &dst).expect("Failed to stage shell");
        println!("[BUILD] Staged shell -> {:?}", dst);
    } else {
        eprintln!("[ERROR] Shell binary not found: {:?}", src);
        exit(1);
    }
}

/// Builds all stress test programs and copies them to iso/bin/
fn build_stress_tests(_args: BuildArgs) {
    println!("[BUILD] Compiling stress tests...");

    let stress_dir = Path::new("zenos-stress-tests");
    let build_dir = stress_dir.join("build");

    // Build stress tests (libc should already be built from build_test_1)
    let compile_status = std::process::Command::new("make")
        .current_dir(stress_dir)
        .env("CC", "clang --target=x86_64-unknown-none-elf")
        .env("LD", "ld.lld")
        .status()
        .expect("Failed to run make for stress tests");

    if !compile_status.success() {
        eprintln!("[ERROR] stress tests build failed.");
        exit(1);
    }

    // Copy all stress test binaries to iso/bin/
    let out_dir = Path::new("iso").join("bin");
    std::fs::create_dir_all(&out_dir).unwrap();

    let stress_tests = [
        ("fork_storm", "forkstrm.elf"),
        ("rapid_spawn", "rapidspn.elf"),
        ("sched_fairness", "schedfar.elf"),
        ("mem_exhaust", "memexhst.elf"),
        ("orphan_zombie", "orphzomb.elf"),
        ("fs_concurrent", "fsconcrn.elf"),
    ];

    for (src_name, dst_name) in stress_tests {
        let src = build_dir.join(src_name);
        let dst = out_dir.join(dst_name);
        if src.exists() {
            std::fs::copy(&src, &dst).expect(&format!("Failed to stage {}", src_name));
            println!("[BUILD] Staged {} -> {:?}", src_name, dst);
        } else {
            eprintln!("[WARN] Stress test binary not found: {:?}", src);
        }
    }
}
