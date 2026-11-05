/// Memory map layout of the Local Advanced Programmable Interrupt Controller (LAPIC).
/// Wrap this entire struct in your custom volatile wrapper for MMIO access.
#[repr(C, align(16))]
#[derive(Debug, Copy, Clone)]
pub struct LapicRegsMmio {
    // 000h - 010h: Reserved
    _reserved1: [LApicReg; 2],
    // 020h: LAPIC ID Register (Read/Write)
    pub id: LApicReg,
    // 030h: LAPIC Version Register (Read Only)
    pub version: LApicReg,
    // 040h - 070h: Reserved
    _reserved2: [LApicReg; 4],
    // 080h: Task Priority Register (TPR) (Read/Write)
    pub task_priority: LApicReg,
    // 090h: Arbitration Priority Register (APR) (Read Only)
    pub arbitration_priority: LApicReg,
    // 0A0h: Processor Priority Register (PPR) (Read Only)
    pub processor_priority: LApicReg,
    // 0B0h: EOI Register (Write Only)
    pub eoi: LApicReg,
    // 0C0h: Remote Read Register (RRD) (Read Only)
    pub remote_read: LApicReg,
    // 0D0h: Logical Destination Register (Read/Write)
    pub logical_destination: LApicReg,
    // 0E0h: Destination Format Register (Read/Write)
    pub destination_format: LApicReg,
    // 0F0h: Spurious Interrupt Vector Register (Read/Write)
    pub spurious_interrupt_vector: LApicReg,
    // 100h - 170h: In-Service Register (ISR) (Read Only)
    pub in_service: [LApicReg; 8],
    // 180h - 1F0h: Trigger Mode Register (TMR) (Read Only)
    pub trigger_mode: [LApicReg; 8],
    // 200h - 270h: Interrupt Request Register (IRR) (Read Only)
    pub interrupt_request: [LApicReg; 8],
    // 280h: Error Status Register (Read Only)
    pub error_status: LApicReg,
    // 290h - 2E0h: Reserved
    _reserved3: [LApicReg; 6],
    // 2F0h: LVT Corrected Machine Check Interrupt (CMCI) Register (Read/Write)
    pub lvt_cmci: LApicReg,
    // 300h - 310h: Interrupt Command Register (ICR) (Read/Write)
    pub interrupt_command: [LApicReg; 2],
    // 320h: LVT Timer Register (Read/Write)
    pub lvt_timer: LApicReg,
    // 330h: LVT Thermal Sensor Register (Read/Write)
    pub lvt_thermal_sensor: LApicReg,
    // 340h: LVT Performance Monitoring Counters Register (Read/Write)
    pub lvt_perf_mon: LApicReg,
    // 350h: LVT LINT0 Register (Read/Write)
    pub lvt_lint0: LApicReg,
    // 360h: LVT LINT1 Register (Read/Write)
    pub lvt_lint1: LApicReg,
    // 370h: LVT Error Register (Read/Write)
    pub lvt_error: LApicReg,
    // 380h: Initial Count Register (for Timer) (Read/Write)
    pub timer_initial_count: LApicReg,
    // 390h: Current Count Register (for Timer) (Read Only)
    pub timer_current_count: LApicReg,
    // 3A0h - 3D0h: Reserved
    _reserved4: [LApicReg; 4],
    // 3E0h: Divide Configuration Register (for Timer) (Read/Write)
    pub timer_divide_configuration: LApicReg,
    // 3F0h: Reserved
    _reserved5: LApicReg,
}

#[repr(C)]
#[derive(Debug, Copy, Clone)]
pub struct LApicReg {
    pub val: u32,
    _resv: [u32; 3],
}

#[repr(C)]
pub struct IoApicRegs {
    /// Register Selector (Offset 0x00)
    /// Used to select which internal register to read or write.
    pub ioregsel: u32,

    /// Reserved padding (Offset 0x04 - 0x0F)
    _reserved: [u32; 3],

    /// Data Window (Offset 0x10)
    /// Used to read or write the data of the register selected by ioregsel.
    pub iowin: u32,
}
const _: () = {
    assert!(core::mem::size_of::<LApicReg>() == 0x10);
    assert!(core::mem::align_of::<LApicReg>() == 4);

    assert!(core::mem::size_of::<LapicRegsMmio>() == 0x400);

    assert!(core::mem::offset_of!(LapicRegsMmio, id) == 0x020);
    assert!(core::mem::offset_of!(LapicRegsMmio, version) == 0x030);
    assert!(core::mem::offset_of!(LapicRegsMmio, task_priority) == 0x080);
    assert!(core::mem::offset_of!(LapicRegsMmio, eoi) == 0x0B0);
    assert!(core::mem::offset_of!(LapicRegsMmio, spurious_interrupt_vector) == 0x0F0);
    assert!(core::mem::offset_of!(LapicRegsMmio, in_service) == 0x100);
    assert!(core::mem::offset_of!(LapicRegsMmio, trigger_mode) == 0x180);
    assert!(core::mem::offset_of!(LapicRegsMmio, interrupt_request) == 0x200);
    assert!(core::mem::offset_of!(LapicRegsMmio, error_status) == 0x280);
    assert!(core::mem::offset_of!(LapicRegsMmio, lvt_cmci) == 0x2F0);
    assert!(core::mem::offset_of!(LapicRegsMmio, interrupt_command) == 0x300);
    assert!(core::mem::offset_of!(LapicRegsMmio, lvt_timer) == 0x320);
    assert!(core::mem::offset_of!(LapicRegsMmio, timer_initial_count) == 0x380);
    assert!(core::mem::offset_of!(LapicRegsMmio, timer_current_count) == 0x390);
    assert!(core::mem::offset_of!(LapicRegsMmio, timer_divide_configuration) == 0x3E0);

    assert!(core::mem::offset_of!(IoApicRegs, ioregsel) == 0x00);
    assert!(core::mem::offset_of!(IoApicRegs, iowin) == 0x10);
};
