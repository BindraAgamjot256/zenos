use crate::firmware::acpi::tables::LapicEntryFlags;
use crate::firmware::acpi::tables::MadtEntry;
use crate::firmware::*;
use alloc::string::String;
use alloc::string::ToString;
use alloc::vec;

mod tables;

pub fn populate(pop: &mut RuntimeBootInfo, rsdp_addr: usize) -> Option<()> {
    let rsdp = tables::Rsdp::from_address(rsdp_addr)?;

    if rsdp.revision() == 0 {
        return None;
    }

    let xsdt = unsafe { rsdp.xsdt_address()?.as_ref() };
    let oem = String::from_utf8_lossy(&xsdt.header.oem_id).to_string();
    pop.platform.oem_id = Some(oem);
    let madt = xsdt.find_table::<tables::Madt>()?;
    parse_madt(pop, madt)?;

    let hpet = xsdt.find_table::<tables::Hpet>();
    if let Some(hpet) = hpet {
        let hpet_addr = PhysAddr::new(hpet.addr.address);
        let resource = Resource::MmioRegion {
            address: hpet_addr,
            size: 0x1000,
        };
        let device = AcpiDevice {
            id: DeviceId::new(DeviceClass::Timer, b"HPET"),
            resources: vec![resource],
        };
        pop.devices.push(Box::new(device));
    }

    Some(())
}

fn parse_madt(pop: &mut RuntimeBootInfo, madt: &tables::Madt) -> Option<()> {
    let mut isr_overrides = Vec::new();
    let mut devices = Vec::new();
    for entry in madt.into_iter() {
        match entry {
            MadtEntry::LocalApic {
                processor_id,
                apic_id,
                flags,
            } => {
                let status = match flags {
                    LapicEntryFlags::Disabled => Status::Offline,
                    LapicEntryFlags::Enabled => Status::Online,
                    LapicEntryFlags::HotPlugable => Status::Unplugged,
                    _ => Status::Unknown,
                };
                pop.platform.cpus.push(Cpu {
                    cpu_id: processor_id,
                    status,
                });
                if processor_id == 0 {
                    pop.platform.boot_cpu_id = Some((apic_id as u16) << 8 | processor_id as u16);
                }
            }
            MadtEntry::IoApic {
                io_apic_id,
                _resv,
                io_apic_addr,
                gsi_base,
            } => {
                let resources = vec![
                    Resource::MmioRegion {
                        address: PhysAddr::new(io_apic_addr as u64),
                        size: 0x1000,
                    },
                    Resource::InterruptController {
                        id: InterruptControllerId(io_apic_id as u16),
                        interrupts: gsi_base as u64,
                        name: Some("IoApic"),
                    },
                ];

                let device_id = DeviceId::new(DeviceClass::InterruptController, b"IoApic");

                let device = AcpiDevice {
                    id: device_id,
                    resources,
                };
                devices.push(Box::new(device));
            }
            MadtEntry::IoApicIsrOverride {
                _bus,
                source,
                gsi,
                flags,
            } => {
                isr_overrides.push((source as u16, gsi, flags));
            }
            _ => return None,
        }
    }

    // push the local apic device
    devices.push(Box::new(AcpiDevice {
        id: DeviceId::new(DeviceClass::InterruptController, b"LocalApic"),
        resources: vec![Resource::MmioRegion {
            address: PhysAddr::new(madt.lapic_addr as u64),
            size: 0x1000,
        }],
    }));

    add_isr_overrides(&mut devices, &isr_overrides);

    pop.devices.extend(
        devices
            .into_iter()
            .map(|device| device as Box<dyn Device>)
            .collect::<Vec<Box<dyn Device>>>(),
    );

    Some(())
}

fn add_isr_overrides(devices: &mut [Box<AcpiDevice>], isr_overrides: &[(u16, u32, u16)]) {
    for (source, gsi, flags) in isr_overrides {
        let polarity = match flags & 0b11 {
            0b00 | 0b01 => Polarity::ActiveHigh,
            0b11 => Polarity::ActiveLow,
            _ => unreachable!(),
        };

        let trigger = match (flags >> 2) & 0b11 {
            0b00 | 0b01 => Trigger::Edge,
            0b11 => Trigger::Level,
            _ => unreachable!(),
        };

        let Some(device) = devices.iter_mut().find(|device| {
            device.resources.iter().any(|resource| {
                matches!(
                    resource,
                    Resource::InterruptController { interrupts, .. }
                        if *gsi as u64 >= *interrupts
                )
            })
        }) else {
            continue;
        };
        let res = device.resources[1].clone();
        let id = match res {
            Resource::InterruptController { id, .. } => id,
            _ => unreachable!(),
        };

        let number = *gsi as u128 | (*source as u128) << 32;

        device.resources.push(Resource::InterruptOverride {
            id,
            number,
            polarity,
            trigger,
        });
    }
}

#[derive(Debug, Clone)]
struct AcpiDevice {
    id: DeviceId,
    resources: Vec<Resource>,
}

impl Device for AcpiDevice {
    fn device_id(&self) -> DeviceId {
        self.id
    }

    fn resources(&self) -> &[Resource] {
        &self.resources
    }
}
