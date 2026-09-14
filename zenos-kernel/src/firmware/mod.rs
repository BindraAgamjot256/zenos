#![allow(dead_code)]

use crate::arch::PhysAddr;
use alloc::string::String;
use alloc::{boxed::Box, vec::Vec};
use bootloader_api::BootInfo;
use core::fmt::Debug;
use uuid::Uuid;

pub mod acpi;

#[derive(Default)]
pub struct RuntimeBootInfo {
    /// The devices found during boot.
    pub devices: Vec<Box<dyn Device>>,
    /// The platform information.
    pub platform: Platform,
    /// Function that expands the device tree, to add all available devices.
    expand_device_tree: Option<Box<dyn Fn(Self) -> Self>>,
}

impl RuntimeBootInfo {
    pub fn expand(mut self) -> Self {
        if let Some(expand) = self.expand_device_tree.take() {
            expand(self)
        } else {
            self
        }
    }
}

impl Debug for RuntimeBootInfo {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("RuntimeBootInfo")
            .field("devices", &self.devices)
            .field("platform", &self.platform)
            .field(
                "expand_device_tree",
                if let Some(_) = self.expand_device_tree {
                    &"Some(Box<dyn Fn(Self) -> Self>)"
                } else {
                    &"None"
                },
            )
            .finish()
    }
}

/// Information about the computer itself.
#[derive(Default, Debug)]
pub struct Platform {
    /// Motherboard manifacturer.
    pub oem_id: Option<String>,
    /// The CPUs found during boot.
    pub cpus: Vec<Cpu>,
    /// The CPU ID of the boot CPU.
    pub boot_cpu_id: Option<u16>,
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

    pub fn uuid_namespace_block() -> Uuid {
        Uuid::new_v5(&Uuid::nil(), b"Block")
    }

    pub fn uuid_namespace_network() -> Uuid {
        Uuid::new_v5(&Uuid::nil(), b"Network")
    }

    /// Creates a new device ID from the given namespace and name.
    ///
    /// The class of the device is determined from the namespace.
    /// Each [`DeviceClass`] has a unique namespace, defined as the function `uuid_namespace_{class}` (for eg: [`uuid_namespace_interrupt_controller`]).
    pub fn new(class: DeviceClass, name: &[u8]) -> Self {
        let namespace = match class {
            DeviceClass::InterruptController => Self::uuid_namespace_interrupt_controller(),
            DeviceClass::Timer => Self::uuid_namespace_timer(),
            DeviceClass::Block => Self::uuid_namespace_block(),
            DeviceClass::Network => Self::uuid_namespace_network(),
            _ => Uuid::nil(),
        };
        let uuid = Uuid::new_v5(&namespace, name);
        Self { uuid, class }
    }
}

/// The class of a device.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
#[non_exhaustive]
pub enum DeviceClass {
    /// A timer device(HPET, ACPI PM Timer, etc).
    Timer,
    /// An interrupt controller device (PIC, IOAPIC etc).
    InterruptController,
    /// A block device (e.g. disk, CD-ROM, etc).
    Block,
    /// A network device (e.g. Ethernet, Wi-Fi, etc).
    Network,
    /// An unknown device class (i.e I was too lazy to add it in.).
    #[default]
    Unknown,
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
#[repr(u8)]
pub enum Trigger {
    Edge = 0,
    Level = 1,
}

/// The polarity of an interrupt.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Ord, PartialOrd)]
#[repr(u8)]
pub enum Polarity {
    ActiveHigh = 0,
    ActiveLow = 1,
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
