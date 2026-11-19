use core::fmt;

#[stabby::export]
extern "C" fn kernel_disable_interrupts() -> usize {
    crate::arch::disable_interrupts_and_save()
}

#[stabby::export]
extern "C" fn kernel_enable_interrupts(flags: usize) {
    crate::arch::restore_interrupts(flags);
}

#[stabby::export(canaries)]
extern "C" fn kernel_log(
    level: u8,
    args: fmt::Arguments,
    file: stabby::str::Str,
    line: u32,
    target: stabby::str::Str,
) {
    let level = match level {
        0 => log::Level::Trace,
        1 => log::Level::Debug,
        2 => log::Level::Info,
        3 => log::Level::Warn,
        4 => log::Level::Error,
        _ => {
            log::error!("Invalid log level: {} (msg: {})", level, args);
            return;
        }
    };
    log::logger().log(
        &log::Record::builder()
            .args(args)
            .file(Some(file.as_str()))
            .line(Some(line))
            .level(level)
            .target(target.as_str())
            .build(),
    )
}
