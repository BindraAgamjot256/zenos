//! APIC Abstraction for x86_64 Kernel
//!
//! Provides Local APIC (xAPIC/x2APIC) with calibrated 10 ms periodic timer,
//! multiple IOAPIC support, and 8259 PIC fallback (panics if none available).
#![allow(dead_code)] // the IPI infrastructure is never used... we silence the warnings for now.
pub(crate) mod keyboard;

use crate::memory::{HIGHER_HALF_BASE, PageType, kalloc_page};
use core::arch::x86_64::__cpuid;
use heapless::Vec;
use log::{debug, error, info, trace, warn};
use spin::{Lazy, Mutex};
use x86_64::{VirtAddr, registers::model_specific::Msr};

/// Maximum number of IOAPICs supported
pub(crate) const MAX_IOAPICS: usize = 8;

/// IDT vector assignments (central place for all vectors
#[allow(dead_code)]
pub mod idt_vectors {
    /// First remapped IRQ (PIC master offset)
    pub const IRQ_BASE: u8 = 0x20;

    /// PIC slave offset
    pub const IRQ_SLAVE_BASE: u8 = 0x28;

    /// Spurious interrupt vector (for SVR)
    pub const SPURIOUS: u8 = 0xFF;

    /// LAPIC timer interrupt vector
    pub const LAPIC_TIMER: u8 = IRQ0_PIT;

    /// Common IRQs (convenience aliases)
    pub const IRQ0_PIT: u8 = IRQ_BASE; // PIT
    pub const IRQ1_KEYBOARD: u8 = IRQ_BASE + 1; // Keyboard
    pub const IRQ2_CASCADE: u8 = IRQ_BASE + 2; // Cascade / slave PIC
    pub const IRQ3_SERIAL2: u8 = IRQ_BASE + 3;
    pub const IRQ4_SERIAL1: u8 = IRQ_BASE + 4;
    pub const IRQ5_LPT2: u8 = IRQ_BASE + 5;
    pub const IRQ6_FLOPPY: u8 = IRQ_BASE + 6;
    pub const IRQ7_LPT1: u8 = IRQ_BASE + 7;
    pub const IRQ8_RTC: u8 = IRQ_BASE + 8; // Real-time clock
    pub const IRQ9_COPROC: u8 = IRQ_BASE + 9;
    pub const IRQ10_RESERVED: u8 = IRQ_BASE + 10;
    pub const IRQ11_RESERVED: u8 = IRQ_BASE + 11;
    pub const IRQ12_MOUSE: u8 = IRQ_BASE + 12; // PS/2 Mouse
    pub const IRQ13_FPU: u8 = IRQ_BASE + 13;
    pub const IRQ14_ATA_PRIMARY: u8 = IRQ_BASE + 14; // Primary ATA
    pub const IRQ15_ATA_SECONDARY: u8 = IRQ_BASE + 15; // Secondary ATA
}

/// APIC register offsets (for documentation)
mod apic_regs {
    pub const APIC_ID: u64 = 0x20;
    pub const APIC_VERSION: u64 = 0x30;
    pub const APIC_EOI: u64 = 0xB0;
    pub const APIC_SVR: u64 = 0xF0;
    pub const APIC_ICR_LOW: u64 = 0x300;
    pub const APIC_ICR_HIGH: u64 = 0x310;
    pub const APIC_LVT_TIMER: u64 = 0x320;
    pub const APIC_TIMER_INIT: u64 = 0x380;
    pub const APIC_TIMER_CURRENT: u64 = 0x390;
    pub const APIC_TIMER_DIVIDE: u64 = 0x3E0;
}

/// Legacy PIC ports/commands
mod pic {
    use log::{debug, info, trace};
    use x86_64::instructions::port::{Port, PortGeneric};

    const PIC1_CMD: u16 = 0x20;
    const PIC1_DATA: u16 = 0x21;
    const PIC2_CMD: u16 = 0xA0;
    const PIC2_DATA: u16 = 0xA1;
    const ICW1_INIT: u8 = 0x10;
    const ICW1_ICW4: u8 = 0x01;
    const ICW4_8086: u8 = 0x01;

    pub fn supported() -> bool {
        debug!("Checking legacy PIC support");
        true
    }

