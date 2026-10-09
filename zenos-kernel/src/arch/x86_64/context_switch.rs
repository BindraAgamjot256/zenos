use crate::arch::CpuContext;

#[unsafe(naked)]
pub extern "C" fn context_switch(old: *mut CpuContext, new: *mut CpuContext) {
    core::arch::naked_asm!(
        // ------------------------------------------------------------
        // On entry:
        //
        //   rdi = old CpuContext
        //   rsi = new CpuContext
        //
        // We are being called normally, so [rsp] is our return address.
        // ------------------------------------------------------------

        // Save the pointer to old somewhere that survives while we
        // modify registers. r12 is callee-saved, so use it as a scratch
        // register, but save its current value first.
        //
        // We cannot push it into old yet because that would overwrite
        // the value we need to save. Instead, use the stack temporarily.
        "push r12",
        "mov r12, rdi",
        // Save the register state that belongs to the continuation of
        // this function.
        //
        // For the callee-saved registers, these are the values required
        // by Rust code when execution eventually resumes.
        //
        // The caller-saved registers (rax, rcx, rdx, rsi, rdi, r8-r11)
        // do not have to survive a normal C function call. We nevertheless
        // store the values we have at the point of the switch so the
        // CpuContext remains complete.
        "mov [r12 + 0x0],   r15",
        "mov [r12 + 0x8],   r14",
        "mov [r12 + 0x10],  r13",
        // r12 currently contains `old`, so its old value is the value
        // we pushed onto the stack.
        // move it into r15, since it was saved beforehand.
        "mov r15,           [rsp]",
        "mov [r12 + 0x18],   r15", // r12
        "mov [r12 + 0x20],   r11",
        "mov [r12 + 0x28],   r10",
        "mov [r12 + 0x30],   r9",
        "mov [r12 + 0x38],   r8",
        "mov [r12 + 0x40],   rbp",
        "mov [r12 + 0x48],   rdi",
        "mov [r12 + 0x50],   rsi",
        "mov [r12 + 0x58],   rdx",
        "mov [r12 + 0x60],   rcx",
        "mov [r12 + 0x68],   rbx",
        "mov [r12 + 0x70],   rax",
        // ------------------------------------------------------------
        // Build the return state for `old`.
        //
        // [rsp + 8] is the return address of context_switch because
        // we temporarily pushed r12.
        //
        // When this context is later restored with iretq, we want to
        // continue immediately after the original call to context_switch.
        // ------------------------------------------------------------
        "mov rax,            [rsp + 8]",
        "mov [r12 + 0x88],   rax", // rip
        // CS = kernel CS = 0x8.
        // SS = kernel DS = 0x10.
        // RFLAGS = current flags.
        "mov rax,            cs",
        "mov [r12 + 0x90],   rax",
        "pushfq",
        "pop rax",
        "mov [r12 + 0x98],   rax",
        "lea rax,            [rsp + 0x10]", // caller RIP was here before call. so we point to it.
        "mov [r12 + 0xA0],   rax",
        "mov rax,            ss",
        "mov [r12 + 0xA8],   rax",
        // Vector/error aren't meaningful for a context that will resume
        // directly after context_switch. (or inside the kernel in general, no clue why i added them 💀)
        "mov qword ptr [r12 + 0x78], 0",
        "mov qword ptr [r12 + 0x80], 0",
        "push r12", // for test panic.
        // ------------------------------------------------------------
        // Load the new context.
        // ------------------------------------------------------------

        // rsi still contains `new`.
        "mov rax,            rsi",
        "mov r15,            [rax + 0x0] ",
        "mov r14,            [rax + 0x8] ",
        "mov r13,            [rax + 0x10]",
        "mov r12,            [rax + 0x18]",
        "mov r11,            [rax + 0x20]",
        "mov r10,            [rax + 0x28]",
        "mov r9,             [rax + 0x30]",
        "mov r8,             [rax + 0x38]",
        "mov rbp,            [rax + 0x40]",
        "mov rdi,            [rax + 0x48]",
        "mov rsi,            [rax + 0x50]",
        "mov rdx,            [rax + 0x58]",
        "mov rcx,            [rax + 0x60]",
        "mov rbx,            [rax + 0x68]",
        // "mov rax,            [rax + 0x70]", DELAYED until after the iretq frame is pushed (line 115 as of writing)
        // Load the values needed by iretq onto the stack.
        //
        // iretq expects:
        //
        //   [rsp + 0]  = RIP
        //   [rsp + 8]  = CS
        //   [rsp + 16] = RFLAGS
        //   [rsp + 24] = RSP
        //   [rsp + 32] = SS
        //
        // "mov rdi,            rax",
        // "pop rsi",
        // "call {}",
        // ==========================================================================================
        // UNCOMMENT THE ABOVE CODE AND LINE 131 TO MAKE CONTEXT SWITCHES PANIC. (debug purposes)->══════╗
        // ==========================================================================================    ║
        "push qword ptr [rax + 0xA8]", // SS                                                             ║
        "push qword ptr [rax + 0xA0]", // RSP                                                            ║
        "push qword ptr [rax + 0x98]", // RFLAGS                                                         ║
        "push qword ptr [rax + 0x90]", // CS                                                             ║
        "push qword ptr [rax + 0x88]", // RIP                                                            ║
        "mov rax,            [rax + 0x70]", //                                                           ║
        // The stack now contains exactly the iretq frame:                                               ║
        //                                                                                               ║
        //   RIP                                                                                         ║
        //   CS                                                                                          ║
        //   RFLAGS                                                                                      ║
        //   RSP                                                                                         ║
        //   SS                                                                                          ║
        //                                                                                               ║
        "iretq", //                                                                                      ║
                 // sym _test_panic<-════════════════════════════════════════════════════════════════════╝
    );
}

extern "C" fn _test_panic(old: *const CpuContext, new: *const CpuContext) {
    let old = unsafe { &*old };
    let new = unsafe { &*new };

    panic!("TEST_PANIC: old={:#?} new={:#?}", old, new);
}
