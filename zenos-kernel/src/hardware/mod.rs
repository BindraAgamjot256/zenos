//! Hardware abstraction layer for x86_64 interrupt controllers.
//!
//! This module provides abstractions for interrupt controller hardware on x86_64
//! systems, supporting multiple APIC modes with automatic fallback to legacy PIC.
//!
//! # Overview
//!
//! The hardware module manages the interrupt delivery infrastructure, including:
//!
//! - **Local APIC (LAPIC)**: Per-CPU interrupt controller for timer, IPI, and local interrupts
//! - **I/O APIC**: System-wide interrupt routing for external devices (keyboard, disk, etc.)
//! - **Legacy 8259 PIC**: Fallback for older systems without APIC support
//!
//! # Architecture
//!
//! ```text
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                           Interrupt Sources                             │
//! │   (Keyboard, Timer, Disk, Network, etc.)                                │
//! └───────────────────────────────┬─────────────────────────────────────────┘
//!                                 │
//!                                 ▼
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                         I/O APIC (or 8259 PIC)                          │
//! │                                                                         │
//! │  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐  ┌─────────────┐    │
//! │  │  IRQ 0-7    │  │  IRQ 8-15   │  │  IRQ 16-23  │  │   ...       │    │
//! │  │ (ISA/Legacy)│  │ (ISA/Legacy)│  │  (PCI/MSI)  │  │             │    │
//! │  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘  └──────┬──────┘    │
//! │         │                │                │                │           │
//! │         └────────────────┴────────────────┴────────────────┘           │
//! │                                 │                                       │
//! │                    Redirection Table Entries                            │
//! └─────────────────────────────────┬───────────────────────────────────────┘
//!                                   │
//!                                   ▼
//! ┌─────────────────────────────────────────────────────────────────────────┐
//! │                           Local APIC (LAPIC)                            │
//! │                                                                         │
//! │  ┌──────────────────┐  ┌──────────────────┐  ┌──────────────────┐      │
//! │  │   Timer (10ms)   │  │   IPI Delivery   │  │  External IRQs   │      │
//! │  │   Periodic IRQ   │  │   (Inter-CPU)    │  │  (from IOAPIC)   │      │
//! │  └────────┬─────────┘  └────────┬─────────┘  └────────┬─────────┘      │
//! │           │                     │                     │                │
//! │           └─────────────────────┴─────────────────────┘                │
//! │                                 │                                       │
//! │                          IDT Vector Delivery                            │
//! └─────────────────────────────────┬───────────────────────────────────────┘
//!                                   │
//!                                   ▼
//!                          CPU Interrupt Handler
//! ```
//!
//! # Supported Modes
//!
//! The module automatically detects and enables the best available mode:
//!
//! | Mode       | Detection      | Features                                    |
//! |------------|----------------|---------------------------------------------|
//! | x2APIC     | CPUID.01H:ECX[21] | MSR-based access, 32-bit APIC IDs        |
//! | xAPIC      | CPUID.01H:EDX[9]  | MMIO-based access, 8-bit APIC IDs        |
//! | Legacy PIC | Always present | Basic IRQ 0-15 only, no SMP support        |
//!
//! # Components
//!
//! - [`ApicManager`]: Central manager coordinating LAPIC and all IOAPICs
//! - [`LocalApic`]: Per-CPU local APIC with timer calibration
//! - [`IoApic`]: I/O APIC for external interrupt routing
//! - [`idt_vectors`]: IDT vector number assignments
//! - [`keyboard`]: PS/2 keyboard input handling
//!
//! # IDT Vector Layout
//!
//! ```text
//! Vector Range    Purpose
//! ──────────────────────────────────────────
//! 0x00 - 0x1F     CPU Exceptions (reserved)
//! 0x20 - 0x2F     Remapped IRQs 0-15
//!   0x20          PIT Timer / LAPIC Timer
//!   0x21          Keyboard (IRQ1)
//!   0x22          Cascade (PIC internal)
//!   0x2E          Primary ATA (IRQ14)
//!   0x2F          Secondary ATA (IRQ15)
//! 0x30 - 0xFE     Available for devices
//! 0xFF            Spurious interrupt vector
//! ```
//!
//! # Timer Calibration
//!
//! The LAPIC timer is calibrated using the PIT (Programmable Interval Timer)
//! as a reference clock:
//!
//! 1. Configure PIT for a 10ms one-shot countdown
//! 2. Set LAPIC timer to maximum value (0xFFFFFFFF)
//! 3. Busy-wait until PIT reaches zero
//! 4. Read elapsed LAPIC ticks to determine frequency
//! 5. Configure LAPIC timer for 10ms periodic interrupts
//!
//! # Initialization Sequence
//!
//! ```rust,ignore
//! use crate::hardware::{init, APIC_MANAGER};
//!
//! // Initialize with ACPI-discovered addresses
//! init(
//!     0xFEE00000,           // LAPIC base address
//!     &[0xFEC00000],        // IOAPIC base addresses
//!     &[(0, 2), (9, 9)],    // Interrupt Source Overrides (ISA IRQ, GSI)
//! );
//!
//! // Send EOI after handling an interrupt
//! if let Some(ref manager) = *APIC_MANAGER.lock() {
//!     manager.send_eoi();
//! }
//! ```
//!
//! # Interrupt Source Overrides (ISO)
//!
//! ACPI provides Interrupt Source Override entries that remap legacy ISA IRQs
//! to different Global System Interrupt (GSI) numbers. Common overrides:
//!
//! | ISA IRQ | Typical GSI | Device          |
//! |---------|-------------|-----------------|
//! | 0       | 2           | PIT Timer       |
//! | 9       | 9           | ACPI SCI        |
//!
//! # Safety
//!
//! This module performs low-level hardware access:
//! - MMIO reads/writes for xAPIC mode
//! - MSR reads/writes for x2APIC mode  
//! - I/O port access for PIT and legacy PIC
//!
//! All hardware access is encapsulated within safe abstractions.
#![allow(dead_code)] // the IPI infrastructure is never used... we silence the warnings for now.
pub(crate) mod keyboard;