    pub fn remap(offset1: u8, offset2: u8) {
        info!("Remapping legacy PIC: master offset={offset1:#x}, slave offset={offset2:#x}",);
        unsafe {
            let mut c1 = Port::new(PIC1_CMD);
            let mut d1 = Port::new(PIC1_DATA);
            let mut c2 = Port::new(PIC2_CMD);
            let mut d2 = Port::new(PIC2_DATA);

            trace!("Sending ICW1 to both PICs");
            c1.write(ICW1_INIT | ICW1_ICW4);
            c2.write(ICW1_INIT | ICW1_ICW4);

            trace!("Setting vector offsets");
            d1.write(offset1);
            d2.write(offset2);

            trace!("Configuring PIC cascade");
            d1.write(4); // PIC1 has slave on IRQ2
            d2.write(2); // PIC2 cascade identity

            trace!("Setting 8086 mode");
            d1.write(ICW4_8086);
            d2.write(ICW4_8086);
        }
        info!("Legacy PIC remapping completed");
    }

    pub fn send_eoi(irq: u8) {
        trace!("Sending EOI to legacy PIC for IRQ {irq}");
        unsafe {
            let mut c1: PortGeneric<u8, _> = Port::new(PIC1_CMD);
            let mut c2: PortGeneric<u8, _> = Port::new(PIC2_CMD);
            if irq >= 8 {
                trace!("IRQ {irq} >= 8, sending EOI to slave PIC");
                c2.write(0x20);
            }
            c1.write(0x20);
        }
    }

    /// Mask or unmask a specific IRQ line on the legacy PICs.
    pub fn set_mask(irq: u8, masked: bool) {
        let (port, bit) = if irq < 8 {
            (PIC1_DATA, irq)
        } else {
            (PIC2_DATA, irq - 8)
        };
        unsafe {
            let mut data: PortGeneric<u8, _> = Port::new(port);
            let mut val = data.read();
            if masked {
                val |= 1 << bit;
            } else {
                val &= !(1 << bit);
            }
            data.write(val);
        }
        trace!(
            "{} IRQ {} on legacy PIC",
            if masked { "Masked" } else { "Unmasked" },
            irq
        );
    }

    /// Convenience helpers.
    pub fn unmask(irq: u8) {
        set_mask(irq, false);
    }
}

/// PIT-based busy-wait helper
mod pit {
    use log::{debug, trace};
    use x86_64::instructions::port::{Port, PortGeneric};

    const PIT_CHANNEL0: u16 = 0x40;
    const PIT_CMD: u16 = 0x43;
    const PIT_FREQ: u32 = 1_193_182;
    static mut COUNT_LATCH: u16 = 0;

    pub fn prepare_sleep(us: u32) {
        let count = ((us as u64 * PIT_FREQ as u64) / 1_000_000) as u16;
        debug!("Preparing PIT sleep for {us} microseconds (count: {count})");

        unsafe {
            COUNT_LATCH = count;
        }
        let mut cmd = Port::new(PIT_CMD);
        unsafe {
            cmd.write(0b0011_0000u8);
        }
        let mut data = Port::new(PIT_CHANNEL0);
        unsafe {
            data.write((COUNT_LATCH & 0xFF) as u8);
            data.write((COUNT_LATCH >> 8) as u8);
        }
        trace!("PIT configured for {us} us delay");
    }

    pub fn perform_sleep() {
        trace!("Starting PIT-based busy wait");
        let mut cmd: PortGeneric<u8, _> = Port::new(PIT_CMD);
        let mut data: PortGeneric<u8, _> = Port::new(PIT_CHANNEL0);
        let mut iterations = 0;

        loop {
            unsafe {
                cmd.write(0b0000_0000);
            }
            let lo = unsafe { data.read() } as u16;
            let hi = unsafe { data.read() } as u16;
            let curr = (hi << 8) | lo;
            iterations += 1;

            if curr == 0 {
                trace!("PIT sleep completed after {iterations} iterations");
                break;
            }
        }
    }
}

/// Delivery modes for IPIs
#[repr(u8)]
#[derive(Debug)]
pub enum DeliveryMode {
    Fixed = 0,
    Lowest = 1,
    Smi = 2,
    Nmi = 4,
    Init = 5,
    Startup = 6,
}

/// Level for level-triggered interrupts
#[repr(u8)]
#[derive(Debug)]
pub enum Level {
    Deassert = 0,
    Assert = 1,
}

/// APIC operating modes
#[derive(Debug, Clone, Copy)]
enum ApicMode {
    XApic,
    X2Apic,
    LegacyPic,
}

