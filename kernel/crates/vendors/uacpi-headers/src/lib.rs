#![no_std]

#[allow(
    non_camel_case_types,
    non_upper_case_globals,
    non_snake_case,
    dead_code,
    improper_ctypes,
    unsafe_op_in_unsafe_fn,
    unsafe_attr_outside_unsafe,
    clippy::all
)]
mod bindings {
    core::include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub use bindings::*;
