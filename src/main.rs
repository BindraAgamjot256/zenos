mod building;
mod disk_image;
mod emulator;

use std::{
    io::{self, Write},
    path::{Path, PathBuf},
    process::{Command, ExitStatus},
};

use clap::{Parser, Subcommand};

type Result<T> = std::result::Result<T, Box<dyn std::error::Error>>;

#[derive(Parser, Debug)]
#[command(author, version, about = "Build and run the Zenos Operating System")]
struct Cli {
    #[command(subcommand)]
    command: Option<CliCommand>,

    /// Run using Bochs instead of QEMU
    #[arg(long, short = 'b', global = true)]
    bochs: bool,

    /// Use FAT filesystem for data partition instead of ext2
    #[arg(long, short = 'f')]
    fat: bool,

    /// Initial ramdisk
    #[arg(long)]
    initrd: Option<PathBuf>,
}

#[derive(Subcommand, Clone, Copy, Default, Debug)]
enum CliCommand {
    /// Build and run the kernel (default)
    #[default]
    #[command(alias = "r")]
    Run,

    /// Only check/build components without launching QEMU
    #[command(alias = "c")]
    Check,

    /// Start QEMU in paused mode and open a GDB stub (port 1234)
    #[command(alias = "d")]
    Debug,

    /// Rerun the previous kernel build without rebuilding
    #[command(alias = "R")]
    Rerun,

    /// Run the host-native unit tests for kernel subcomponents.
    #[command(alias = "t")]
    Test,
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match cli.command.unwrap_or_default() {
        CliCommand::Check => check(),
        CliCommand::Rerun => rerun(&cli),
        CliCommand::Test => building::host_test(),
        CliCommand::Run | CliCommand::Debug => build_and_run(&cli, cli.command.unwrap_or_default()),
    }
}

fn check() -> Result<()> {
    println!("[CHECK] Verification mode: building components...");

    building::build_init()?;
    building::build_kernel()?;

    println!("[CHECK] All components compiled successfully.");

    Ok(())
}

fn rerun(cli: &Cli) -> Result<()> {
    println!("[RERUN] Skipping build, using existing uefi.img...");

    let image = Path::new(disk_image::DISK_IMAGE);

    if !image.exists() {
        return Err("No existing uefi.img found. Run a full build first.".into());
    }

    emulator::run(cli.bochs, image, false)
}

fn build_and_run(cli: &Cli, command: CliCommand) -> Result<()> {
    println!("[BUILD] Starting full system build...");

    building::clean_staging()?;

    building::build_init()?;
    building::build_coreutils()?;
    building::build_shell()?;

    let kernel = building::build_kernel()?;
    let image = disk_image::build(&kernel, cli.initrd.as_deref(), cli.fat)?;

    println!("[INFO] Kernel binary: {}", kernel.display());
    println!("[INFO] UEFI image:    {}", image.display());

    let debugger = matches!(command, CliCommand::Debug);

    emulator::run(cli.bochs, &image, debugger)
}

fn run_command(command: &mut Command, name: &str) -> Result<()> {
    println!("[RUN] Command: {command:#?}");
    io::stdout().flush()?;

    let status = command
        .status()
        .map_err(|error| format!("failed to launch {name}: {error}"))?;

    ensure_success(status, name)
}

fn ensure_success(status: ExitStatus, name: &str) -> Result<()> {
    if !status.success() {
        return Err(format!("{name} exited with status {status}").into());
    }

    println!("[DONE] {name} exited successfully.");

    Ok(())
}