use crate::memory::{HIGHER_HALF_BASE, PageType, kalloc_page};
use core::arch::x86_64::__cpuid;
use heapless::Vec;
use log::{debug, error, info, trace, warn};
use spin::{Lazy, Mutex};
use x86_64::{VirtAddr, registers::model_specific::Msr};

/// Maximum number of IOAPICs supported by the kernel.
///
/// Most systems have only 1-2 IOAPICs, but server systems may have more.
/// This limit is enforced by the heapless Vec used in [`ApicManager`].
pub(crate) const MAX_IOAPICS: usize = 8;

/// IDT vector assignments for interrupt routing.
///
/// This module defines the interrupt vector numbers used throughout the kernel.
/// IRQs are remapped to start at `0x20` to avoid conflicts with CPU exceptions
/// (vectors `0x00`-`0x1F`).
///
/// # Vector Layout
///
/// | Range       | Purpose                          |
/// |-------------|----------------------------------|
/// | 0x00 - 0x1F | CPU exceptions (reserved by Intel) |
/// | 0x20 - 0x2F | Legacy ISA IRQs (remapped)       |
/// | 0x30 - 0xFE | Available for PCI/MSI devices    |
/// | 0xFF        | Spurious interrupt vector        |
///
/// # Example
///
/// ```rust,ignore
/// use crate::hardware::idt_vectors;
///
/// // Check if an interrupt vector is the keyboard
/// if vector == idt_vectors::IRQ1_KEYBOARD {
///     handle_keyboard_interrupt();
/// }
/// ```
#[allow(dead_code)]
pub mod idt_vectors {
    /// Base vector for remapped IRQs (PIC master offset).
    ///
    /// IRQ 0 maps to vector 0x20, IRQ 1 to 0x21, etc.
    pub const IRQ_BASE: u8 = 0x20;

    /// PIC slave offset for IRQs 8-15.
    ///
    /// Used during legacy PIC remapping to place slave PIC
    /// interrupts at vectors 0x28-0x2F.
    pub const IRQ_SLAVE_BASE: u8 = 0x28;

    /// Spurious interrupt vector for LAPIC SVR register.
    ///
    /// Set to 0xFF (highest priority) to ensure spurious interrupts
    /// don't accidentally trigger real interrupt handlers.
    pub const SPURIOUS: u8 = 0xFF;

