use alloc::string::String;
use core::fmt::Debug;
use kprimitives::alloc::{GlobalAllocator, KernelObject, RefCounted, boxed::KBox};

pub enum BusError {
    EnumerationFailed,
}

pub trait Bus: KernelObject + Send + Sync + Debug {
    fn name(&self) -> &'static str;

    fn enumerate(
        &mut self,
        callback: &mut dyn FnMut(KBox<dyn Device, GlobalAllocator>),
    ) -> Result<(), BusError>;
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceId {
    AcpiId(String),

    PciId { vendor_id: u16, device_id: u16 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceMatch {
    AcpiId(&'static str),

    PciId {
        vendor_id: Option<u16>,
        device_id: Option<u16>,
    },
}

impl DeviceMatch {
    pub fn matches(&self, device_id: &DeviceId) -> bool {
        match (self, device_id) {
            (DeviceMatch::AcpiId(pattern), DeviceId::AcpiId(id)) => pattern == &id.as_str(),

            (
                DeviceMatch::PciId {
                    vendor_id: pattern_vendor,
                    device_id: pattern_device,
                },
                DeviceId::PciId {
                    vendor_id: id_vendor,
                    device_id: id_device,
                },
            ) => {
                pattern_vendor.is_none_or(|v| v == *id_vendor)
                    && pattern_device.is_none_or(|v| v == *id_device)
            }

            _ => false,
        }
    }
}

pub trait Device: RefCounted + Send + Sync + Debug {
    fn name(&self) -> &str;

    fn ids(&self) -> &[DeviceId];

    fn as_bus(&self) -> Option<KBox<dyn Bus, GlobalAllocator>>;
}

pub enum DriverError {
    ProbeFailed,
    UnsupportedDevice,
}

pub trait Driver: RefCounted + Send + Sync + Debug {
    fn name(&self) -> &'static str;

    fn supported_ids(&self) -> &'static [DeviceMatch];

    fn probe(&self, device: &dyn Device) -> Result<(), DriverError>;
}

#[cfg(test)]
mod tests {
    use super::*;

    extern crate alloc;

    use alloc::string::ToString;

    #[test]
    fn pci_vendor_wildcard_matches() {
        let pattern = DeviceMatch::PciId {
            vendor_id: Some(0x8086),
            device_id: None,
        };

        let device = DeviceId::PciId {
            vendor_id: 0x8086,
            device_id: 0x1234,
        };

        assert!(pattern.matches(&device));
    }

    #[test]
    fn unrelated_buses_do_not_match() {
        let pattern = DeviceMatch::AcpiId("PNP0303");

        let device = DeviceId::PciId {
            vendor_id: 0x8086,
            device_id: 0x1234,
        };

        assert!(!pattern.matches(&device));
    }

    #[test]
    fn acpi_hardware_id_matches() {
        let pattern = DeviceMatch::AcpiId("PNP0303");

        let device = DeviceId::AcpiId("PNP0303".to_string());

        assert!(pattern.matches(&device));
    }

    #[test]
    fn acpi_different_id_does_not_match() {
        let pattern = DeviceMatch::AcpiId("PNP0303");

        let device = DeviceId::AcpiId("PNP0C0F".to_string());

        assert!(!pattern.matches(&device));
    }

    const _: () = {
        const fn assert_dyn_compatible<T: ?Sized + Send + Sync>() {}

        assert_dyn_compatible::<dyn Bus>();
        assert_dyn_compatible::<dyn Device>();
        assert_dyn_compatible::<dyn Driver>();
    };
}
