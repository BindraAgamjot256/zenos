// required methods for integration:
// 1. kernel CS getter
// 2. kernel DS getter
// 3. interrupt disable
// 4. interrupt enable
// 5. kernel log
//

unsafe extern "C" {
    pub(crate) safe fn kernel_disable_interrupts() -> usize;
    pub(crate) safe fn kernel_enable_interrupts(flags: usize);

    cfg_select! {
        target_arch = "x86_64" => {
            pub(crate) safe fn kernel_get_cs() -> u16;
            pub(crate) safe fn kernel_get_ds() -> u16;
        }
        _ => {}
    }
}
