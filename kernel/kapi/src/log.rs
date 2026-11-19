#[macro_export]
macro_rules! debug {
    ($($args:tt)*) => {
        $crate::ffi::kernel_log(1, core::format_args!($($args)*), $crate::stabby::str::Str::new(file!()), line!(), $crate::stabby::str::Str::new(module_path!()))
    };
}

#[macro_export]
macro_rules! info {
    ($($args:tt)*) => {
        $crate::ffi::kernel_log(2, core::format_args!($($args)*), $crate::stabby::str::Str::new(file!()), line!(), $crate::stabby::str::Str::new(module_path!()))
    };
}

mod warn_workaround {
    #[macro_export]
    macro_rules! warn_impl {
        ($($arg:tt)*) => {
            $crate::ffi::kernel_log(3, core::format_args!($($arg)*), $crate::stabby::str::Str::new(file!()), line!(), $crate::stabby::str::Str::new(module_path!()))
        };
    }

    // Renaming on re-export avoids colliding with the built-in `warn` attribute
    pub use warn_impl as warn;
}

#[macro_export]
macro_rules! error {
    ($($args:tt)*) => {
        $crate::ffi::kernel_log(4, core::format_args!($($args)*), $crate::stabby::str::Str::new(file!()), line!(), $crate::stabby::str::Str::new(module_path!()))
    };
}

pub use debug;
pub use error;
pub use info;
pub use warn_workaround::warn;
