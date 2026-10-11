use crate::scheduler::sync::Mutex;
use device_prelude::device_manager::DeviceManager;

mod acpi;

pub static DEVICE_MANAGER: Mutex<DeviceManager> = Mutex::new(DeviceManager::new());

pub fn init() {
    let mut lock = DEVICE_MANAGER.lock();

    if let Ok(acpi) = acpi::init() {
        lock.register_bus(acpi);
    }

    lock.enumerate_devices().unwrap();
    lock.bind_drivers();

    todo!();
}
