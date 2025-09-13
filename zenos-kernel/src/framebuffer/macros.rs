#[macro_export]
/// Prints to the host through the kernel's framebuffer interface.
macro_rules! kprint {
    ($($arg:tt)*) => (
        $crate::_print(format_args!($($arg)*))
    )
}

#[macro_export]
/// Prints to the host through the kernel's framebuffer interface, appending a newline.
macro_rules! kprintln {
    () => (kprint!("\n"));
    ($($arg:tt)*) => ($crate::kprint!("{}\n", format_args!($($arg)*)))
}
