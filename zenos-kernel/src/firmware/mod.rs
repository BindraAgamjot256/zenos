use alloc::string::String;
use bootloader_api::BootInfo;

pub mod acpi;

#[derive(Debug, Default)]
pub struct RuntimeBootInfo {
    pub hpet: Option<Hpet>,
    pub oem_id: Option<String>,
    // TODO: more fields(lapic, arm timers, apic PM timer, shutdown callbacks etc).
}

#[derive(Debug)]
pub struct Hpet {
    pub address: usize,
    pub bits_64: bool,
}

pub fn init(boot_info: &'static BootInfo) -> RuntimeBootInfo {
    let mut populate = RuntimeBootInfo::default();
    if let Some(rsdp) = boot_info.rsdp_addr.into_option().map(|a| a as usize) {
        acpi::populate(&mut populate, rsdp);
    }
    // TODO: populate using other methods also, like devicetree on arm.

    populate
}
