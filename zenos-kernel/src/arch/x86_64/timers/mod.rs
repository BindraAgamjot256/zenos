use crate::{
    arch::{timers::Clocksource, x86_64::timers::hpet::HpetTimer},
    firmware::{self, DeviceId, Resource},
    mm,
};
use kprimitives::{alloc::boxed::KBox, rwlock::RwLock};

mod hpet;
mod tsc;

pub static CLOCKSOURCE: RwLock<Option<KBox<dyn Clocksource + Send + Sync, mm::GlobalAllocator>>> =
    RwLock::new(None);

fn install_clocksource<T>(clocksource: KBox<T, mm::GlobalAllocator>)
where
    T: Clocksource + Send + Sync + 'static,
{
    let mut guard = CLOCKSOURCE.write();
    *guard = Some(clocksource);
}

#[inline]
fn init_tsc() -> Result<(), ()> {
    let mut tsc = KBox::new(tsc::TscTimer::new()).map_err(|err| {
        log::warn!("failed to allocate TSC timer: {:?}", err);
    })?;

    tsc.configure().map_err(|err| {
        log::warn!("failed to configure TSC timer: {:?}", err);
    })?;

    install_clocksource(tsc);

    Ok(())
}

#[inline]
fn init_hpet(bootdata: &firmware::RuntimeBootInfo) -> Result<(), ()> {
    let hpet_device = bootdata
        .devices
        .iter()
        .find(|d| d.device_id() == DeviceId::new(DeviceId::uuid_namespace_timer(), b"HPET"))
        .ok_or_else(|| log::warn!("Failed to find HPET device"))?;
    let hpet_address = hpet_device
        .resources()
        .iter()
        .find(|r| matches!(r, Resource::MmioRegion { .. }))
        .ok_or_else(|| log::warn!("Failed to find HPET address"))?;
    
    let hpet_address = match hpet_address {
        Resource::MmioRegion { address, .. } => address.as_usize(),
        _ => return Err(()),
    };
    
    let mut hpet = HpetTimer::new(hpet_address, cfg!(target_arch = "x86_64"));
    hpet.configure()
        .map_err(|e| log::warn!("failed to configure HPET timer: {:?}", e))?;
    install_clocksource(
        KBox::new(hpet).map_err(|e| log::warn!("failed to allocate HPET timer: {:?}", e))?,
    );
    Ok(())
}

pub fn init(bootdata: &firmware::RuntimeBootInfo) {
    if init_tsc().is_ok() {
        return;
    }

    log::warn!("failed to initialize TSC timer, trying HPET");

    if init_hpet(bootdata).is_ok() {
        return;
    }

    log::warn!("failed to initialize HPET timer, giving up");

    panic!("FUCK YOU ANCIENT COMPUTER HISTORY");
}