/// Local APIC or legacy PIC abstraction
pub struct LocalApic {
    mode: ApicMode,
    mmio_base: u64,
}

impl LocalApic {
    /// Calibrates and starts a 10 ms periodic LAPIC timer
    fn calibrate_timer_10ms(&self) {
        info!("Starting LAPIC timer calibration for 10ms periodic timer");

        // Set timer divide by 16
        trace!("Setting LAPIC timer divisor to 16");
        self.write(apic_regs::APIC_TIMER_DIVIDE, 0x3);

        // Use PIT to sleep for 10ms while LAPIC timer counts down
        pit::prepare_sleep(10000);

        // Set LAPIC timer to maximum value
        trace!("Setting LAPIC timer initial count to maximum");
        self.write(apic_regs::APIC_TIMER_INIT, 0xFFFF_FFFF);

        // Perform the actual sleep
        pit::perform_sleep();

        // Stop timer and read elapsed count
        trace!("Stopping LAPIC timer and reading elapsed count");
        self.write(apic_regs::APIC_LVT_TIMER, 1 << 16); // Mask timer interrupt
        let elapsed = 0xFFFF_FFFFu32.wrapping_sub(self.read(apic_regs::APIC_TIMER_CURRENT));

        info!("LAPIC timer calibration: {elapsed} ticks in 10ms");

        // Configure for periodic mode with configured LAPIC timer vector
        trace!(
            "Configuring LAPIC timer for periodic mode (vector {:#x})",
            idt_vectors::LAPIC_TIMER
        );
        self.write(
            apic_regs::APIC_LVT_TIMER,
            idt_vectors::LAPIC_TIMER as u32 | (1 << 17),
        ); // Periodic mode
        self.write(apic_regs::APIC_TIMER_DIVIDE, 0x3);
        self.write(apic_regs::APIC_TIMER_INIT, elapsed);

        info!("LAPIC timer calibration completed and timer started");
    }

    fn read(&self, off: u64) -> u32 {
        let value = match self.mode {
            ApicMode::X2Apic => {
                let msr_addr = 0x800 + (off >> 4);
                trace!("Reading x2APIC MSR {msr_addr:#x} (offset {off:#x})");
                unsafe { rdmsr(msr_addr as u32) as u32 }
            }
            ApicMode::XApic => {
                let addr = self.mmio_base + off;
                trace!("Reading xAPIC MMIO at {addr:#x} (offset {off:#x})");
                unsafe { core::ptr::read_volatile(addr as *const u32) }
            }
            ApicMode::LegacyPic => {
                trace!("Attempted APIC read in legacy PIC mode (offset {off:#x})");
                0
            }
        };
        trace!("APIC read offset {off:#x} = {value:#x}");
        value
    }

    fn write(&self, off: u64, v: u32) {
        trace!("APIC write offset {off:#x} = {v:#x}");
        match self.mode {
            ApicMode::X2Apic => {
                let msr_addr = 0x800 + (off >> 4);
                trace!("Writing x2APIC MSR {msr_addr:#x} (offset {off:#x}) = {v:#x}",);
                unsafe { wrmsr(msr_addr as u32, v as u64) }
            }
            ApicMode::XApic => {
                let addr = self.mmio_base + off;
                trace!("Writing xAPIC MMIO at {addr:#x} (offset {off:#x}) = {v:#x}",);
                unsafe { core::ptr::write_volatile(addr as *mut u32, v) }
            }
            ApicMode::LegacyPic => {
                trace!("Attempted APIC write in legacy PIC mode (offset {off:#x})",);
            }
        }
    }

    /// Send EOI
    pub fn send_eoi(&self) {
        match self.mode {
            ApicMode::LegacyPic => {
                trace!("Sending EOI via legacy PIC");
                pic::send_eoi(0)
            }
            _ => {
                trace!("Sending EOI via LAPIC");
                self.write(apic_regs::APIC_EOI, 0)
            }
        }
    }