    /// LAPIC timer interrupt vector.
    ///
    /// Configured to fire every 10ms for preemptive scheduling.
    /// Shares the same vector as IRQ0 (PIT) since only one is active.
    pub const LAPIC_TIMER: u8 = IRQ0_PIT;

    // ─────────────────────────────────────────────────────────────────
    // Legacy ISA IRQ mappings (after remapping from 0-15 to 0x20-0x2F)
    // ─────────────────────────────────────────────────────────────────

    /// IRQ 0: Programmable Interval Timer (PIT) / LAPIC Timer.
    pub const IRQ0_PIT: u8 = IRQ_BASE;

    /// IRQ 1: PS/2 Keyboard controller.
    pub const IRQ1_KEYBOARD: u8 = IRQ_BASE + 1;

    /// IRQ 2: Cascade interrupt (internal PIC wiring, not usable).
    pub const IRQ2_CASCADE: u8 = IRQ_BASE + 2;

    /// IRQ 3: COM2 / COM4 serial port.
    pub const IRQ3_SERIAL2: u8 = IRQ_BASE + 3;

    /// IRQ 4: COM1 / COM3 serial port.
    pub const IRQ4_SERIAL1: u8 = IRQ_BASE + 4;

    /// IRQ 5: LPT2 parallel port (or sound card on some systems).
    pub const IRQ5_LPT2: u8 = IRQ_BASE + 5;

    /// IRQ 6: Floppy disk controller.
    pub const IRQ6_FLOPPY: u8 = IRQ_BASE + 6;

    /// IRQ 7: LPT1 parallel port (may generate spurious interrupts).
    pub const IRQ7_LPT1: u8 = IRQ_BASE + 7;

    /// IRQ 8: Real-Time Clock (RTC).
    pub const IRQ8_RTC: u8 = IRQ_BASE + 8;

    /// IRQ 9: ACPI SCI / legacy coprocessor redirect.
    pub const IRQ9_COPROC: u8 = IRQ_BASE + 9;

    /// IRQ 10: Available (often used by network cards).
    pub const IRQ10_RESERVED: u8 = IRQ_BASE + 10;

    /// IRQ 11: Available (often used by sound cards).
    pub const IRQ11_RESERVED: u8 = IRQ_BASE + 11;

    /// IRQ 12: PS/2 Mouse controller.
    pub const IRQ12_MOUSE: u8 = IRQ_BASE + 12;

    /// IRQ 13: FPU / Coprocessor error.
    pub const IRQ13_FPU: u8 = IRQ_BASE + 13;

    /// IRQ 14: Primary ATA controller.
    pub const IRQ14_ATA_PRIMARY: u8 = IRQ_BASE + 14;

    /// IRQ 15: Secondary ATA controller.
    pub const IRQ15_ATA_SECONDARY: u8 = IRQ_BASE + 15;
}

/// APIC register offsets for MMIO/MSR access.
///
/// These offsets are used for both xAPIC (MMIO) and x2APIC (MSR) modes:
/// - **xAPIC**: Add offset to MMIO base address (e.g., `base + 0x20`)
/// - **x2APIC**: Convert to MSR: `0x800 + (offset >> 4)` (e.g., `0x802` for APIC_ID)
mod apic_regs {
    /// APIC ID Register - identifies this LAPIC.
    pub const APIC_ID: u64 = 0x20;

    /// APIC Version Register - hardware version and max LVT entries.
    pub const APIC_VERSION: u64 = 0x30;

    /// End-Of-Interrupt Register - write 0 to signal interrupt completion.
    pub const APIC_EOI: u64 = 0xB0;

    /// Spurious Interrupt Vector Register - enables LAPIC and sets spurious vector.
    pub const APIC_SVR: u64 = 0xF0;

    /// Interrupt Command Register (low 32 bits) - sends IPIs.
    pub const APIC_ICR_LOW: u64 = 0x300;

    /// Interrupt Command Register (high 32 bits) - IPI destination.
    pub const APIC_ICR_HIGH: u64 = 0x310;

    /// LVT Timer Register - configures local timer interrupt.
    pub const APIC_LVT_TIMER: u64 = 0x320;

    /// Timer Initial Count Register - countdown start value.
    pub const APIC_TIMER_INIT: u64 = 0x380;

    /// Timer Current Count Register - current countdown value.
    pub const APIC_TIMER_CURRENT: u64 = 0x390;

