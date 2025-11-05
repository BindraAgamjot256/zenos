use crate::arch::{inl, outl};
use log::{info, trace, warn};

const CONFIG_ADDRESS: u16 = 0xCF8;
const CONFIG_DATA: u16 = 0xCFC;

pub struct PciDevice {
    pub _bus: u8,
    pub _slot: u8,
    pub _func: u8,
    pub bar5: u32, // MMIO base
}

// Read a byte from PCI config space
pub fn pci_config_read_byte(bus: u8, slot: u8, func: u8, offset: u8) -> u8 {
    let dword = pci_config_read_dword(bus, slot, func, offset & 0xFC);
    let value = ((dword >> ((offset & 3) * 8)) & 0xFF) as u8;
    trace!(
        "Read byte: bus={} slot={} func={} offset=0x{:02X} value=0x{:02X}",
        bus, slot, func, offset, value
    );
    value
}

// Read a 32-bit dword from PCI config space
pub fn pci_config_read_dword(bus: u8, slot: u8, func: u8, offset: u8) -> u32 {
    let address: u32 = (1 << 31)
        | ((bus as u32) << 16)
        | ((slot as u32) << 11)
        | ((func as u32) << 8)
        | (offset as u32 & 0xFC);
    unsafe {
        outl(CONFIG_ADDRESS, address);
        let value = inl(CONFIG_DATA);
        trace!(
            "Read dword: bus={} slot={} func={} offset=0x{:02X} value=0x{:08X}",
            bus, slot, func, offset, value
        );
        value
    }
}

// Helper to read 16-bit word (for vendor/device id)
pub fn pci_config_read_word(bus: u8, slot: u8, func: u8, offset: u8) -> u16 {
    let dword = pci_config_read_dword(bus, slot, func, offset & 0xFC);
    let value = ((dword >> ((offset & 2) * 8)) & 0xFFFF) as u16;
    trace!(
        "Read word: bus={} slot={} func={} offset=0x{:02X} value=0x{:04X}",
        bus, slot, func, offset, value
    );
    value
}

// Scan all buses for AHCI controller
pub fn scan_pci_for_ahci() -> Option<PciDevice> {
    info!("Starting PCI scan for AHCI controllers...");

    for bus in 0..=255 {
        for slot in 0..32 {
            for func in 0..8 {
                let vendor_id = pci_config_read_word(bus, slot, func, 0x00);
                if vendor_id == 0xFFFF {
                    continue;
                }

                let class_code = pci_config_read_byte(bus, slot, func, 0x0B);
                let subclass = pci_config_read_byte(bus, slot, func, 0x0A);
                let prog_if = pci_config_read_byte(bus, slot, func, 0x09);

                trace!(
                    "Checking device: bus={} slot={} func={} vendor_id=0x{:04X} class=0x{:02X} subclass=0x{:02X} prog_if=0x{:02X}",
                    bus, slot, func, vendor_id, class_code, subclass, prog_if
                );

                if class_code == 0x01 {
                    info!(
                        "Found SATA controller: bus={} slot={} func={} prog_if=0x{:02X}",
                        bus, slot, func, prog_if
                    );
                }

                if class_code == 0x01 && subclass == 0x06 {
                    info!(
                        "Found SATA controller: bus={} slot={} func={} prog_if=0x{:02X}",
                        bus, slot, func, prog_if
                    );
                }

                // AHCI: class 0x01, subclass 0x06, prog IF 0x01
                if class_code == 0x01 && subclass == 0x06 && prog_if == 0x01 {
                    let bar5 = pci_config_read_dword(bus, slot, func, 0x24) & 0xFFFF_FFF0;
                    info!(
                        "Found AHCI controller: bus={} slot={} func={} BAR5=0x{:08X}",
                        bus, slot, func, bar5
                    );
                    return Some(PciDevice {
                        _bus: bus,
                        _slot: slot,
                        _func: func,
                        bar5,
                    });
                }
            }
        }
    }

    warn!("No AHCI controller was found during the PCI scan.");
    None
}
