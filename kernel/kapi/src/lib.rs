#![cfg_attr(not(test), no_std)]
#![expect(incomplete_features)]
#![feature(coerce_unsized)]
#![feature(unsize)]
#![feature(ptr_metadata)]
#![feature(generic_const_exprs)]

extern crate alloc as std_alloc;

pub(crate) mod ffi;
#[macro_use]
pub mod macros;
pub mod log;
pub mod types;

pub use types::*;
