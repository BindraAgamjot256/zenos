use crate::firmware::*;
use alloc::vec;
use core::panic;
use uacpi_sys::acpi_madt_entry_type;

mod helpers;

pub fn populate(pop: &mut RuntimeBootInfo, rsdp_addr: usize) -> Option<()> {
    log::info!("ACPI: starting firmware population");
    log::info!("ACPI: RSDP address = {rsdp_addr:#x}");

    let mut local = RuntimeBootInfo::default();

    log::debug!("ACPI: initializing uACPI helpers");
    helpers::init(rsdp_addr);

    log::debug!("ACPI: calling uacpi_initialize()");
    let t = unsafe { uacpi_sys::uacpi_initialize(0) };

    if t != uacpi_sys::uacpi_status::UACPI_STATUS_OK {
        log::error!("ACPI: uacpi_initialize() failed: {t:?}");
        panic!("uacpi_initialize failed: {t:?}");
    }

    log::info!("ACPI: uACPI initialized successfully");

    if !unsafe { uacpi_sys::uacpi_table_subsystem_available() } {
        log::error!("ACPI: table subsystem is not available");
        return None;
    }

    log::info!("ACPI: table subsystem is available");

    log::debug!("ACPI: populating HPET");
    if populate_hpet(&mut local).is_none() {
        log::warn!("ACPI: HPET table not found or could not be populated");
    } else {
        log::info!("ACPI: HPET populated successfully");
    }

    log::debug!("ACPI: populating MADT");
    if populate_madt(&mut local).is_none() {
        log::error!("ACPI: failed to populate MADT");
        return None;
    }

    log::info!(
        "ACPI: firmware population complete: {} CPUs, {} devices",
        local.platform.cpus.len(),
        local.devices.len()
    );

    log::debug!("ACPI: boot CPU ID = {:?}", local.platform.boot_cpu_id);

    *pop = local;

    Some(())
}

pub fn populate_hpet(pop: &mut RuntimeBootInfo) -> Option<()> {
    log::debug!("ACPI/HPET: looking up HPET table");

    let Ok(hpet_addr) = uacpi_sys::get_table_address(uacpi_sys::ACPI_HPET_SIGNATURE) else {
        log::warn!("ACPI/HPET: HPET table not found");
        return None;
    };

    log::info!("ACPI/HPET: table address = {hpet_addr:#x}");

    let hpet_table = unsafe { &*core::ptr::without_provenance::<uacpi_sys::acpi_hpet>(hpet_addr) };
    let phys_addr = PhysAddr::new(hpet_table.address.address);

    log::debug!("ACPI/HPET: hardware address = {:#x}", phys_addr.as_usize());

    log::debug!(
        "ACPI/HPET: address space = {:?}, access width = {:?}",
        hpet_table.address.address_space_id,
        hpet_table.address.register_bit_width
    );

    let resource = Resource::MmioRegion {
        address: phys_addr,
        size: 0x1000,
    };

    let device = AcpiDevice {
        id: DeviceId::new(DeviceClass::Timer, b"HPET"),
        resources: vec![resource],
    };

    log::info!("ACPI/HPET: created device: {:#?}", device);

    pop.devices.push(Box::new(device));

    log::debug!(
        "ACPI/HPET: device added, total devices = {}",
        pop.devices.len()
    );

    Some(())
}

// A temporary container to hold our state during iteration
struct MadtContext {
    cpus: Vec<Cpu>,
    boot_cpu_id: Option<u16>,
    isr_overrides: Vec<(u16, u32, u16)>,
    devices: Vec<Box<AcpiDevice>>,
    failed: bool,
    addr_override: Option<PhysAddr>,
}