    /// Send IPI (no-op on PIC)
    pub fn send_ipi(&self, dest: u32, vec: u8, mode: DeliveryMode, lvl: Level) {
        if matches!(self.mode, ApicMode::XApic | ApicMode::X2Apic) {
            info!("Sending IPI: dest={dest:#x}, vector={vec:#x}, mode={mode:?}, level={lvl:?}",);

            self.write(apic_regs::APIC_ICR_HIGH, dest);
            let icr = ((mode as u32) << 8) | ((lvl as u32) << 14) | vec as u32;
            self.write(apic_regs::APIC_ICR_LOW, icr);

            trace!("IPI sent with ICR_LOW={icr:#x}");
        } else {
            trace!("IPI requested but running in legacy PIC mode - ignoring");
        }
    }

    /// APIC ID or 0
    pub fn id(&self) -> u8 {
        let id = match self.mode {
            ApicMode::X2Apic | ApicMode::XApic => (self.read(apic_regs::APIC_ID) >> 24) as u8,
            ApicMode::LegacyPic => 0,
        };
        trace!("LAPIC ID: {id}");
        id
    }

    /// Probe & enable modes, calibrate timer, or panic if none
    pub fn new(mmio_base: u64) -> Self {
        info!("Initializing Local APIC (MMIO base: {mmio_base:#x})");

        // Always remap PIC first
        pic::remap(idt_vectors::IRQ_BASE, idt_vectors::IRQ_SLAVE_BASE);

        // Check CPUID for APIC support
        debug!("Checking CPUID for APIC capabilities");
        let leaf = unsafe { __cpuid(1) };
        let have_apic = (leaf.edx >> 9) & 1 != 0;
        let have_x2 = (leaf.ecx >> 21) & 1 != 0;

        info!("APIC support: APIC={have_apic}, x2APIC={have_x2}");

        let mode = if have_x2 {
            info!("Enabling x2APIC mode");
            let m = unsafe { rdmsr(0x1B) };
            debug!("Current APIC_BASE MSR: {m:#x}");
            unsafe { wrmsr(0x1B, m | (1 << 10)) }; // Enable x2APIC
            let new_m = unsafe { rdmsr(0x1B) };
            debug!("New APIC_BASE MSR: {new_m:#x}");
            ApicMode::X2Apic
        } else if have_apic {
            info!("Enabling xAPIC mode");
            let m = unsafe { rdmsr(0x1B) };
            debug!("Current APIC_BASE MSR: {m:#x}");
            unsafe { wrmsr(0x1B, m | (1 << 11)) }; // Enable xAPIC
            let new_m = unsafe { rdmsr(0x1B) };
            debug!("New APIC_BASE MSR: {new_m:#x}");
            ApicMode::XApic
        } else if pic::supported() {
            warn!("No APIC support detected, falling back to legacy PIC");
            ApicMode::LegacyPic
        } else {
            error!("No interrupt controller available!");
            panic!("No APIC or PIC")
        };

        let lapic = LocalApic { mode, mmio_base };

        if !matches!(mode, ApicMode::LegacyPic) {
            info!("Configuring LAPIC (mode: {mode:?})");

            // Read and log APIC version
            let version = lapic.read(apic_regs::APIC_VERSION);
            info!(
                "LAPIC version: {:#x}, max LVT entries: {}",
                version & 0xFF,
                ((version >> 16) & 0xFF) + 1
            );

            // Enable LAPIC
            let svr = lapic.read(apic_regs::APIC_SVR);
            debug!("Current SVR: {svr:#x}");
            lapic.write(
                apic_regs::APIC_SVR,
                svr | (1 << 8) | idt_vectors::SPURIOUS as u32,
            ); // Enable + spurious vector

            let new_svr = lapic.read(apic_regs::APIC_SVR);
            debug!("New SVR: {new_svr:#x}");

            // Ensure TPR = 0 so all interrupt priorities are accepted
            // TPR is at offset 0x80 for xAPIC; x2APIC maps via MSR 0x808.
            debug!("Setting LAPIC TPR to 0");
            lapic.write(0x80, 0);

            // Calibrate and start timer
            lapic.calibrate_timer_10ms();

            info!("LAPIC initialization completed (ID: {})", lapic.id());
        } else {
            info!("Using legacy PIC mode");
            // Unmask keyboard IRQ (IRQ1) so it can fire in legacy mode.
            info!("Unmasking IRQ1 (keyboard) on legacy PIC");
            pic::unmask(idt_vectors::IRQ1_KEYBOARD - idt_vectors::IRQ_BASE);
        }

        lapic
    }
}

/// IOAPIC redirection entry
#[derive(Clone, Copy)]
pub struct IoRedirEntry {
    pub low: u32,
    pub high: u32,
}

