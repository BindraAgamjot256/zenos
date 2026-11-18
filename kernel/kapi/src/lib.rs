#![cfg_attr(not(test), no_std)]
#![feature(coerce_unsized)]
#![feature(unsize)]
#![feature(ptr_metadata)]

extern crate alloc as std_alloc;

pub(crate) mod ffi;
#[macro_use]
pub mod macros;
pub mod log;
pub mod types;

pub use types::*;
