use alloc::string::String;
use core::fmt::Debug;
use kprimitives::alloc::{GlobalAllocator, KernelObject, RefCounted, boxed::KBox};

/// Errors that may occur while enumerating devices on a bus.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum BusError {
    /// The bus failed to enumerate its devices.
    EnumerationFailed,
}

/// A bus that can discover and enumerate connected devices.
///
/// A bus is responsible for discovering devices attached to it and
/// providing them to the kernel's device management subsystem.
///
/// Implementations may represent physical buses, such as PCI, or
/// firmware-defined buses, such as ACPI.
///
/// Buses must be thread-safe and support kernel object semantics.
pub trait Bus: KernelObject + Send + Sync + Debug {
    /// Returns the name of the bus.
    ///
    /// The returned string must have a static lifetime.
    fn name(&self) -> &'static str;

    /// Enumerates devices connected to this bus.
    ///
    /// Each discovered device is passed to `callback` as an owned,
    /// allocator-backed kernel object.
    ///
    /// The callback may store the device for later driver matching
    /// and initialization.
    ///
    /// # Errors
    ///
    /// Returns [`BusError`] if enumeration fails.
    ///
    /// Devices passed to the callback before an error occurs are
    /// not automatically rolled back.
    fn enumerate(
        &mut self,
        callback: &mut dyn FnMut(KBox<dyn Device, GlobalAllocator>),
    ) -> Result<(), BusError>;
}

/// A hardware or firmware identifier associated with a device.
///
/// Device identifiers are used to determine which drivers may
/// support a particular device.
///
/// A device may expose multiple identifiers through [`Device::ids`].
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum DeviceId {
    /// An ACPI hardware or compatible identifier.
    ///
    /// For example, `"PNP0303"` identifies a standard keyboard
    /// controller.
    AcpiId(String),

    /// A PCI identifier consisting of a vendor ID and device ID.
    ///
    /// Both values are 16-bit identifiers assigned within the
    /// PCI device identification scheme.
    PciId {
        /// Identifies the PCI device vendor.
        vendor_id: u16,

        /// Identifies the device within the vendor's namespace.
        device_id: u16,
    },
}

/// A pattern used to match device identifiers against driver support.
///
/// Unlike [`DeviceId`], a match pattern may contain wildcards
/// for PCI vendor and device identifiers.
///
/// A `None` value in a PCI field matches any value for that field.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DeviceMatch {
    /// Matches an exact ACPI identifier.
    ///
    /// ACPI identifiers are compared using case-sensitive
    /// string equality.
    AcpiId(&'static str),

    /// Matches a PCI vendor and device identifier.
    ///
    /// Either field may be `None` to indicate a wildcard.
    PciId {
        /// The required vendor ID, or `None` for any vendor.
        vendor_id: Option<u16>,

        /// The required device ID, or `None` for any device.
        device_id: Option<u16>,
    },
}

impl DeviceMatch {
    /// Checks whether this pattern matches a device identifier.
    ///
    /// ACPI patterns match only ACPI identifiers, and PCI patterns
    /// match only PCI identifiers.
    ///
    /// For PCI identifiers, each specified field must match the
    /// corresponding field of the device. Unspecified fields
    /// match any value.
    ///
    /// # Examples
    ///
    /// ```
    /// # extern crate alloc;
    /// # use alloc::string::ToString;
    /// # use device_core::bus::{DeviceId, DeviceMatch};
    ///
    /// let pattern = DeviceMatch::PciId {
    ///     vendor_id: Some(0x8086),
    ///     device_id: None,
    /// };
    ///
    /// let device = DeviceId::PciId {
    ///     vendor_id: 0x8086,
    ///     device_id: 0x1234,
    /// };
    ///
    /// assert!(pattern.matches(&device));
    /// ```
    ///
    /// # Returns
    ///
    /// `true` if the identifier satisfies the pattern,
    /// or `false` otherwise.
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

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum DeviceResource {
    MMIO {
        base: usize,
        len: usize,
    },
    IoPort {
        base: usize,
        len: usize,
    },
    IRQ {
        number: u32,
        triggering: u8,
        polarity: u8,
        sharing: u8,
        wake_capability: u8,
    },
    ExtendedIRQ {
        number: u32,
        triggering: u8,
        polarity: u8,
        sharing: u8,
        wake_capability: u8,
    },
}

/// A device discovered by a bus.
///
/// Devices expose identifying information that allows the kernel
/// to locate compatible drivers.
///
/// Some devices may also act as buses, allowing the kernel to
/// discover additional devices through a hierarchical topology.
///
/// Implementations must support reference counting and be safe
/// to share across threads.
pub trait Device: RefCounted + Send + Debug {
    /// Returns the human-readable name of the device.
    ///
    /// The returned string is borrowed from the device and
    /// remains valid for the duration of the borrow.
    fn name(&self) -> &str;

    /// Returns the identifiers associated with this device.
    ///
    /// Drivers may use these identifiers to determine whether
    /// they support the device.
    ///
    /// A device may provide multiple identifiers, such as
    /// multiple ACPI compatible IDs.
    fn ids(&self) -> &[DeviceId];

    /// Returns a bus interface if this device also acts as a bus.
    ///
    /// This allows devices such as bridges and controllers to
    /// expose additional devices for enumeration.
    ///
    /// # Returns
    ///
    /// - `Some(bus)` if the device provides a bus interface.
    /// - `None` if the device does not act as a bus.
    ///
    /// The returned bus is represented by an owned [`KBox`].
    fn as_bus(&self) -> Option<KBox<dyn Bus, GlobalAllocator>>;

    /// Returns a slice of resources owned by this device
    fn resources(&self) -> &[DeviceResource];
}

/// Errors that may occur while probing a device with a driver.
pub enum DriverError {
    /// The driver encountered an error while probing or
    /// initializing the device.
    ProbeFailed,

    /// The driver does not support the provided device.
    UnsupportedDevice,
}

/// A driver capable of probing and initializing supported devices.
///
/// Drivers declare the device identifiers they support and provide
/// a probe operation for initializing compatible hardware.
///
/// The kernel's driver management subsystem can compare a device's
/// identifiers against [`Driver::supported_ids`] before attempting
/// to probe the device.
///
/// Implementations must support reference counting and be safe
/// to share across threads.
pub trait Driver: RefCounted + Send + Sync + Debug {
    /// Returns the name of the driver.
    ///
    /// The returned string must have a static lifetime.
    fn name(&self) -> &'static str;

    /// Returns the device identifier patterns supported by
    /// this driver.
    ///
    /// Each pattern describes a device or group of devices
    /// that the driver can potentially handle.
    ///
    /// PCI patterns may contain wildcard fields.
    ///
    /// The returned slice must have a static lifetime.
    fn supported_ids(&self) -> &'static [DeviceMatch];

    /// Attempts to probe and initialize a device.
    ///
    /// The device is provided as a shared reference. Any
    /// persistent state or ownership required by the driver
    /// must be managed by its implementation.
    ///
    /// # Errors
    ///
    /// Returns [`DriverError::UnsupportedDevice`] if the
    /// device is not supported.
    ///
    /// Returns [`DriverError::ProbeFailed`] if probing or
    /// initialization fails.
    ///
    /// # Returns
    ///
    /// `Ok(())` if probing completes successfully.
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
        const fn assert_dyn_compatible<T: ?Sized>() {}

        assert_dyn_compatible::<dyn Bus>();
        assert_dyn_compatible::<dyn Device>();
        assert_dyn_compatible::<dyn Driver>();
    };
}
