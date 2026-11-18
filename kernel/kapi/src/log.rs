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

#[macro_export]
macro_rules! error {
    ($($args:tt)*) => {
        core::hint::spin_loop()
    };
}

#[macro_export]
macro_rules! debug {
    ($($args:tt)*) => {
        core::hint::spin_loop()
    };
}

pub use debug;
pub use error;
pub use info;
pub use warning;