/// IOAPIC abstraction
pub struct IoApic {
    pub id: u8,
    base: u64,
    pub max_entries: u8,
}

impl IoApic {
    fn read_reg(&self, reg: u8) -> u32 {
        trace!("Reading IOAPIC register {reg:#x}");
        unsafe {
            core::ptr::write_volatile(self.base as *mut u32, reg as u32);
            let value = core::ptr::read_volatile((self.base + 0x10) as *const u32);
            trace!("IOAPIC reg {reg:#x} = {value:#x}");
            value
        }
    }

    fn write_reg(&self, reg: u8, val: u32) {
        trace!("Writing IOAPIC register {reg:#x} = {val:#x}");
        unsafe {
            core::ptr::write_volatile(self.base as *mut u32, reg as u32);
            core::ptr::write_volatile((self.base + 0x10) as *mut u32, val);
        }
    }

    pub fn new(base: u64) -> Self {
        info!("Initializing IOAPIC at base address {base:#x}");

        // Read version register
        unsafe {
            core::ptr::write_volatile(base as *mut u32, 1);
            let ver = core::ptr::read_volatile((base + 0x10) as *const u32);
            let max_entries = ((ver >> 16) & 0xFF) + 1;
            debug!(
                "IOAPIC version: {:#x}, max redirection entries: {}",
                ver & 0xFF,
                max_entries
            );

            // Read ID register
            core::ptr::write_volatile(base as *mut u32, 0);
            let id_reg = core::ptr::read_volatile((base + 0x10) as *const u32);
            let id = (id_reg >> 24) as u8;

            info!("IOAPIC initialized: ID={id}, {max_entries} redirection entries",);

            IoApic {
                id,
                base,
                max_entries: max_entries as u8,
            }
        }
    }

    pub fn read_redir(&self, idx: u8) -> IoRedirEntry {
        debug!("Reading IOAPIC redirection entry {idx}");
        IoRedirEntry {
            low: self.read_reg(0x10 + idx * 2),
            high: self.read_reg(0x11 + idx * 2),
        }
    }

    pub fn write_redir(&self, idx: u8, e: IoRedirEntry) {
        debug!(
            "Writing IOAPIC redirection entry {}: low={:#x}, high={:#x}",
            idx, e.low, e.high
        );
        self.write_reg(0x10 + idx * 2, e.low);
        self.write_reg(0x11 + idx * 2, e.high);
    }

    pub fn mask_irq(&self, idx: u8) {
        debug!("Masking IOAPIC IRQ {idx}");
        let mut e = self.read_redir(idx);
        e.low |= 1 << 16;
        self.write_redir(idx, e);
    }

    pub fn unmask_irq(&self, idx: u8) {
        debug!("Unmasking IOAPIC IRQ {idx}");
        let mut e = self.read_redir(idx);
        e.low &= !(1 << 16);
        self.write_redir(idx, e);
    }
}

/// Manager tying LAPIC and IOAPICs
pub struct ApicManager {
    pub lapic: Mutex<LocalApic>,
    pub ioapics: Vec<IoApic, MAX_IOAPICS>,
}

impl ApicManager {
    /// Initialize everything in one call
    pub fn init(apic_base: u64, io_bases: &[u64]) -> Self {
        info!("Initializing APIC Manager");
        info!("LAPIC base: {apic_base:#x}");
        info!("IOAPIC bases: {io_bases:?}");
        let r = kalloc_page(
            VirtAddr::new(apic_base + HIGHER_HALF_BASE),
            PageType::MmioRecursive,
        );
        if r.is_ok() {
            trace!("Allocated page for LAPIC MMIO");
        } else {
            error!("Failed to allocate page for LAPIC MMIO, error: {r:#?}");
            panic!("Fuc-") // :)
        }

        let lapic = LocalApic::new(apic_base + HIGHER_HALF_BASE);
        let mut ios: Vec<IoApic, MAX_IOAPICS> = Vec::new();

        for (i, &base) in io_bases.iter().enumerate().take(MAX_IOAPICS) {
            info!("Initializing IOAPIC {i} at {base:#x}");
            let r = kalloc_page(
                VirtAddr::new(base + HIGHER_HALF_BASE),
                PageType::MmioRecursive,
            );
            if r.is_ok() {
                trace!("Allocated page for IOAPIC MMIO");
            } else {
                error!("Failed to allocate page for IOAPIC MMIO, error: {r:#?}");
                panic!("Failed to allocate page for IOAPIC MMIO, error: {r:#?}")
            }
            if ios.push(IoApic::new(base + HIGHER_HALF_BASE)).is_ok() {
                debug!("IOAPIC {i} added successfully");
            } else {
                error!("Failed to add IOAPIC {i} - maximum limit reached");
                break;
            }
        }

        info!("APIC Manager initialized with {} IOAPICs", ios.len());
        ApicManager {
            lapic: Mutex::new(lapic),
            ioapics: ios,
        }
    }

