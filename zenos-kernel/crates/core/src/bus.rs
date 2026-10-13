use crate::scheduler::sync::Mutex;
use device_core::device_manager::DeviceManager;

pub static DEVICE_MANAGER: Mutex<DeviceManager> = Mutex::new(DeviceManager::new());

pub fn init() {
    let mut lock = DEVICE_MANAGER.lock();

    lock.enumerate_devices().unwrap();
    log::info!("lock: {:#?}", lock);
    lock.bind_drivers();

    todo!();
}
