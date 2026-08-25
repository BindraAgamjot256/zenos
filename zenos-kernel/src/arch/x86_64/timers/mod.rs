use crate::{arch::timers::Clocksource, firmware, mm};
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
    let Some(hpet_info) = bootdata.hpet.as_ref() else {
        log::warn!("HPET is not available in firmware boot data");
        return Err(());
    };

    if !hpet_info.bits_64 {
        log::warn!("HPET timer is not 64-bit, giving up(I don't wanna support it :sob:)");
        return Err(());
    }

    let mut hpet =
        KBox::new(hpet::HpetTimer::new(hpet_info.address, hpet_info.bits_64)).map_err(|err| {
            log::warn!("failed to allocate HPET timer: {:?}", err);
        })?;

    hpet.configure().map_err(|err| {
        log::warn!("failed to configure HPET timer: {:?}", err);
    })?;

    install_clocksource(hpet);

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