    pub fn send_eoi(&self) {
        trace!("ApicManager::send_eoi()");
        self.lapic.lock().send_eoi();
    }

    pub fn send_ipi(&self, dest: u32, vec: u8, mode: DeliveryMode, lvl: Level) {
        debug!("ApicManager::send_ipi(dest={dest:#x}, vec={vec:#x})");
        self.lapic.lock().send_ipi(dest, vec, mode, lvl)
    }

    pub fn mask_all_irqs(&self) {
        info!("Masking all IOAPIC IRQs");
        for (io_idx, io) in self.ioapics.iter().enumerate() {
            debug!("Masking all IRQs for IOAPIC {} (ID: {})", io_idx, io.id);
            for i in 0..io.max_entries {
                io.mask_irq(i);
            }
        }
        info!("All IOAPIC IRQs masked");
    }

    pub fn unmask_irq(&self, io_id: u8, idx: u8) {
        debug!("Attempting to unmask IRQ {idx} on IOAPIC ID {io_id}");
        if let Some(io) = self.ioapics.iter().find(|x| x.id == io_id) {
            info!("Unmasking IRQ {idx} on IOAPIC ID {io_id}");
            io.unmask_irq(idx);
        } else {
            warn!("IOAPIC with ID {io_id} not found");
        }
    }
}

// Helper functions for reading and writing MSRs.
unsafe fn rdmsr(msr: u32) -> u64 {
    Msr::new(msr).read()
}

unsafe fn wrmsr(msr: u32, value: u64) {
    Msr::new(msr).write(value);
}

// Usage example in kernel:
pub(crate) static APIC_MANAGER: Lazy<Mutex<Option<ApicManager>>> = Lazy::new(|| Mutex::new(None));

pub fn init(apic_base: u64, io_bases: &[u64], iso: &[(u64, u64)]) {
    let apic_manager = ApicManager::init(apic_base, io_bases);

    let lapic_id = apic_manager.lapic.lock().id();

    // For every ISO pair (isa, gsi)
    for &(isa, gsi) in iso {
        let vector = idt_vectors::IRQ_BASE + isa as u8; // remapped IDT vector (0x20=IRQ0, 0x21=IRQ1, etc)

        // Build a redirection entry
        let entry = IoRedirEntry {
            low: vector as u32,            // destination vector
            high: (lapic_id as u32) << 24, // LAPIC ID destination
        };

        // Apply it
        if let Some(ioapic) = apic_manager.ioapics.first() {
            // Safety check: ensure GSI index is in range for this IOAPIC.
            if (gsi as u8) < ioapic.max_entries {
                ioapic.write_redir(gsi as u8, entry);
                ioapic.unmask_irq(gsi as u8);
            } else {
                warn!(
                    "GSI {} out of range for selected IOAPIC (max_entries={}) - check GSI base and IOAPIC selection",
                    gsi, ioapic.max_entries
                );
            }
        }
    }

    // After processing all ISO entries...
    for isa_irq in 0..16 {
        // Skip ones already overridden in `iso`
        if iso.iter().any(|&(isa, _gsi)| isa as u8 == isa_irq) {
            continue;
        }

        let gsi = isa_irq; // default 1:1 mapping when no override
        let vector = idt_vectors::IRQ_BASE + isa_irq;
        let lapic_id = apic_manager.lapic.lock().id();

        let entry = IoRedirEntry {
            low: vector as u32,            // vector number
            high: (lapic_id as u32) << 24, // destination LAPIC
        };

        if let Some(ioapic) = apic_manager.ioapics.first()
            && gsi < ioapic.max_entries
        {
            ioapic.write_redir(gsi, entry);
            ioapic.unmask_irq(gsi);
        }
    }

    *APIC_MANAGER.lock() = Some(apic_manager);
}
