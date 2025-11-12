pub(crate) mod gdt;

use crate::hardware::idt_vectors::*;
use crate::interrupts::gdt::DOUBLE_FAULT_IST_INDEX;
use log::{error, warn};
use spin::Lazy;
use x86_64::PrivilegeLevel::Ring3;
use x86_64::instructions::port::Port;
use x86_64::structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode};

static IDT: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();
    x86_64::set_general_handler!(&mut idt, my_general_handler);
    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index(DOUBLE_FAULT_IST_INDEX as u16)
    };
    idt.page_fault.set_handler_fn(page_fault_handler);
    idt[IRQ0_PIT].set_handler_fn(timer);
    idt[SPURIOUS].set_handler_fn(spurious_interrupt);
    idt[IRQ1_KEYBOARD].set_handler_fn(keyboard);
    idt[IRQ2_CASCADE].set_handler_fn(cascade_handler);
    idt.invalid_opcode.set_handler_fn(undefined_opcode);
    idt[0x80]
        .set_handler_fn(crate::syscall::sys_rt0)
        .set_privilege_level(Ring3); // syscall entry point
    idt
});

extern "x86-interrupt" fn double_fault_handler(ist: InterruptStackFrame, error_code: u64) -> ! {
    error!("Double fault occurred, error code: {error_code}");
    error!("stack frame: {ist:#?}");
    panic!("Double fault occurred, error code: {}", error_code);
}

extern "x86-interrupt" fn cascade_handler(_: InterruptStackFrame) {}

extern "x86-interrupt" fn page_fault_handler(
    ist: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    error!("Page fault occurred, error code: {error_code:?}");
    error!("stack frame: {ist:#?}");
    let cr2 = x86_64::registers::control::Cr2::read();
    error!("cr2: {cr2:#?}");
    panic!("Page fault occurred, error code: {:?}", error_code);
}

fn my_general_handler(stack_frame: InterruptStackFrame, index: u8, error_code: Option<u64>) {
    error!("interrupt occurred, index: {index:#x}, error code: {error_code:?}",);
    error!("stack frame: {stack_frame:#?}");
    todo!("handle interrupt, idt index: {}", index)
}
pub fn init_idt() {
    IDT.load();
}

extern "x86-interrupt" fn timer(_: InterruptStackFrame) {
    let mut guard = crate::hardware::APIC_MANAGER.lock();
    crate::update_cursor();
    guard.as_mut().unwrap().send_eoi();
}

extern "x86-interrupt" fn spurious_interrupt(_: InterruptStackFrame) {
    warn!("Spurious interrupt occurred");
    let mut guard = crate::hardware::APIC_MANAGER.lock();
    guard.as_mut().unwrap().send_eoi();
}

extern "x86-interrupt" fn keyboard(_: InterruptStackFrame) {
    let mut guard = crate::hardware::APIC_MANAGER.lock();
    let mut port = Port::new(0x60);

    let scancode: u8 = unsafe { port.read() };

    guard.as_mut().unwrap().send_eoi();

    crate::hardware::keyboard::joint_keyboard_handler(scancode);
}

extern "x86-interrupt" fn undefined_opcode(isf: InterruptStackFrame) {
    let bytes: &[u8] = unsafe { core::slice::from_raw_parts(isf.instruction_pointer.as_ptr(), 20) };

    error!("undefined opcode occurred... bytes: {:x?}", bytes);
    error!("stack frame: {isf:#?}");
    crate::print_stack_trace();
    panic!("undefined opcode occurred");
}
