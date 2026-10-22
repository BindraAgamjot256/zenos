#![no_std]
#![deny(unsafe_op_in_unsafe_fn)]
// TODO #1: pass inputs into a single unified TTY subsystem or somth.
// TODO #2: use a centeralise port locking mechanism to reserve the ports.
// TODO #3: add PS/2 mouse (MOU_) support also

use core::sync::atomic::{AtomicUsize, Ordering};
use kcore::{
    arch::{
        CpuContext,
        interrupts::InterruptHandlerRet,
        ports::{Port, ReadOnly, ReadWrite},
    },
    device_core::bus::{Device, DeviceMatch, DeviceResource, Driver, DriverError},
    irq::IRQ_CONTROLLER,
    log,
};
use kprimitives::alloc::{
    CreatableKernelObject, GlobalAllocator, KernelObject, RefCounted, boxed::KBox,
};

#[derive(Debug)]
pub struct Ps2KeyboardDriver {
    refs: AtomicUsize,
}

impl Ps2KeyboardDriver {
    pub const fn new() -> Self {
        Self {
            refs: AtomicUsize::new(1),
        }
    }
}

impl Default for Ps2KeyboardDriver {
    fn default() -> Self {
        Self::new()
    }
}
static SUPPORTED_IDS: &[DeviceMatch] = &[
    DeviceMatch::AcpiId("PNP0303"),
    DeviceMatch::AcpiId("PNP030B"),
    DeviceMatch::AcpiId("PNP0F13"),
];
static SHIFT: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

impl KernelObject for Ps2KeyboardDriver {}
impl CreatableKernelObject for Ps2KeyboardDriver {
    type Allocator = GlobalAllocator;
}
unsafe impl RefCounted for Ps2KeyboardDriver {
    fn ref_count(&self) -> usize {
        self.refs.load(Ordering::Relaxed)
    }
    fn inc_refcount(&self, order: Ordering) -> usize {
        self.refs.fetch_add(1, order) + 1
    }
    fn dec_refcount(&self, order: Ordering) -> usize {
        self.refs.fetch_sub(1, order) - 1
    }
}

impl Driver for Ps2KeyboardDriver {
    fn name(&self) -> &'static str {
        "ps2-keyboard"
    }

    fn supported_ids(&self) -> &'static [DeviceMatch] {
        SUPPORTED_IDS
    }

    fn probe(&self, device: &dyn Device) -> Result<(), DriverError> {
        log::info!("PS/2 keyboard candidate: {}", device.name());
        let resources = device.resources();
        for resource in resources {
            log::info!("  resource: {:?}", resource);
        }

        let mut io_base = None;
        let mut irq_number = None;
        for resource in resources {
            match resource {
                DeviceResource::IoPort { base, len } if *len >= 1 && *base <= u16::MAX as usize => {
                    if io_base.is_none() || *base == 0x60 {
                        io_base = Some(*base as u16);
                    }
                }
                DeviceResource::IRQ { number, .. } | DeviceResource::ExtendedIRQ { number, .. } => {
                    irq_number = Some(*number);
                }
                _ => {}
            }
        }
        let (Some(data_port), Some(irq_number)) = (io_base, irq_number) else {
            log::warn!("PS/2 keyboard missing usable I/O port or IRQ resource");
            return Err(DriverError::ProbeFailed);
        };

        log::info!(
            "registering PS/2 keyboard resources: data={:#x}, IRQ={}",
            data_port,
            irq_number
        );
        let controller = IRQ_CONTROLLER.write();
        let mut irq = controller
            .set_number(irq_number)
            .ok_or(DriverError::ProbeFailed)?;
        irq.set_handler(keyboard_irq);
        core::mem::forget(irq);
        Ok(())
    }
}

fn keyboard_irq(_: &mut CpuContext) -> InterruptHandlerRet {
    let status: Port<u8, ReadOnly> = unsafe { Port::new(0x64) };
    let data: Port<u8, ReadWrite> = unsafe { Port::new(0x60) };
    if unsafe { status.read() } & 1 != 0 {
        let code = unsafe { data.read() };
        match code {
            0x2a | 0x36 => SHIFT.store(true, Ordering::Relaxed),
            0xaa | 0xb6 => SHIFT.store(false, Ordering::Relaxed),
            code if code & 0x80 == 0 => match translate(code, SHIFT.load(Ordering::Relaxed)) {
                Some(ch) => log::info!("keyboard: {}", ch),
                None => log::info!("keyboard scancode: {:#x}", code),
            },
            _ => {}
        }
    }
    IRQ_CONTROLLER.read().send_eoi();
    InterruptHandlerRet::Handled
}

fn translate(code: u8, shift: bool) -> Option<char> {
    let (plain, shifted) = match code {
        0x02..=0x0b => (
            b"1234567890"[(code - 0x02) as usize],
            b"!@#$%^&*()"[(code - 0x02) as usize],
        ),
        0x10..=0x19 => (
            b"qwertyuiop"[(code - 0x10) as usize],
            b"QWERTYUIOP"[(code - 0x10) as usize],
        ),
        0x1e..=0x26 => (
            b"asdfghjkl"[(code - 0x1e) as usize],
            b"ASDFGHJKL"[(code - 0x1e) as usize],
        ),
        0x2c..=0x32 => (
            b"zxcvbnm"[(code - 0x2c) as usize],
            b"ZXCVBNM"[(code - 0x2c) as usize],
        ),
        0x39 => (b' ', b' '),
        0x1c => (b'\n', b'\n'),
        _ => return None,
    };
    Some(if shift { shifted } else { plain } as char)
}

pub fn init() -> Result<KBox<Ps2KeyboardDriver, GlobalAllocator>, &'static str> {
    KBox::new(Ps2KeyboardDriver::new()).map_err(|_| "Memory allocation failed")
}
