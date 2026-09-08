use self::lapic::LocalApic;
use crate::{
    arch::{
        CLOCKSOURCE, PhysAddr,
        common::interrupts::{
            IrqError,
            controller::{Clockevent, InterruptController},
        },
        ports::WriteOnlyPort,
    },
    firmware::{Device, DeviceClass, DeviceId, Resource, RuntimeBootInfo},
    mm,
};
use alloc::vec::Vec;
use kprimitives::alloc::{CreatableKernelObject, KernelObject, boxed::KBox};

mod ioapic;
mod lapic;
mod mmio;

struct XapicController {
    ioapics: Vec<ioapic::IoApic>,
    lapic: KBox<LocalApic, mm::GlobalAllocator>,
}

impl KernelObject for XapicController {}
impl CreatableKernelObject for XapicController {
    type Allocator = mm::GlobalAllocator;
}

impl XapicController {
    pub const fn new(
        ioapics: Vec<ioapic::IoApic>,
        lapic: KBox<LocalApic, mm::GlobalAllocator>,
    ) -> Self {
        Self { ioapics, lapic }
    }
}

impl InterruptController for XapicController {
    fn enable(
        &self,
        number: u32,
        trigger: crate::firmware::Trigger,
        polarity: crate::firmware::Polarity,
        irq: u32,
    ) -> Result<(), IrqError> {
        let ioapic = self.ioapics.iter().find(|ioapic| {
            let (max, min) = ioapic.range();
            min <= number && number <= max
        });
        if let Some(ioapic) = ioapic {
            ioapic.set_redirection(number, irq as u8, trigger, polarity, false, 0)?;
        } else {
            return Err(IrqError::OutOfRange);
        }

        Ok(())
    }

    fn disable(&self, number: u32) -> u32 {
        let ioapic = self.ioapics.iter().find(|ioapic| {
            let (max, min) = ioapic.range();
            min <= number && number <= max
        });
        if let Some(ioapic) = ioapic {
            let Ok(redir) = ioapic.get_redirection(number) else {
                return 0;
            };
            ioapic
                .set_redirection(number, redir.0, redir.1, redir.2, true, redir.4)
                .ok();
            redir.0 as u32
        } else {
            0
        }
    }

    fn send_eoi(&self) {
        self.lapic.send_eoi()
    }

    fn timer(&self) -> KBox<dyn Clockevent, mm::GlobalAllocator> {
        self.lapic.clone()
    }
}

pub fn init(bootdata: &RuntimeBootInfo) {
    // disable legacy pics
    unsafe {
        let pic1_data = WriteOnlyPort::new(0x21);
        let pic2_data = WriteOnlyPort::new(0xA1);
        pic1_data.write(0xffu8);
        pic2_data.write(0xffu8)
    }

    // locate lapics
    let lapic_base = bootdata
        .devices
        .iter()
        .find(|d| d.device_id() == DeviceId::new(DeviceClass::InterruptController, b"LocalApic"))
        .and_then(|d| {
            d.resources().iter().find_map(|r| {
                if let crate::firmware::Resource::MmioRegion { address, .. } = r {
                    Some(*address)
                } else {
                    None
                }
            })
        })
        .expect("Local Apic not found");

    let clocksource = CLOCKSOURCE.read();
    let mut lapic = LocalApic::new();
    lapic
        .configure(clocksource.as_ref().unwrap(), lapic_base)
        .unwrap();

    let mut iovec = Vec::new();
    // get a list of all ioapics
    let ioapics = bootdata
        .devices
        .iter()
        .filter(|d| d.device_id() == DeviceId::new(DeviceClass::InterruptController, b"IoApic"))
        .map(|d| d.resources());

    for ioapic in ioapics {
        let ioapic_base = &ioapic[0]; // guaranteed to be mmio region,
        let ioapic_intcontroller = &ioapic[1];

        match ioapic_intcontroller {
            Resource::InterruptController { id, interrupts, .. } => {
                let mut ioapic_s = ioapic::IoApic::new(*interrupts as u32);
                let mmio = match ioapic_base {
                    Resource::MmioRegion { address, .. } => address,
                    _ => unreachable!(),
                };

                let isr_overrides = ioapic[2..].iter().map(|r| match r {
                    Resource::InterruptOverride {
                        number,
                        polarity,
                        trigger,
                        ..
                    } => {
                        let source = (*number >> 32) as u32;
                        let target = (*number & 0xFFFFFFFF) as u32;
                        ioapic::IoApicOverride {
                            gsi_source: source,
                            gsi_target: target,
                            flag: (trigger.clone(), polarity.clone()),
                        }
                    }
                    _ => unreachable!(),
                });

                log::info!(
                    "configuring ioapic_s: mmio={:#x}, id={:?}",
                    mmio.as_usize(),
                    *id
                );

                let boot_cpu_apic_id = bootdata
                    .platform
                    .boot_cpu_id
                    .map(|id| (id >> 8) as u8)
                    .unwrap_or(0);

                ioapic_s
                    .configure(*mmio, isr_overrides, boot_cpu_apic_id)
                    .unwrap();

                log::info!(
                    "ioapic_s configured: mmio={:#x}, id={:?}",
                    mmio.as_usize(),
                    *id
                );

                iovec.push(ioapic_s);
            }
            _ => {
                unreachable!()
            }
        }
    }

    let controller = XapicController::new(iovec, KBox::new(lapic).unwrap());
    *super::super::INTERRUPT_CONTROLLER.write() = Some(KBox::new(controller).unwrap());
}
