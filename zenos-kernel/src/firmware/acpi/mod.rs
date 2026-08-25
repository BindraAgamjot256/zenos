use crate::firmware::RuntimeBootInfo;
use acpi::{AcpiTables, HpetInfo, sdt::fadt::Fadt};
use alloc::string::String;

mod handler;

pub fn populate(populate: &mut RuntimeBootInfo, rsdp_addr: usize) {
    let rsdp = unsafe { AcpiTables::from_rsdp(handler::AcpiHandler, rsdp_addr) }.ok();
    if rsdp.is_none() {
        return;
    }
    let rsdp = rsdp.unwrap();

    let oemid = rsdp.find_table::<Fadt>().map(|f| f.header.oem_id);

    let oemid = oemid.unwrap_or_default().clone();
    let str = str::from_utf8(&oemid).ok();
    populate.oem_id = Some(String::from(str.unwrap_or_default()));

    log::info!("Running on machine made by: {}", str.unwrap_or_default());

    populate.hpet = match HpetInfo::new(&rsdp) {
        Ok(info) => {
            log::info!(
                "HPET found at {:#x}, 64-bit counter: {}",
                info.base_address,
                info.main_counter_is_64bits
            );

            Some(super::Hpet {
                address: info.base_address,
                bits_64: info.main_counter_is_64bits,
            })
        }
        Err(err) => {
            log::warn!("No usable HPET found: {:?}", err);
            None
        }
    };
}
