#![allow(static_mut_refs)]

use core::mem::size_of;

use log::info;

//
// 64-bit Task State Segment
//

#[repr(C, packed)]
pub struct TaskStateSegment {
    reserved0: u32,

    pub rsp: [u64; 3], // RSP0,RSP1,RSP2

    reserved1: u64,

    pub ist: [u64; 7], // IST1–IST7

    reserved2: u64,
    reserved3: u16,

    pub io_map_base: u16,
}

impl TaskStateSegment {
    pub const fn new(rsp0: u64, ist: [u64; 7]) -> Self {
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

//
// Standard 8-byte segment descriptor
//

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

    pub const fn null() -> Self {
        Self::new(0, 0, 0, 0)
    }
}

//
// 16-byte TSS descriptor
//

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
    pub fn set_tss(&mut self, tss: &'static TaskStateSegment) {
        self.tss = TssDescriptor::new(tss);
    }
}

#[repr(C, packed)]
pub struct GdtPointer {
    pub limit: u16,
    pub base: u64,
}

impl Gdt {
    pub fn pointer(&self) -> GdtPointer {
        GdtPointer {
            limit: (size_of::<Self>() - 1) as u16,
            base: self as *const _ as u64,
        }
    }
    pub fn load(&self) {
        let pointer = self.pointer();

        unsafe {
            core::arch::asm!("lgdt [{0}]", in(reg) &pointer, options(nostack, preserves_flags))
        };
    }
}

pub fn init_gdt() {
    static TSS: TaskStateSegment = TaskStateSegment::new(0, [0; 7]);
    static mut GDT: Gdt = Gdt::new();
    unsafe {
        GDT.set_tss(&TSS);
        GDT.load();
    }
    info!("GDT initialized with TSS at {:p}", &TSS);
}
