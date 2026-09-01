use crate::arch::PhysAddr;
use alloc::string::String;
use alloc::{boxed::Box, vec::Vec};
use bootloader_api::BootInfo;
use core::fmt::Debug;
use uuid::Uuid;

pub mod acpi;

#[derive(Default, Debug)]
pub struct RuntimeBootInfo {
    /// The devices found during boot.
    pub devices: Vec<Box<dyn Device>>,
    /// The platform information.
    pub platform: Platform,
}

/// Information about the computer itself.
#[derive(Default, Debug)]
pub struct Platform {
    /// Motherboard manifacturer.
    pub oem_id: Option<String>,
    /// The CPUs found during boot.
    pub cpus: Vec<Cpu>,
}

/// A device found during boot.
pub trait Device: Debug {
    /// The device's unique identifier.
    fn device_id(&self) -> DeviceId;
    /// The resources used by the device.
    fn resources(&self) -> &[Resource];
}

/// The unique identifier for a device.
///
/// This struct is opaque, compare [`DeviceId`] for equality,
/// if you wish to support a certain device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct DeviceId {
    uuid: Uuid,
    class: DeviceClass,
}

impl DeviceId {
    pub fn uuid_namespace_interrupt_controller() -> Uuid {
        Uuid::new_v5(&Uuid::nil(), b"InterruptController")
    }

    pub fn uuid_namespace_timer() -> Uuid {
        Uuid::new_v5(&Uuid::nil(), b"Timer")
    }

    /// Creates a new device ID from the given namespace and name.
    ///
    /// The class of the device is determined from the namespace.
    /// Each [`DeviceClass`] has a unique namespace, defined as the function `uuid_namespace_{class}` (for eg: [`uuid_namespace_interrupt_controller`]).
    pub fn new(namespace: Uuid, name: &[u8]) -> Self {
        let uuid = Uuid::new_v5(&namespace, name);
        let class = if namespace == Self::uuid_namespace_interrupt_controller() {
            DeviceClass::InterruptController
        } else if namespace == Self::uuid_namespace_timer() {
            DeviceClass::Timer
        } else {
            DeviceClass::Unknown
        };
        Self { uuid, class }
    }
}

/// The class of a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
enum DeviceClass {
    /// A timer device(HPET, ACPI PM Timer, etc).
    Timer,
    /// An interrupt controller device (PIC, IOAPIC etc).
    ///
    /// Note:
    /// Lapic is not stored here, since it is a part of the cpu itself,
    /// and thus belongs to the Platform struct.
    InterruptController,
    #[default]
    /// An unknown device class (i.e I was too lazy to add it in.).
    Unknown,
    // TODO: More classes as needed.
}

/// A resource used by a device.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Resource {
    /// A memory-mapped I/O region.
    MmioRegion { address: PhysAddr, size: usize },
    /// An I/O port.
    IoPort { base: u16, size: u16 },
    /// An interrupt.
    Interrupt {
        controller: InterruptControllerId,
        number: u32,
        trigger: Trigger,
        polarity: Polarity,
    },
    /// A Interrupt controller, that can manage interrupts.
    InterruptController {
        id: InterruptControllerId,
        /// arbitrary token that is used by the controller to identify
        /// how many interrupts it manages.
        interrupts: u64,
        name: Option<&'static str>,
    },
    InterruptOverride {
        id: InterruptControllerId,
        /// Arbitrary token used to override the interrupt.
        /// useful only to the interrupt controller.
        number: u128,
        trigger: Trigger,
        polarity: Polarity,
    },
}

/// The trigger mode of an interrupt.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Trigger {
    Level,
    Edge,
}

/// The polarity of an interrupt.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub enum Polarity {
    ActiveHigh,
    ActiveLow,
}

/// A CPU.
#[derive(Debug)]
pub struct Cpu {
    pub cpu_id: u8,
    pub status: Status,
}

#[derive(Debug)]
#[repr(u8)]
pub enum Status {
    Online,
    Offline,
    Unplugged,
    Unknown,
}

/// Opaque identifier for an interrupt controller.
///
/// On x86, this is the IOAPIC ID.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
pub struct InterruptControllerId(u16);

pub fn init(boot_info: &'static BootInfo) -> RuntimeBootInfo {
    let mut populate = RuntimeBootInfo::default();
    if let Some(rsdp) = boot_info.rsdp_addr.into_option().map(|a| a as usize) {
        let res = acpi::populate(&mut populate, rsdp);
        if res.is_some() {
            return populate;
        }
    }
    // TODO: populate using other methods also, like devicetree on arm.

    panic!("population");
}