unsafe extern "C" fn madt_subtable_cb(
    user: *mut core::ffi::c_void,
    entry_ptr: *mut uacpi_sys::acpi_entry_hdr,
) -> u32 {
    let ctx = unsafe { &mut *(user as *mut MadtContext) };
    let entry = unsafe { &*entry_ptr };

    log::debug!(
        "ACPI/MADT: processing entry: type = {}, length = {}",
        entry.type_,
        entry.length
    );

    match acpi_madt_entry_type(entry.type_ as u32) {
        uacpi_sys::acpi_madt_entry_type::ACPI_MADT_ENTRY_TYPE_LAPIC => {
            let lapic = unsafe { &*(entry_ptr as *const uacpi_sys::acpi_madt_lapic) };

            log::info!(
                "ACPI/MADT: LAPIC: processor_uid = {}, apic_id = {}, flags = {:#x}",
                lapic.uid,
                lapic.id,
                lapic.flags as u16
            );

            let status = if (lapic.flags & 1) != 0 {
                Status::Online
            } else if (lapic.flags & 2) != 0 {
                Status::Unplugged
            } else {
                Status::Offline
            };

            log::debug!(
                "ACPI/MADT: LAPIC {} mapped to status {:?}",
                lapic.id,
                status
            );

            ctx.cpus.push(Cpu {
                cpu_id: lapic.id,
                status,
            });

            log::debug!("ACPI/MADT: CPU added, total CPUs = {}", ctx.cpus.len());

            if lapic.uid == 0 {
                let boot_cpu_id = (lapic.uid as u16) << 8 | lapic.id as u16;

                log::info!(
                    "ACPI/MADT: LAPIC {} identified as boot CPU, boot_cpu_id = {}",
                    lapic.id,
                    boot_cpu_id
                );

                ctx.boot_cpu_id = Some(boot_cpu_id);
            }
        }

        uacpi_sys::acpi_madt_entry_type::ACPI_MADT_ENTRY_TYPE_IOAPIC => {
            let ioapic = unsafe { &*(entry_ptr as *const uacpi_sys::acpi_madt_ioapic) };

            let id = ioapic.id as u16;

            log::info!(
                "ACPI/MADT: IOAPIC: id = {}, address = {:#x}, GSI base = {}",
                id,
                PhysAddr::new(ioapic.address as u64).as_u64(),
                ioapic.gsi_base as u16
            );

            let resources = vec![
                Resource::MmioRegion {
                    address: PhysAddr::new(ioapic.address as u64),
                    size: 0x1000,
                },
                Resource::InterruptController {
                    id: InterruptControllerId(ioapic.id as u16),
                    interrupts: ioapic.gsi_base as u64,
                    name: Some("IoApic"),
                },
            ];

            let device = AcpiDevice {
                id: DeviceId::new(DeviceClass::InterruptController, b"IoApic"),
                resources,
            };

            log::debug!("ACPI/MADT: created IOAPIC device: {:#?}", device);

            ctx.devices.push(Box::new(device));

            log::debug!(
                "ACPI/MADT: IOAPIC added, total devices = {}",
                ctx.devices.len()
            );
        }

        uacpi_sys::acpi_madt_entry_type::ACPI_MADT_ENTRY_TYPE_INTERRUPT_SOURCE_OVERRIDE => {
            let iso =
                unsafe { &*(entry_ptr as *const uacpi_sys::acpi_madt_interrupt_source_override) };

            let source = iso.source as u16;
            let gsi = iso.gsi;
            let flags = iso.flags;

            log::info!(
                "ACPI/MADT: interrupt source override: source IRQ = {}, GSI = {}, flags = {:#06x}",
                source,
                gsi,
                flags
            );

            ctx.isr_overrides.push((source, gsi, flags));

            log::debug!(
                "ACPI/MADT: ISR override added, total overrides = {}",
                ctx.isr_overrides.len()
            );
        }

        uacpi_sys::acpi_madt_entry_type::ACPI_MADT_ENTRY_TYPE_NMI_SOURCE => {
            log::info!("ACPI/MADT: NMI source entry encountered");
        }

        uacpi_sys::acpi_madt_entry_type::ACPI_MADT_ENTRY_TYPE_LAPIC_NMI => {
            log::info!("ACPI/MADT: LAPIC NMI entry encountered");
        }

        uacpi_sys::acpi_madt_entry_type::ACPI_MADT_ENTRY_TYPE_LAPIC_ADDRESS_OVERRIDE => {
            let override_entry =
                unsafe { &*(entry_ptr as *const uacpi_sys::acpi_madt_lapic_address_override) };
            let phys_addr = PhysAddr::new(override_entry.address);

            log::info!(
                "ACPI/MADT: LAPIC address override: address = {:#x}",
                phys_addr.as_usize()
            );

            ctx.addr_override = Some(phys_addr);
        }

        _ => {
            log::warn!(
                "ACPI/MADT: unsupported/unexpected entry type: {}",
                entry.type_
            );
        }
    }

    uacpi_sys::uacpi_status::UACPI_STATUS_OK.0
}

