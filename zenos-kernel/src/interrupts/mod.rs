pub(crate) mod gdt;

use crate::process::{FxSaveArea, PROCESSES, ProcessStatus};
use crate::{
    hardware::idt_vectors::*,
    interrupts::gdt::DOUBLE_FAULT_IST_INDEX,
    kprint, kprintln,
    process::{ProcessState, SCHEDULER},
    serial::SERIAL,
};
use core::arch::global_asm;
use log::{error, info, warn};
use spin::Lazy;
use x86_64::instructions::tlb;
use x86_64::registers::rflags::RFlags;
use x86_64::{
    instructions::port::Port,
    registers::control::Cr3,
    structures::idt::{InterruptDescriptorTable, InterruptStackFrame, PageFaultErrorCode},
    structures::paging::PhysFrame,
};

// Assembly timer interrupt handler that saves full context
global_asm!(
    r#"
.global timer_interrupt_handler_asm
timer_interrupt_handler_asm:
    // CPU has pushed: SS, RSP, RFLAGS, CS, RIP onto the stack
    // We need to save all general purpose registers
    
    // Save all GP registers
    push r15
    push r14
    push r13
    push r12
    push r11
    push r10
    push r9
    push r8
    push rdi
    push rsi
    push rbp
    push rdx
    push rcx
    push rbx
    push rax
    
    // Check if we came from userspace (CS & 3 != 0)
    // We pushed 15 registers (15*8 = 120 bytes)
    // CPU pushed: RIP, CS, RFLAGS, RSP, SS
    // CS is at offset: 15*8 + 8 = 128 = 0x80
    mov rax, [rsp + 0x80]   // CS is at offset 128 bytes
    and rax, 3
    jz .kernel_timer        // If from kernel, skip swapgs
    
    swapgs
    
.kernel_timer:
    // Pass pointer to saved registers as first argument
    mov rdi, rsp
    
    // Call the Rust timer handler
    call timer_interrupt_handler_rust
    
    // rax now contains 1 if we should switch context, 0 otherwise
    // (we don't actually need to check this, the context is already updated on the stack)

    // Check if returning to userspace (use rbx to avoid clobbering rax on stack)
    // The Rust handler may have modified the context, so we read the NEW CS value
    mov rbx, [rsp + 0x80]   // CS at offset 128 (may be modified by context switch)
    and rbx, 3
    jz .kernel_return
    
    swapgs
    
.kernel_return:
    // Restore all GP registers
    pop rax
    pop rbx
    pop rcx
    pop rdx
    pop rbp
    pop rsi
    pop rdi
    pop r8
    pop r9
    pop r10
    pop r11
    pop r12
    pop r13
    pop r14
    pop r15
    
    iretq
"#
);

unsafe extern "C" {
    fn timer_interrupt_handler_asm();
}

/// Context passed to the Rust timer handler from assembly
/// Layout must match the push order in assembly
#[repr(C)]
#[derive(Debug)]
pub struct InterruptContext {
    // Pushed by our handler (in reverse order of push)
    pub rax: u64,
    pub rbx: u64,
    pub rcx: u64,
    pub rdx: u64,
    pub rbp: u64,
    pub rsi: u64,
    pub rdi: u64,
    pub r8: u64,
    pub r9: u64,
    pub r10: u64,
    pub r11: u64,
    pub r12: u64,
    pub r13: u64,
    pub r14: u64,
    pub r15: u64,
    // Pushed by CPU on interrupt
    pub rip: u64,
    pub cs: u64,
    pub rflags: u64,
    pub rsp: u64,
    pub ss: u64,
}

#[macro_export]
macro_rules! kernel_busy_wait {
    () => {};
}

/// Rust timer interrupt handler called from assembly
/// Returns 1 if context switch occurred, 0 otherwise
#[unsafe(no_mangle)]
pub unsafe extern "C" fn timer_interrupt_handler_rust(ctx: *mut InterruptContext) -> u64 {
    // Increment the tick counter for timekeeping
    crate::disk::fs::proc::tick();

    // Update cursor (existing functionality)
    crate::update_cursor();

    // Send EOI first
    {
        let mut guard = crate::hardware::APIC_MANAGER.lock();
        guard.as_mut().unwrap().send_eoi();
    }

    // Check if we came from userspace (for preemption)
    let context = &mut *ctx;
    // Build ProcessState from interrupt context
    let mut fxsave = FxSaveArea::new();
    fxsave.save();
    let current_state = ProcessState {
        rax: context.rax,
        rbx: context.rbx,
        rcx: context.rcx,
        rdx: context.rdx,
        rsi: context.rsi,
        rdi: context.rdi,
        rbp: context.rbp,
        rsp: context.rsp,
        r8: context.r8,
        r9: context.r9,
        r10: context.r10,
        r11: context.r11,
        r12: context.r12,
        r13: context.r13,
        r14: context.r14,
        r15: context.r15,
        rip: context.rip,
        rflags: context.rflags,
        cs: context.cs,
        ss: context.ss,
        fxsave,
    };

    // Try to schedule next process - use try_lock to avoid deadlock
    // If we can't get the lock, another operation is in progress, so skip scheduling
    let switch_info = {
        match SCHEDULER.try_lock() {
            Some(mut sched) => sched.schedule(&current_state),
            None => None, // Lock held by syscall, skip scheduling this tick
        }
    };

    if let Some((new_pid, new_cr3, new_state)) = switch_info {
        // Switch CR3 to new process
        let frame = PhysFrame::containing_address(x86_64::PhysAddr::new(new_cr3));
        Cr3::write(frame, Cr3::read().1);
        tlb::flush_all();

        info!("Switched to process {}", new_pid);
        // Update the interrupt context with new process state
        context.rax = new_state.rax;
        context.rbx = new_state.rbx;
        context.rcx = new_state.rcx;
        context.rdx = new_state.rdx;
        context.rsi = new_state.rsi;
        context.rdi = new_state.rdi;
        context.rbp = new_state.rbp;
        context.rsp = new_state.rsp;
        context.r8 = new_state.r8;
        context.r9 = new_state.r9;
        context.r10 = new_state.r10;
        context.r11 = new_state.r11;
        context.r12 = new_state.r12;
        context.r13 = new_state.r13;
        context.r14 = new_state.r14;
        context.r15 = new_state.r15;
        context.rip = new_state.rip;
        context.rflags = new_state.rflags;
        // CS and SS must be updated for forked processes
        context.cs = new_state.cs;
        context.ss = new_state.ss;

        // Verify the context was updated correctly
        if context.rax != new_state.rax {
            warn!(
                "CRITICAL: context.rax ({:#x}) != new_state.rax ({:#x})",
                context.rax, new_state.rax
            ); // this isn't a error, as it is possible for raxes to be equal... unlikely, but possible... i'll take my odds with russian roulette.
            // the odds are 1/2**64 for this, 1/3 for russian roulette...
        }
        let rfl = RFlags::from_bits_retain(new_state.rflags);
        info!("rflags after switch: {rfl:?}");
        assert!(rfl.contains(RFlags::INTERRUPT_FLAG));

        return 1;
    }

    0
}

