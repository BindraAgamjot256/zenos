#[macro_export]
macro_rules! info {
    ($($args:tt)*) => {
        core::hint::spin_loop()
    };
}

#[macro_export]
macro_rules! warning {
    ($($args:tt)*) => {
        core::hint::spin_loop()
    };
}

pub use info;
pub use warning;