pub fn populate_madt(pop: &mut RuntimeBootInfo) -> Option<()> {
    log::debug!("ACPI/MADT: looking up MADT table");

    let Ok(madt_addr) = uacpi_sys::get_table_address(uacpi_sys::ACPI_MADT_SIGNATURE) else {
        log::error!("ACPI/MADT: MADT table not found");
        return None;
    };

    log::info!("ACPI/MADT: table address = {madt_addr:#x}");

    let madt_table =
        unsafe { &mut *core::ptr::without_provenance_mut::<uacpi_sys::acpi_madt>(madt_addr) };

    log::info!(
        "ACPI/MADT: local interrupt controller address = {:#x}",
        madt_table.local_interrupt_controller_address as u64
    );

    log::debug!(
        "ACPI/MADT: MADT table length = {}",
        madt_table.hdr.length as u64
    );

    let mut ctx = MadtContext {
        cpus: Vec::new(),
        boot_cpu_id: None,
        isr_overrides: Vec::new(),
        devices: Vec::new(),
        failed: false,
        addr_override: None,
    };

    log::debug!("ACPI/MADT: starting subtable iteration");

    let status = unsafe {
        uacpi_sys::uacpi_for_each_subtable(
            &raw mut madt_table.hdr,
            core::mem::size_of::<uacpi_sys::acpi_madt>(),
            Some(madt_subtable_cb),
            &raw mut ctx as *mut core::ffi::c_void,
        )
    };

    log::debug!(
        "ACPI/MADT: subtable iteration returned status = {:?}",
        status
    );

    if status != uacpi_sys::uacpi_status::UACPI_STATUS_OK || ctx.failed {
        log::error!(
            "ACPI/MADT: subtable iteration failed: status = {:?}, failed = {}",
            status,
            ctx.failed
        );

        return None;
    }

    log::info!(
        "ACPI/MADT: parsed {} CPUs, {} devices, {} interrupt overrides",
        ctx.cpus.len(),
        ctx.devices.len(),
        ctx.isr_overrides.len()
    );

    log::info!("ACPI/MADT: boot CPU ID = {:?}", ctx.boot_cpu_id);

    for cpu in &ctx.cpus {
        log::debug!(
            "ACPI/MADT: CPU: id = {}, status = {:?}",
            cpu.cpu_id,
            cpu.status
        );
    }

    pop.platform.cpus.extend(ctx.cpus);

    if ctx.boot_cpu_id.is_some() {
        pop.platform.boot_cpu_id = ctx.boot_cpu_id;
    }

    log::debug!(
        "ACPI/MADT: platform now contains {} CPUs",
        pop.platform.cpus.len()
    );

    let mut devices = ctx.devices;

    let local_apic_address = if let Some(addr) = ctx.addr_override {
        addr
    } else {
        PhysAddr::new(madt_table.local_interrupt_controller_address as u64)
    };

    log::info!(
        "ACPI/MADT: creating Local APIC device at {:?}",
        local_apic_address
    );

    devices.push(Box::new(AcpiDevice {
        id: DeviceId::new(DeviceClass::InterruptController, b"LocalApic"),
        resources: vec![Resource::MmioRegion {
            address: local_apic_address,
            size: 0x1000,
        }],
    }));

    log::debug!(
        "ACPI/MADT: Local APIC device added, total devices = {}",
        devices.len()
    );

    add_isr_overrides(&mut devices, &ctx.isr_overrides);

    log::debug!(
        "ACPI/MADT: extending {} devices into RuntimeBootInfo",
        devices.len()
    );

    pop.devices
        .extend(devices.into_iter().map(|device| device as Box<dyn Device>));

    log::info!(
        "ACPI/MADT: population complete, RuntimeBootInfo now has {} devices",
        pop.devices.len()
    );

    Some(())
}

