//! Interrupt Descriptor Table (IDT) construction and loading.
//!
//! This module creates the descriptor table seen by the CPU, attaches the
//! generated interrupt stubs to their vectors, and loads the final table.

use core::arch::asm;
use paste::paste;
use seq_macro::seq;
use super::handlers::*;

const IDT_ENTRIES: usize = 256;
const KERNEL_CS: u16 = 0x08;
const INTERRUPT_GATE: u8 = 0x8E;
const PAGE_FAULT_VECTOR: u8 = 13;


/// A single entry in the x86_64 Interrupt Descriptor Table.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct IdtEntry {
    offset_low: u16,
    selector: u16,
    ist: u8,
    attributes: u8,
    offset_mid: u16,
    offset_high: u32,
    reserved: u32,
}

impl IdtEntry {
    /// Returns an empty entry that leaves the vector unconfigured.
    pub const fn missing() -> Self {
        Self {
            offset_low: 0,
            selector: 0,
            ist: 0,
            attributes: 0,
            offset_mid: 0,
            offset_high: 0,
            reserved: 0,
        }
    }

    /// Creates an interrupt gate descriptor for a given handler address.
    pub const fn new(
        handler: usize,
        ist: u8,
        attributes: u8,
    ) -> Self {
        Self {
            offset_low: handler as u16,
            selector: KERNEL_CS,
            ist,
            attributes,
            offset_mid: (handler >> 16) as u16,
            offset_high: (handler >> 32) as u32,
            reserved: 0,
        }
    }
}

/// The full IDT as seen by the processor.
#[repr(C, align(16))]
pub struct Idt {
    entries: [IdtEntry; IDT_ENTRIES],
}

impl Idt {
    /// Creates an IDT with all entries initialized to the missing-entry pattern.
    pub const fn new() -> Self {
        Self {
            entries: [IdtEntry::missing(); IDT_ENTRIES],
        }
    }

    /// Associates a handler stub with one interrupt vector.
    pub fn set_handler(
        &mut self,
        vector: usize,
        handler: unsafe extern "C" fn(),
    ) {
        self.entries[vector] =
            IdtEntry::new(
                handler as usize,
                0,
                INTERRUPT_GATE,
            );
    }

    /// Loads the IDT into the processor using the `lidt` instruction.
    pub unsafe fn load(&self) {
        let ptr = Idtr {
            limit: (core::mem::size_of::<Self>() - 1) as u16,
            base: self as *const _ as u64,
        };

        unsafe {
            asm!(
                "lidt [{}]",
                in(reg) &ptr,
                options(readonly, nostack)
            );
        }
    }
}

#[repr(C, packed)]
struct Idtr {
    limit: u16,
    base: u64,
}

static mut IDT: Idt = Idt::new();


// Declare symbols
seq!(N in 0..256 {
    paste! {
        unsafe extern "C" {
            fn [<isr_ N>]();
        }
    }
});

// Generate handler array
macro_rules! make_isr_array {
    () => {
        seq!(N in 0..256 {
            [
                #(
                    paste! {
                        [<isr_ N>] as unsafe extern "C" fn()
                    },
                )*
            ]
        })
    };
}

static ISR_HANDLERS: [unsafe extern "C" fn(); 256] = make_isr_array!();


/// Initializes the interrupt descriptor table and registers the page-fault handler.
///
/// This is the main bring-up step for the interrupt subsystem. It installs the
/// generated entry stubs into the CPU-visible IDT, loads the table into the
/// processor, and registers the kernel's page-fault handler for the fault
/// vector used by the x86_64 CPU.
pub fn init_idt() {
    log::info!("Initializing interrupt descriptor table with {IDT_ENTRIES} entries");

    unsafe {
        let idt = &raw mut IDT;

        for (vector, handler) in ISR_HANDLERS.iter().copied().enumerate() {
            (*idt).set_handler(vector, handler);
        }

        (*idt).load();
    }

    log::info!("Registering page-fault handler for vector {PAGE_FAULT_VECTOR:#x}");
    super::registry::register_guardless(PAGE_FAULT_VECTOR, pf_handler);
}