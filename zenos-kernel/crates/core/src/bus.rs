use crate::scheduler::sync::Mutex;
use device_core::device_manager::DeviceManager;

pub static DEVICE_MANAGER: Mutex<DeviceManager> = Mutex::new(DeviceManager::new());

pub fn init() {
    let mut lock = DEVICE_MANAGER.lock();

    lock.enumerate_devices().unwrap();
    log::info!("lock: {:#?}", lock);
    lock.bind_drivers();

    loop {
        crate::halt_and_catch_fire!()
        //todo: don't do this. Stop it. Get some help.
        // McDonald's wants you to give yourself a chance,
        // a chance to find out the wonderful things you can really be
    }
}
