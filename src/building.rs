use std::{
    fs,
    path::{Path, PathBuf},
    process::Command,
};

use crate::{run_command, Result};

const KERNEL_TARGET: &str = "x86_64-unknown-zenos.json";
const USER_TARGET: &str = "x86_64-unknown-zenos-user.json";

const KERNEL_TRIPLE: &str = "x86_64-unknown-zenos";
const USER_TRIPLE: &str = "x86_64-unknown-zenos-user";

const KERNEL_PACKAGE: &str = "zenos-kernel";
const INIT_PACKAGE: &str = "zenos-init";

const KERNEL_BINARY: &str = "zenos-kernel";
const INIT_BINARY: &str = "zenos-init";

const COREUTILS: &[&str] = &[
    "ls", "cat", "grep", "echo", "true", "false", "yes", "wc", "head", "tail", "od",
];

pub(crate) fn build_kernel(test_timer: bool) -> Result<PathBuf> {
    println!("[BUILD] Compiling kernel...");

    let mut command = cargo_build(KERNEL_PACKAGE, KERNEL_TARGET);

    command.args([
        &format!("--bin={KERNEL_BINARY}"),
        "-Z",
        "build-std=core,alloc",
        "-Z",
        "build-std-features=compiler-builtins-mem",
    ]);

    if test_timer {
        command.arg("--features=__test_timer");
    }

    apply_debug_rustflags(&mut command);
    run_build(&mut command, "Kernel")?;

    Ok(target_binary(KERNEL_TRIPLE, KERNEL_BINARY))
}

pub(crate) fn build_init() -> Result<()> {
    println!("[BUILD] Compiling userspace init...");

    let mut command = cargo_build(INIT_PACKAGE, USER_TARGET);

    command.args([
        &format!("--bin={INIT_BINARY}"),
        "-Z",
        "build-std=core,alloc",
        "-Z",
        "build-std-features=compiler-builtins-mem",
    ]);

    apply_debug_rustflags(&mut command);
    run_build(&mut command, "Init")?;

    let init_binary = target_binary(USER_TRIPLE, INIT_BINARY);

    stage_binary(&init_binary, Path::new("iso/bin/init"), "init")
}

pub(crate) fn build_shell() -> Result<()> {
    println!("[BUILD] Compiling shell...");

    run_make("zenos-shell", "shell")?;

    stage_binary(
        Path::new("zenos-shell/build/shell"),
        Path::new("iso/bin/shell"),
        "shell",
    )
}

pub(crate) fn build_coreutils() -> Result<()> {
    println!("[BUILD] Compiling coreutils...");

    run_make("zenos-coreutils", "coreutils")?;

    let build_dir = Path::new("zenos-coreutils/build");
    let output_dir = Path::new("iso/usr/bin");

    fs::create_dir_all(output_dir)?;

    for utility in COREUTILS {
        let source = build_dir.join(utility);
        let destination = output_dir.join(utility);

        if source.exists() {
            stage_binary(&source, &destination, utility)?;
        } else {
            eprintln!("[WARN] Coreutil binary not found: {}", source.display());
        }
    }

    Ok(())
}

pub(crate) fn host_test() -> Result<()> {
    println!("[TEST] running memory manager tests...");

    let mut command = Command::new("cargo");
    command.args([
        "test",
        "-p",
        "kmm",
        "--",
        "--nocapture",
        "--color",
        "always",
    ]);

    run_command(&mut command, "Host tests")?;

    let mut command = Command::new("make");
    command.arg("test");
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    command.current_dir(root.join("libc"));

    run_command(&mut command, "Host libc tests")?;
    println!("[TEST] Host tests completed successfully.");

    Ok(())
}

pub(crate) fn clean_staging() -> Result<()> {
    for directory in ["iso/bin", "iso/usr/bin"] {
        let path = Path::new(directory);

        if path.exists() {
            println!("[CLEAN] Removing {}", path.display());
            fs::remove_dir_all(path)?;
        }
    }

    Ok(())
}

fn cargo_build(package: &str, target: &str) -> Command {
    let mut command = Command::new("cargo");

    command.args(["+nightly", "build", "-p", package, "-Z", "json-target-spec"]);

    command.arg(format!("--target={target}"));

    if cfg!(not(debug_assertions)) {
        println!("[BUILD] Target: release");
        command.arg("--release");
    }

    command
}

fn target_binary(triple: &str, binary: &str) -> PathBuf {
    let profile = if cfg!(debug_assertions) {
        "debug"
    } else {
        "release"
    };

    Path::new("target").join(triple).join(profile).join(binary)
}

fn apply_debug_rustflags(command: &mut Command) {
    #[cfg(debug_assertions)]
    command.env("RUSTFLAGS", "-Cforce-frame-pointers=yes");
}

fn run_build(command: &mut Command, name: &str) -> Result<()> {
    run_command(command, &format!("{name} build"))
}

fn run_make(directory: &str, name: &str) -> Result<()> {
    let mut command = Command::new("make");

    command
        .current_dir(directory)
        .env("CC", "clang --target=x86_64-unknown-none-elf")
        .env("LD", "ld.lld");

    run_command(&mut command, &format!("{name} build"))
}

fn stage_binary(source: &Path, destination: &Path, name: &str) -> Result<()> {
    if !source.is_file() {
        return Err(format!("{name} binary not found: {}", source.display()).into());
    }

    let Some(parent) = destination.parent() else {
        return Err(format!("destination has no parent: {}", destination.display()).into());
    };

    fs::create_dir_all(parent)?;
    fs::copy(source, destination)?;

    println!("[BUILD] Staged {} -> {}", name, destination.display());

    Ok(())
}
