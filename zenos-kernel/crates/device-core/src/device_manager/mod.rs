use crate::bus::{Bus, Device, Driver};
use alloc::vec::Vec;
use kprimitives::alloc::{GlobalAllocator, boxed::KBox, sync::KArc};

#[derive(Debug)]
pub struct DeviceManager {
    buses: Vec<KBox<dyn Bus, GlobalAllocator>>,
    drivers: Vec<KArc<dyn Driver, GlobalAllocator>>,
    devices: Vec<DeviceEntry>,
}

#[derive(Debug)]
struct DeviceEntry {
    device: KArc<dyn Device, GlobalAllocator>,
    driver: Option<KArc<dyn Driver, GlobalAllocator>>,
}

impl DeviceManager {
    pub const fn new() -> Self {
        Self {
            buses: Vec::new(),
            drivers: Vec::new(),
            devices: Vec::new(),
        }
    }

    pub fn register_device(&mut self, device: KArc<dyn Device, GlobalAllocator>) {
        let entry = DeviceEntry {
            device,
            driver: None,
        };
        self.devices.push(entry);
    }

    pub fn register_driver(&mut self, driver: KArc<dyn Driver, GlobalAllocator>) {
        self.drivers.push(driver);
    }

    pub fn register_bus(&mut self, bus: KBox<dyn Bus, GlobalAllocator>) {
        self.buses.push(bus);
    }

    pub fn bind_drivers(&mut self) {
        for entry in self.devices.iter_mut() {
            if entry.driver.is_some() {
                continue;
            }

            for drv in &self.drivers {
                let matches = drv
                    .supported_ids()
                    .iter()
                    .any(|matcher| entry.device.ids().iter().any(|id| matcher.matches(id)));

                if !matches {
                    continue;
                }

                if drv.probe(&*entry.device).is_ok() {
                    entry.driver = Some(drv.clone());
                    break;
                }
            }
        }
    }

    pub fn enumerate_devices(&mut self) -> Result<(), &'static str> {
        let mut index = 0;

        while index < self.buses.len() {
            let mut new_buses = Vec::new();
            let mut new_devices = Vec::new();

            self.buses[index]
                .enumerate(&mut |device| {
                    if let Some(bus) = device.as_bus() {
                        new_buses.push(bus);
                    } else {
                        new_devices.push(device.into_arc());
                    }
                })
                .map_err(|_| "Failed to enumerate devices")?;

            self.buses.extend(new_buses);

            for device in new_devices {
                self.register_device(device);
            }

            index += 1;
        }
        Ok(())
    }
}

impl Default for DeviceManager {
    fn default() -> Self {
        Self::new()
    }
}
