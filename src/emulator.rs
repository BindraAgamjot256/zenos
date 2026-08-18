use std::{fs, path::Path, process::Command};

use crate::{run_command, Result};

pub(crate) fn run(bochs: bool, image: &Path, debugger: bool) -> Result<()> {
    match bochs {
        true => run_bochs(image, debugger),
        false => run_qemu(image, debugger),
    }
}

fn run_qemu(image: &Path, debugger: bool) -> Result<()> {
    println!("[RUN] Launching QEMU...");

    let mut command = Command::new("qemu-system-x86_64");

    command.args([
        "-machine",
        "q35",
        "-m",
        "512M",
        "-smp",
        "2",
        "-no-reboot",
        "-no-shutdown",
        "-d",
        "cpu_reset",
    ]);

    command.arg("-bios").arg(ovmf_prebuilt::ovmf_pure_efi());

    command.args(["-device", "ahci,id=ahci"]);

    command.arg("-drive").arg(format!(
        "id=disk0,if=none,file={},format=raw",
        image.display()
    ));

    command.args(["-device", "ide-hd,drive=disk0,bus=ahci.0"]);

    if debugger {
        command.args(["-s", "-S"]);

        println!(
            "[DEBUG] QEMU paused. Attach debugger to port 1234 \
             (target remote :1234)"
        );
    }

    command.args(["-serial", "stdio"]);

    run_command(&mut command, "QEMU")
}

fn run_bochs(image: &Path, debugger: bool) -> Result<()> {
    println!("[RUN] Launching Bochs...");

    let ovmf_source = ovmf_prebuilt::ovmf_pure_efi();

    let ovmf_dir = Path::new(".ovmf");
    fs::create_dir_all(ovmf_dir)?;

    let ovmf_destination = ovmf_dir.join("OVMF-pure-efi.fd");

    if !ovmf_destination.exists() {
        if ovmf_source.exists() {
            fs::copy(&ovmf_source, &ovmf_destination)?;
        } else {
            eprintln!(
                "[WARN] OVMF firmware not found at {}. Bochs may fail.",
                ovmf_source.display()
            );
        }
    }

    let bochsrc = format!(
        r#"# Auto-generated .bochsrc
megs: 512
boot: disk
ata0-master: type=disk, path="{}", mode=flat
romimage: file="{}"
vga: extension=cirrus
pci: enabled=1, chipset=i440fx, slot1=cirrus
clock: sync=realtime, time0=local
log: bochs.log
display_library: sdl2
com1: enabled=1, mode=file, dev=serial.log
"#,
        image.display(),
        ovmf_destination.display(),
    );

    fs::write(".bochsrc", bochsrc)?;

    let mut command = Command::new("bochs");
    command.arg("-f").arg(".bochsrc");

    if !debugger {
        command.arg("-q");
    }

    run_command(&mut command, "Bochs")
}