    /// Timer Divide Configuration Register - sets timer frequency divisor.
    pub const APIC_TIMER_DIVIDE: u64 = 0x3E0;
}

/// Legacy 8259 PIC (Programmable Interrupt Controller) support.
///
/// The 8259 PIC is used as a fallback when no APIC is available, and is always
/// initialized (then typically disabled) to remap IRQs away from CPU exceptions.
///
/// # Hardware Layout
///
/// ```text
/// ┌─────────────┐      ┌─────────────┐
/// │  PIC Master │◄────►│  PIC Slave  │
/// │  (IRQ 0-7)  │ IRQ2 │  (IRQ 8-15) │
/// └──────┬──────┘      └──────┬──────┘
///        │                    │
///        └────────┬───────────┘
///                 ▼
///              CPU INTR
/// ```
///
/// # I/O Ports
///
/// | Port   | PIC    | Purpose           |
/// |--------|--------|-------------------|
/// | 0x20   | Master | Command register  |
/// | 0x21   | Master | Data register     |
/// | 0xA0   | Slave  | Command register  |
/// | 0xA1   | Slave  | Data register     |
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

/// Programmable Interval Timer (PIT) for timing calibration.
///
/// The PIT runs at a fixed 1.193182 MHz frequency and is used to calibrate
/// the LAPIC timer, which runs at an unknown CPU-dependent frequency.
///
/// # Channel 0 Mode
///
/// Channel 0 is configured in one-shot mode for calibration:
/// - Load a 16-bit count value
/// - Counter decrements at ~1.19 MHz
/// - Busy-wait until count reaches 0
///
/// # Timing Formula
///
/// ```text
/// count = (microseconds × 1,193,182) / 1,000,000
/// ```
mod pit {
    use log::{debug, trace};
    use x86_64::instructions::port::{Port, PortGeneric};

    /// PIT Channel 0 data port.
    const PIT_CHANNEL0: u16 = 0x40;

    /// PIT command/mode register.
    const PIT_CMD: u16 = 0x43;

    /// PIT oscillator frequency in Hz (1.193182 MHz).
    const PIT_FREQ: u32 = 1_193_182;

    /// Latched count value for the current sleep operation.
    static mut COUNT_LATCH: u16 = 0;

    /// Prepare the PIT for a timed sleep.
    ///
    /// Configures Channel 0 in one-shot mode with a count calculated
    /// from the requested microseconds.
    ///
    /// # Arguments
    ///
    /// * `us` - Sleep duration in microseconds (max ~54,925 µs for 16-bit counter)
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