static IDT: Lazy<InterruptDescriptorTable> = Lazy::new(|| {
    let mut idt = InterruptDescriptorTable::new();
    x86_64::set_general_handler!(&mut idt, my_general_handler);
    unsafe {
        idt.double_fault
            .set_handler_fn(double_fault_handler)
            .set_stack_index((DOUBLE_FAULT_IST_INDEX + 1) as u16)
    };
    idt.page_fault.set_handler_fn(page_fault_handler);
    // Use the assembly timer handler for preemptive scheduling
    unsafe {
        idt[IRQ0_PIT].set_handler_addr(x86_64::VirtAddr::new(
            timer_interrupt_handler_asm as *const () as u64,
        ));
    }
    idt[SPURIOUS].set_handler_fn(spurious_interrupt);
    idt[IRQ1_KEYBOARD].set_handler_fn(keyboard);
    idt[IRQ2_CASCADE].set_handler_fn(cascade_handler);
    idt.invalid_opcode.set_handler_fn(undefined_opcode);
    idt.general_protection_fault.set_handler_fn(gpf_handler);
    idt
});

extern "x86-interrupt" fn double_fault_handler(ist: InterruptStackFrame, error_code: u64) -> ! {
    unsafe { SERIAL.force_unlock() }
    kprintln!("DOUBLE FAULT!");
    error!("Double fault occurred, error code: {error_code}");
    error!("stack frame: {ist:#?}");
    panic!("Double fault occurred, error code: {}", error_code);
}

extern "x86-interrupt" fn cascade_handler(ist: InterruptStackFrame) {
    error!("interrupt occured");
    error!("ist: {ist:#?}");
}
extern "x86-interrupt" fn gpf_handler(ist: InterruptStackFrame, error_code: u64) {
    error!("General Protection Fault occurred, error code: {error_code}");
    error!("stack frame: {ist:#?}");
    panic!(
        "General Protection Fault occurred, error code: {}",
        error_code
    );
}

extern "x86-interrupt" fn page_fault_handler(
    ist: InterruptStackFrame,
    error_code: PageFaultErrorCode,
) {
    error!("stack frame: {ist:#?}");
    let cr2 = x86_64::registers::control::Cr2::read();
    error!("cr2: {cr2:#?}");
    if error_code.contains(PageFaultErrorCode::USER_MODE) {
        let current_pid = {
            let sched = SCHEDULER.lock();
            let cpid = sched.current_pid();
            if cpid.is_none() {
                // already rescheduled, proc is a living zombie. I have no idea how to fix it, but I will when I know how. return early for now
                return;
            }
            cpid.unwrap()
        };
        let mut procs = PROCESSES.lock();
        let proc = procs.iter_mut().find(|p| p.pid == current_pid).unwrap();
        error!("Faulting process PID: {}", proc.pid);
        // Mark as exited (zombie) instead of removing - parent needs to wait() to reap
        proc.exit_code = Some(u64::MAX);
        proc.status = ProcessStatus::Exited;
        let pid = proc.pid;
        // I know that technically this should have no effect, but it's done so that I can
        // re-borrow procs mutably again, which is needed to reparent children.
        #[allow(dropping_references)]
        drop(proc); // release the mutable borrow
        // Reparent children to init
        for child in procs.iter_mut().filter(|p| p.parent_pid == pid) {
            child.parent_pid = 1;
        }
        kprint!("segmentation fault. core not dumped\n");
        SCHEDULER.lock().force_reschedule();
        return; // return, don't panic the kernel, will reschedule next timer interrupt.
    }
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
    unsafe { SERIAL.force_unlock() }
    error!("undefined opcode occurred... bytes: {:x?}", bytes);
    error!("stack frame: {isf:#?}");
    panic!("undefined opcode occurred");
}
