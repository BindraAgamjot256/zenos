fn main() {
    // read env variables that were set in the build script
    let uefi_path = env!("UEFI_PATH");
    let kernel_path = env!("KERNEL_PATH");

    println!("uefi path: {}", uefi_path);
    println!("kernel path: {}", kernel_path);

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
    cmd.arg("-drive")
        .arg(format!("id=disk0,if=none,file={},format=raw", uefi_path));
    cmd.arg("-device").arg("ide-hd,drive=disk0,bus=ahci.0");

    print!("running command: {cmd:#?}");
    let mut child = cmd.spawn().unwrap();
    child.wait().expect("failed to wait on child");
}