fn add_isr_overrides(devices: &mut [Box<AcpiDevice>], isr_overrides: &[(u16, u32, u16)]) {
    log::debug!(
        "ACPI/IRQ: processing {} interrupt source overrides",
        isr_overrides.len()
    );

    for (source, gsi, flags) in isr_overrides {
        log::info!(
            "ACPI/IRQ: processing override: legacy IRQ {} -> GSI {}, flags = {:#06x}",
            source,
            gsi,
            flags
        );

        let polarity = match flags & 0b11 {
            0b00 | 0b01 => {
                log::debug!(
                    "ACPI/IRQ: legacy IRQ {} -> GSI {} polarity = ActiveHigh",
                    source,
                    gsi
                );
                Polarity::ActiveHigh
            }
            0b11 => {
                log::debug!(
                    "ACPI/IRQ: legacy IRQ {} -> GSI {} polarity = ActiveLow",
                    source,
                    gsi
                );
                Polarity::ActiveLow
            }
            value => {
                log::error!(
                    "ACPI/IRQ: invalid polarity encoding {:#b} for IRQ {} -> GSI {}",
                    value,
                    source,
                    gsi
                );
                unreachable!();
            }
        };

        let trigger = match (flags >> 2) & 0b11 {
            0b00 | 0b01 => {
                log::debug!(
                    "ACPI/IRQ: legacy IRQ {} -> GSI {} trigger = Edge",
                    source,
                    gsi
                );
                Trigger::Edge
            }
            0b11 => {
                log::debug!(
                    "ACPI/IRQ: legacy IRQ {} -> GSI {} trigger = Level",
                    source,
                    gsi
                );
                Trigger::Level
            }
            value => {
                log::error!(
                    "ACPI/IRQ: invalid trigger encoding {:#b} for IRQ {} -> GSI {}",
                    value,
                    source,
                    gsi
                );
                unreachable!();
            }
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
            log::warn!(
                "ACPI/IRQ: no interrupt controller found for legacy IRQ {} -> GSI {}",
                source,
                gsi
            );
            continue;
        };

        log::debug!("ACPI/IRQ: found interrupt controller for GSI {}", gsi);

        let res = device.resources[1].clone();

        let id = match res {
            Resource::InterruptController { id, .. } => id,
            _ => {
                log::error!(
                    "ACPI/IRQ: matched device does not have expected InterruptController resource"
                );
                unreachable!();
            }
        };

        let number = *gsi as u128 | (*source as u128) << 32;

        log::debug!(
            "ACPI/IRQ: creating override resource: controller = {:?}, number = {:#x}, polarity = {:?}, trigger = {:?}",
            id,
            number,
            polarity,
            trigger
        );

        device.resources.push(Resource::InterruptOverride {
            id,
            number,
            polarity,
            trigger,
        });

        log::info!("ACPI/IRQ: installed override IRQ {} -> GSI {}", source, gsi);
    }

    log::debug!("ACPI/IRQ: interrupt source override processing complete");
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
