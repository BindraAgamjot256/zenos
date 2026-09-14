use super::mmio::IoApicRegs;
use crate::{
    arch::{PhysAddr, common::interrupts::IrqError, mem::ioremap},
    firmware::{Polarity, Trigger},
};
use alloc::vec::Vec;

pub struct IoApic {
    base_addr: usize,
    gsi_base: u32,
    gsi_max: u32,
    overrides: Vec<IoApicOverride>,
}

pub struct IoApicOverride {
    pub gsi_source: u32,
    pub gsi_target: u32,
    pub flag: (Trigger, Polarity),
}

impl IoApic {
    const IOAPICVER: u32 = 0x1;

    pub fn new(gsi_base: u32) -> Self {
        Self {
            base_addr: 0,
            gsi_base,
            gsi_max: gsi_base,
            overrides: Vec::new(),
        }
    }

    pub const fn ioredtbl(&self, pin: u32) -> (u32, u32) {
        let lo = pin * 2 + 0x10;
        let hi = lo + 1;
        (lo, hi)
    }

    fn regs(&self) -> *mut IoApicRegs {
        core::ptr::with_exposed_provenance_mut(self.base_addr)
    }

    fn write_reg(&self, reg: u32, value: u32) {
        let regs = self.regs();
        unsafe {
            (&raw mut (*regs).ioregsel).write_volatile(reg);
        }
        unsafe {
            (&raw mut (*regs).iowin).write_volatile(value);
        }
    }

    fn read_reg(&self, reg: u32) -> u32 {
        let regs = self.regs();
        unsafe {
            (&raw mut (*regs).ioregsel).write_volatile(reg);
        }
        unsafe { (&raw mut (*regs).iowin).read_volatile() }
    }

    pub fn set_redirection(
        &self,
        gsi: u32,
        vector: u8,
        trigger: Trigger,
        polarity: Polarity,
        masked: bool,
        destination_apic_id: u8,
    ) -> Result<(), IrqError> {
        if gsi < self.gsi_base || gsi > self.gsi_max {
            return Err(IrqError::OutOfRange);
        }

        let pin = gsi - self.gsi_base;
        let (lo_reg, hi_reg) = self.ioredtbl(pin);

        let lo = (vector as u32 & 0xFF)
            | ((polarity as u32 & 1) << 13)
            | ((trigger as u32 & 1) << 15)
            | ((masked as u32 & 1) << 16);

        let hi = (destination_apic_id as u32) << 24;

        self.write_reg(hi_reg, hi);
        self.write_reg(lo_reg, lo);

        Ok(())
    }

    pub fn get_redirection(&self, gsi: u32) -> Result<(u8, Trigger, Polarity, bool, u8), IrqError> {
        if gsi < self.gsi_base || gsi > self.gsi_max {
            return Err(IrqError::OutOfRange);
        }
        let pin = gsi - self.gsi_base;

        let (lo_reg, hi_reg) = self.ioredtbl(pin);
        let lo = self.read_reg(lo_reg);
        let hi = self.read_reg(hi_reg);

        let vector = (lo & 0xFF) as u8;

        let polarity = if (lo & (1 << 13)) != 0 {
            Polarity::ActiveLow
        } else {
            Polarity::ActiveHigh
        };

        let trigger = if (lo & (1 << 15)) != 0 {
            Trigger::Level
        } else {
            Trigger::Edge
        };

        let masked = (lo & (1 << 16)) != 0;
        let destination_apic_id = (hi >> 24) as u8;

        Ok((vector, trigger, polarity, masked, destination_apic_id))
    }

    pub fn mask_gsi(&self, gsi: u32, vector: Option<u8>, mask: bool) -> Result<(), IrqError> {
        if gsi < self.gsi_base || gsi > self.gsi_max {
            return Err(IrqError::OutOfRange);
        }

        let pin = gsi - self.gsi_base;
        let (lo_reg, _) = self.ioredtbl(pin);

        let mut lo = self.read_reg(lo_reg);

        lo = (lo & !(1 << 16)) | ((mask as u32) << 16);

        if let Some(vector) = vector {
            lo = (lo & !0xFF) | vector as u32;
        }

        self.write_reg(lo_reg, lo);

        Ok(())
    }

    pub fn configure(
        &mut self,
        base_phys: PhysAddr,
        overrides: impl Iterator<Item = IoApicOverride>,
        boot_apic_id: u8,
    ) -> Option<()> {
        let virt = ioremap(base_phys, size_of::<IoApicRegs>())
            .map_err(|e| log::error!("error: {:?}", e))
            .ok()?;
        self.base_addr = virt.as_usize();

        let max_index = (self.read_reg(Self::IOAPICVER) >> 16) & 0xFF;
        self.gsi_max = self.gsi_base + max_index;

        let overrides = overrides
            .filter(|o| o.gsi_target >= self.gsi_base && o.gsi_target <= self.gsi_max)
            .collect::<Vec<_>>();

        for isroverride in overrides.iter() {
            log::info!(
                "handling override: gsi_target={}, gsi_source={}, flags={:?}",
                isroverride.gsi_target,
                isroverride.gsi_source,
                isroverride.flag
            );
            self.set_redirection(
                isroverride.gsi_target,
                0,
                isroverride.flag.0.clone(),
                isroverride.flag.1.clone(),
                true,
                boot_apic_id,
            )
            .ok()?;
        }

        self.overrides = overrides;
        Some(())
    }

    pub const fn range(&self) -> (u32, u32) {
        (self.gsi_base, self.gsi_max)
    }
}
