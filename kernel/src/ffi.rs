#[stabby::export]
extern "C" fn kernel_disable_interrupts() -> usize {
    crate::arch::disable_interrupts_and_save()
}

#[stabby::export]
extern "C" fn kernel_enable_interrupts(flags: usize) {
    crate::arch::restore_interrupts(flags);
}