    /// Busy-wait for the PIT counter to reach zero.
    ///
    /// Must be called after [`prepare_sleep`] to perform the actual delay.
    /// This function polls the PIT counter in a tight loop.
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

/// IPI (Inter-Processor Interrupt) delivery modes.
///
/// Specifies how the interrupt should be delivered to the target CPU(s).
/// Used when sending IPIs via [`LocalApic::send_ipi`].
///
/// # Delivery Mode Summary
///
/// | Mode    | Value | Description                                      |
/// |---------|-------|--------------------------------------------------|
/// | Fixed   | 0     | Deliver to specific vector on target CPU(s)      |
/// | Lowest  | 1     | Deliver to lowest-priority CPU                   |
/// | SMI     | 2     | System Management Interrupt                      |
/// | NMI     | 4     | Non-Maskable Interrupt                           |
/// | INIT    | 5     | INIT signal (CPU reset to wait-for-SIPI state)   |
/// | Startup | 6     | Startup IPI (SIPI) to wake AP from INIT state    |
#[repr(u8)]
#[derive(Debug)]
pub enum DeliveryMode {
    /// Deliver interrupt to the vector specified in the ICR.
    Fixed = 0,
    /// Deliver to the processor with lowest priority.
    Lowest = 1,
    /// System Management Interrupt (enters SMM mode).
    Smi = 2,
    /// Non-Maskable Interrupt (vector ignored, delivers NMI).
    Nmi = 4,
    /// INIT signal - resets target CPU to wait-for-SIPI state.
    Init = 5,
    /// Startup IPI - boots target CPU from real mode.
    Startup = 6,
}

/// Interrupt level/trigger mode for IPIs.
///
/// Controls the assertion level of the interrupt signal.
#[repr(u8)]
#[derive(Debug)]
pub enum Level {
    /// De-assert the interrupt line.
    Deassert = 0,
    /// Assert the interrupt line.
    Assert = 1,
}

/// APIC operating mode detected at initialization.
///
/// The kernel probes CPUID to determine the best available mode.
#[derive(Debug, Clone, Copy)]
enum ApicMode {
    /// xAPIC mode - MMIO-based access at physical address.
    XApic,
    /// x2APIC mode - MSR-based access (faster, 32-bit APIC IDs).
    X2Apic,
    /// Legacy 8259 PIC fallback (no APIC available).
    LegacyPic,
}

/// Local APIC abstraction with automatic mode detection.
///
/// Manages the per-CPU Local APIC (or legacy PIC fallback), providing:
/// - Timer interrupts (calibrated 10ms periodic)
/// - EOI (End-Of-Interrupt) signaling
/// - IPI (Inter-Processor Interrupt) delivery
///
/// # Modes
///
/// The LAPIC automatically selects the best available mode:
/// 1. **x2APIC** (preferred): MSR-based, supports 32-bit APIC IDs
/// 2. **xAPIC**: MMIO-based, 8-bit APIC IDs
/// 3. **Legacy PIC**: Fallback when no APIC is present
///
/// # Example
///
/// ```rust,ignore
/// // LocalApic is typically accessed through ApicManager
/// let lapic = LocalApic::new(0xFFFF_8000_FEE0_0000);
/// lapic.send_eoi();  // Signal interrupt completion
/// ```
pub struct LocalApic {
    /// Current operating mode (x2APIC, xAPIC, or LegacyPIC).
    mode: ApicMode,
    /// MMIO base address (only used in xAPIC mode).
    mmio_base: u64,
}

impl LocalApic {
    /// Calibrates and starts a 10ms periodic LAPIC timer.
    ///
    /// Uses the PIT as a reference clock to measure LAPIC timer frequency,
    /// then configures periodic interrupts at vector [`idt_vectors::LAPIC_TIMER`].
    ///
    /// # Calibration Process
    ///
    /// 1. Set LAPIC timer divisor to 16
    /// 2. Start PIT countdown for 10ms
    /// 3. Set LAPIC initial count to maximum (0xFFFFFFFF)
    /// 4. Busy-wait until PIT reaches zero
    /// 5. Read elapsed LAPIC ticks
    /// 6. Configure periodic mode with calibrated count
    fn calibrate_timer_10ms(&self) {
        info!("Starting LAPIC timer calibration for 10ms periodic timer");

        // Set timer divide by 16
        trace!("Setting LAPIC timer divisor to 16");
        self.write(apic_regs::APIC_TIMER_DIVIDE, 0x3);

        // Use PIT to sleep for 10ms while LAPIC timer counts down
        pit::prepare_sleep(10_000);

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

    /// Signal End-Of-Interrupt to the interrupt controller.
    ///
    /// Must be called after handling any hardware interrupt to allow
    /// the controller to deliver subsequent interrupts.
    ///
    /// Automatically routes to LAPIC or legacy PIC based on current mode.
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

    /// Send an Inter-Processor Interrupt (IPI) to another CPU.
    ///
    /// IPIs are used for SMP coordination, including:
    /// - TLB shootdowns (flush remote TLBs after page table changes)
    /// - Scheduler wake-ups
    /// - CPU startup (INIT-SIPI-SIPI sequence)
    ///
    /// # Arguments
    ///
    /// * `dest` - Destination APIC ID (shifted to bits 24-31)
    /// * `vec` - Interrupt vector number
    /// * `mode` - Delivery mode (Fixed, NMI, INIT, Startup, etc.)
    /// * `lvl` - Assert or Deassert level
    ///
    /// # Note
    ///
    /// No-op when running in legacy PIC mode (no SMP support).
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

    /// Get this CPU's LAPIC ID.
    ///
    /// Returns the hardware-assigned APIC ID, or 0 in legacy PIC mode.
    /// The APIC ID is used for IPI targeting and IOAPIC routing.
    pub fn id(&self) -> u8 {
        let id = match self.mode {
            ApicMode::X2Apic | ApicMode::XApic => (self.read(apic_regs::APIC_ID) >> 24) as u8,
            ApicMode::LegacyPic => 0,
        };
        trace!("LAPIC ID: {id}");
        id
    }

    /// Create and initialize a new Local APIC.
    ///
    /// Probes CPUID for APIC support, enables the best available mode,
    /// calibrates the timer, and configures 10ms periodic interrupts.
    ///
    /// # Arguments
    ///
    /// * `mmio_base` - MMIO base address for xAPIC mode (from ACPI MADT)
    ///
    /// # Panics
    ///
    /// Panics if no interrupt controller is available (no APIC and no PIC).
    ///
    /// # Initialization Steps
    ///
    /// 1. Remap legacy PIC to vectors 0x20-0x2F
    /// 2. Probe CPUID for APIC/x2APIC support
    /// 3. Enable xAPIC or x2APIC via APIC_BASE MSR
    /// 4. Configure spurious vector and enable LAPIC
    /// 5. Calibrate and start periodic timer
    pub fn new(mmio_base: u64) -> Self {
        info!("Initializing Local APIC (MMIO base: {mmio_base:#x})");

        // Always remap PIC first
        pic::remap(idt_vectors::IRQ_BASE, idt_vectors::IRQ_SLAVE_BASE);

        // Check CPUID for APIC support
        debug!("Checking CPUID for APIC capabilities");
        let leaf = { __cpuid(1) };
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

/// I/O APIC redirection table entry.
///
/// Each entry configures how a specific IRQ is delivered to the CPU(s).
/// The entry is split into two 32-bit registers in hardware.
///
/// # Low 32 bits (bits 0-31)
///
/// | Bits  | Field           | Description                              |
/// |-------|-----------------|------------------------------------------|
/// | 0-7   | Vector          | IDT vector number                        |
/// | 8-10  | Delivery Mode   | Fixed, Lowest, SMI, NMI, INIT, ExtINT    |
/// | 11    | Dest Mode       | 0=Physical, 1=Logical                    |
/// | 12    | Delivery Status | 0=Idle, 1=Pending (read-only)            |
/// | 13    | Pin Polarity    | 0=Active High, 1=Active Low              |
/// | 14    | Remote IRR      | For level-triggered (read-only)          |
/// | 15    | Trigger Mode    | 0=Edge, 1=Level                          |
/// | 16    | Mask            | 0=Enabled, 1=Masked                      |
///
/// # High 32 bits (bits 32-63)
///
/// | Bits  | Field           | Description                              |
/// |-------|-----------------|------------------------------------------|
/// | 56-63 | Destination     | APIC ID (physical) or logical dest       |
#[derive(Clone, Copy)]
pub struct IoRedirEntry {
    /// Low 32 bits: vector, delivery mode, polarity, trigger, mask.
    pub low: u32,
    /// High 32 bits: destination APIC ID (bits 56-63 of full entry).
    pub high: u32,
}

/// I/O APIC controller for external interrupt routing.
///
/// The IOAPIC receives interrupts from external devices (keyboard, disk, etc.)
/// and routes them to one or more Local APICs based on the redirection table.
///
/// # MMIO Registers
///
/// The IOAPIC is accessed via two MMIO registers:
/// - **IOREGSEL** (offset 0x00): Index register - select which register to access
/// - **IOWIN** (offset 0x10): Data register - read/write selected register
///
/// # Redirection Table
///
/// Each IOAPIC has 24 or more redirection entries (check `max_entries`).
/// Entry N is accessed at registers `0x10 + 2*N` (low) and `0x11 + 2*N` (high).
///
/// # Example
///
/// ```rust,ignore
/// let ioapic = IoApic::new(0xFFFF_8000_FEC0_0000);
///
/// // Route IRQ1 (keyboard) to LAPIC ID 0, vector 0x21
/// let entry = IoRedirEntry {
///     low: 0x21,           // Vector 0x21, edge-triggered, unmasked
///     high: 0 << 24,       // Destination LAPIC ID 0
/// };
/// ioapic.write_redir(1, entry);
/// ```
pub struct IoApic {
    /// Hardware-assigned IOAPIC ID (from register 0).
    pub id: u8,
    /// MMIO base address (in higher-half virtual address space).
    base: u64,
    /// Maximum number of redirection entries (typically 24).
    pub max_entries: u8,
}

impl IoApic {
    /// Read an IOAPIC register via the index/data window.
    fn read_reg(&self, reg: u8) -> u32 {
        trace!("Reading IOAPIC register {reg:#x}");
        unsafe {
            core::ptr::write_volatile(self.base as *mut u32, reg as u32);
            let value = core::ptr::read_volatile((self.base + 0x10) as *const u32);
            trace!("IOAPIC reg {reg:#x} = {value:#x}");
            value
        }
    }

    /// Write an IOAPIC register via the index/data window.
    fn write_reg(&self, reg: u8, val: u32) {
        trace!("Writing IOAPIC register {reg:#x} = {val:#x}");
        unsafe {
            core::ptr::write_volatile(self.base as *mut u32, reg as u32);
            core::ptr::write_volatile((self.base + 0x10) as *mut u32, val);
        }
    }

    /// Create and initialize a new IOAPIC instance.
    ///
    /// Reads the IOAPIC ID and version registers to determine capabilities.
    ///
    /// # Arguments
    ///
    /// * `base` - MMIO base address (must already be mapped in virtual memory)
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

    /// Read a redirection table entry.
    ///
    /// # Arguments
    ///
    /// * `idx` - Redirection entry index (0 to `max_entries - 1`)
    pub fn read_redir(&self, idx: u8) -> IoRedirEntry {
        debug!("Reading IOAPIC redirection entry {idx}");
        IoRedirEntry {
            low: self.read_reg(0x10 + idx * 2),
            high: self.read_reg(0x11 + idx * 2),
        }
    }

    /// Write a redirection table entry.
    ///
    /// # Arguments
    ///
    /// * `idx` - Redirection entry index (0 to `max_entries - 1`)
    /// * `e` - The redirection entry to write
    pub fn write_redir(&self, idx: u8, e: IoRedirEntry) {
        debug!(
            "Writing IOAPIC redirection entry {}: low={:#x}, high={:#x}",
            idx, e.low, e.high
        );
        self.write_reg(0x10 + idx * 2, e.low);
        self.write_reg(0x11 + idx * 2, e.high);
    }

    /// Mask (disable) an IRQ by setting bit 16 of the redirection entry.
    ///
    /// Masked IRQs are held pending but not delivered to the CPU.
    pub fn mask_irq(&self, idx: u8) {
        debug!("Masking IOAPIC IRQ {idx}");
        let mut e = self.read_redir(idx);
        e.low |= 1 << 16;
        self.write_redir(idx, e);
    }

    /// Unmask (enable) an IRQ by clearing bit 16 of the redirection entry.
    ///
    /// The IRQ will be delivered to the configured destination LAPIC.
    pub fn unmask_irq(&self, idx: u8) {
        debug!("Unmasking IOAPIC IRQ {idx}");
        let mut e = self.read_redir(idx);
        e.low &= !(1 << 16);
        self.write_redir(idx, e);
    }
}

/// Central manager for LAPIC and all IOAPICs.
///
/// The `ApicManager` coordinates the entire interrupt controller subsystem,
/// providing a unified interface for:
/// - Sending EOI after interrupt handling
/// - Sending IPIs to other CPUs
/// - Managing IOAPIC redirection entries
///
/// # Architecture
///
/// ```text
///                    ┌─────────────────┐
///                    │   ApicManager   │
///                    └────────┬────────┘
///                             │
///          ┌──────────────────┼──────────────────┐
///          │                  │                  │
///          ▼                  ▼                  ▼
///    ┌──────────┐      ┌──────────┐      ┌──────────┐
///    │  LAPIC   │      │ IOAPIC 0 │      │ IOAPIC 1 │
///    │(per-CPU) │      │(24 IRQs) │      │(24 IRQs) │
///    └──────────┘      └──────────┘      └──────────┘
/// ```
///
/// # Example
///
/// ```rust,ignore
/// use crate::hardware::{init, APIC_MANAGER};
///
/// // Initialize during boot
/// init(0xFEE00000, &[0xFEC00000], &[]);
///
/// // Send EOI after handling interrupt
/// if let Some(ref manager) = *APIC_MANAGER.lock() {
///     manager.send_eoi();
/// }
/// ```
pub struct ApicManager {
    /// Local APIC instance (protected by mutex for interior mutability).
    pub lapic: Mutex<LocalApic>,
    /// All discovered IOAPICs (up to [`MAX_IOAPICS`]).
    pub ioapics: Vec<IoApic, MAX_IOAPICS>,
}

impl ApicManager {
    /// Initialize the APIC subsystem with ACPI-discovered addresses.
    ///
    /// Maps MMIO regions for LAPIC and all IOAPICs, then initializes
    /// each controller.
    ///
    /// # Arguments
    ///
    /// * `apic_base` - Physical base address of the Local APIC (typically 0xFEE00000)
    /// * `io_bases` - Physical base addresses of all IOAPICs (typically 0xFEC00000)
    ///
    /// # Panics
    ///
    /// Panics if MMIO page allocation fails for LAPIC or any IOAPIC.
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

    /// Signal End-Of-Interrupt to the LAPIC.
    ///
    /// Must be called after handling any hardware interrupt.
    pub fn send_eoi(&self) {
        trace!("ApicManager::send_eoi()");
        self.lapic.lock().send_eoi();
    }

    /// Send an Inter-Processor Interrupt.
    ///
    /// See [`LocalApic::send_ipi`] for details on parameters.
    pub fn send_ipi(&self, dest: u32, vec: u8, mode: DeliveryMode, lvl: Level) {
        debug!("ApicManager::send_ipi(dest={dest:#x}, vec={vec:#x})");
        self.lapic.lock().send_ipi(dest, vec, mode, lvl)
    }

    /// Mask all IRQs on all IOAPICs.
    ///
    /// Used during shutdown or when reconfiguring interrupt routing.
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

    /// Unmask a specific IRQ on a specific IOAPIC.
    ///
    /// # Arguments
    ///
    /// * `io_id` - Target IOAPIC's hardware ID
    /// * `idx` - IRQ index within that IOAPIC
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

/// Read a Model-Specific Register (MSR).
///
/// # Safety
///
/// The caller must ensure `msr` is a valid MSR address for the current CPU.
unsafe fn rdmsr(msr: u32) -> u64 {
    Msr::new(msr).read()
}

/// Write a Model-Specific Register (MSR).
///
/// # Safety
///
/// The caller must ensure `msr` is a valid MSR address and `value` is appropriate.
unsafe fn wrmsr(msr: u32, value: u64) {
    Msr::new(msr).write(value);
}

/// Global APIC manager instance.
///
/// Initialized by [`init`] during boot. Protected by a mutex for thread-safe access.
///
/// # Example
///
/// ```rust,ignore
/// use crate::hardware::APIC_MANAGER;
///
/// // Send EOI after handling an interrupt
/// if let Some(ref manager) = *APIC_MANAGER.lock() {
///     manager.send_eoi();
/// }
/// ```
pub(crate) static APIC_MANAGER: Lazy<Mutex<Option<ApicManager>>> = Lazy::new(|| Mutex::new(None));

/// Initialize the hardware interrupt subsystem.
///
/// This is the main entry point for hardware initialization, called during
/// kernel boot after ACPI tables have been parsed.
///
/// # Arguments
///
/// * `apic_base` - Physical base address of the Local APIC (from ACPI MADT)
/// * `io_bases` - Physical base addresses of all IOAPICs (from ACPI MADT)
/// * `iso` - Interrupt Source Override pairs `(ISA IRQ, GSI)` from ACPI MADT
///
/// # Interrupt Source Overrides
///
/// The `iso` parameter contains ACPI-defined mappings that override the default
/// 1:1 ISA-to-GSI mapping. For example, `(0, 2)` means ISA IRQ 0 (PIT timer)
/// is actually connected to GSI 2 on the IOAPIC.
///
/// # Initialization Steps
///
/// 1. Create [`ApicManager`] with LAPIC and all IOAPICs
/// 2. Apply Interrupt Source Overrides to IOAPIC redirection table
/// 3. Configure default 1:1 mapping for remaining ISA IRQs (0-15)
/// 4. Store manager in global [`APIC_MANAGER`]
///
/// # Example
///
/// ```rust,ignore
/// use crate::hardware::init;
///
/// // Typical ACPI-discovered values
/// init(
///     0xFEE00000,           // LAPIC base (standard address)
///     &[0xFEC00000],        // Single IOAPIC at standard address
///     &[(0, 2), (9, 9)],    // Timer remapped to GSI 2, ACPI SCI at GSI 9
/// );
/// ```
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
