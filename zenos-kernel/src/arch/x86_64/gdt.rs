#![allow(static_mut_refs)]

//! Global Descriptor Table (GDT) support for x86_64.
//!
//! The GDT describes the kernel and user code/data segments that the CPU uses
//! for privilege checks and memory access. This module also builds the task
//! state segment (TSS) descriptor that the interrupt stack table relies on.

use core::mem::size_of;

use log::info;

//
// 64-bit Task State Segment
//

/// A 64-bit Task State Segment (TSS) layout used by the x86_64 architecture.
///
/// The TSS stores the privileged stack pointers for ring transitions and the
/// interrupt stack table (IST) entries used by exception handlers.
#[repr(C, packed)]
pub struct TaskStateSegment {
    reserved0: u32,

    pub rsp: [u64; 3], // RSP0,RSP1,RSP2

    reserved1: u64,

    pub ist: [*const u8; 7], // IST1–IST7

    reserved2: u64,
    reserved3: u16,

    pub io_map_base: u16,
}

impl TaskStateSegment {
    /// Creates a new TSS with the given privileged stack pointer and IST entries.
    ///
    /// The initial stack pointer for privilege level 0 is stored in `rsp0`, while
    /// the IST array provides alternate stacks for specific interrupts and faults.
    pub const fn new(rsp0: u64, ist: [*const u8; 7]) -> Self {
        Self {
            reserved0: 0,

            rsp: [rsp0, 0, 0],

            reserved1: 0,

            ist,

            reserved2: 0,
            reserved3: 0,

            io_map_base: size_of::<Self>() as u16,
        }
    }
}

unsafe impl Send for TaskStateSegment {}
unsafe impl Sync for TaskStateSegment {}

//
// Standard 8-byte segment descriptor
//

/// A packed x86_64 segment descriptor used by the GDT.
///
/// Segments are described with a base address, a limit, and access/flag bits
/// that control privilege levels, present state, and granularity.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct SegmentDescriptor {
    limit_low: u16,
    base_low: u16,
    base_mid: u8,
    access: u8,
    granularity: u8,
    base_high: u8,
}

impl SegmentDescriptor {
    /// Builds a standard 8-byte descriptor from the provided base, limit, and flags.
    pub const fn new(base: u32, limit: u32, access: u8, flags: u8) -> Self {
        Self {
            limit_low: limit as u16,

            base_low: base as u16,

            base_mid: (base >> 16) as u8,

            access,

            granularity: (((limit >> 16) & 0x0F) as u8) | ((flags & 0x0F) << 4),

            base_high: (base >> 24) as u8,
        }
    }

    /// Returns a zeroed descriptor that acts as a null segment entry.
    pub const fn null() -> Self {
        Self::new(0, 0, 0, 0)
    }
}

//
// 16-byte TSS descriptor
//

/// A packed 16-byte descriptor that points at the TSS entry in the GDT.
#[repr(C, packed)]
#[derive(Clone, Copy)]
pub struct TssDescriptor {
    limit_low: u16,
    base_low: u16,
    base_mid1: u8,

    access: u8,

    granularity: u8,

    base_mid2: u8,

    base_high: u32,

    reserved: u32,
}

impl TssDescriptor {
    /// Creates a TSS descriptor for the given static TSS instance.
    pub fn new(tss: &'static TaskStateSegment) -> Self {
        let base = tss as *const _ as u64;
        let limit = (size_of::<TaskStateSegment>() - 1) as u32;

        Self {
            limit_low: limit as u16,

            base_low: base as u16,

            base_mid1: (base >> 16) as u8,

            access: 0x89, // Present + Available TSS

            granularity: ((limit >> 16) & 0x0F) as u8,

            base_mid2: (base >> 24) as u8,

            base_high: (base >> 32) as u32,

            reserved: 0,
        }
    }

    /// Returns an empty descriptor for entries that have not yet been initialized.
    pub const fn null() -> Self {
        Self {
            limit_low: 0,
            base_low: 0,
            base_mid1: 0,
            access: 0,
            granularity: 0,
            base_mid2: 0,
            base_high: 0,
            reserved: 0,
        }
    }
}

//
// Full GDT
//

/// The full Global Descriptor Table layout used by the kernel.
///
/// The table contains the null selector, kernel/user code and data descriptors,
/// and the TSS descriptor that enables privileged stack switching.
#[repr(C, packed)]
pub struct Gdt {
    pub null: SegmentDescriptor,
    pub kernel_code: SegmentDescriptor,
    pub kernel_data: SegmentDescriptor,

    pub user_data: SegmentDescriptor,
    pub user_code: SegmentDescriptor,

    pub tss: TssDescriptor,
}

impl Gdt {
    /// Constructs a default GDT with the standard kernel and user segment entries.
    pub const fn new() -> Self {
        Self {
            null: SegmentDescriptor::null(),

            kernel_code: SegmentDescriptor::new(0, 0xFFFFF, 0x9A, 0xA),

            kernel_data: SegmentDescriptor::new(0, 0xFFFFF, 0x92, 0xC),

            user_data: SegmentDescriptor::new(0, 0xFFFFF, 0xF2, 0xC),

            user_code: SegmentDescriptor::new(0, 0xFFFFF, 0xFA, 0xA),

            tss: TssDescriptor::null(),
        }
    }
    /// Associates this GDT entry with a specific TSS instance.
    pub fn set_tss(&mut self, tss: &'static TaskStateSegment) {
        self.tss = TssDescriptor::new(tss);
    }
}

/// The structure expected by the `lgdt` instruction.
#[repr(C, packed)]
pub struct GdtPointer {
    pub limit: u16,
    pub base: u64,
}

impl Gdt {
    /// Builds the CPU-visible pointer descriptor that can be loaded with `lgdt`.
    pub fn pointer(&self) -> GdtPointer {
        GdtPointer {
            limit: (size_of::<Self>() - 1) as u16,
            base: self as *const _ as u64,
        }
    }
    /// Loads this GDT into the processor.
    pub fn load(&self) {
        let pointer = self.pointer();

        unsafe {
            core::arch::asm!("lgdt [{0}]", in(reg) &pointer, options(nostack, preserves_flags))
        };
    }
}

static mut DOUBLE_FAULT_IST: [u8; 4096] = [0; 4096];

/// Initializes the kernel GDT and installs the boot-time TSS descriptor.
pub fn init_gdt() {
    static TSS: TaskStateSegment = unsafe {
        TaskStateSegment::new(
            0,
            [
                DOUBLE_FAULT_IST.as_ptr(),
                core::ptr::null(),
                core::ptr::null(),
                core::ptr::null(),
                core::ptr::null(),
                core::ptr::null(),
                core::ptr::null(),
            ],
        )
    };
    static mut GDT: Gdt = Gdt::new();
    unsafe {
        GDT.set_tss(&TSS);
        GDT.load();
    }
    info!("GDT initialized with TSS at {:p}", &TSS);
}
