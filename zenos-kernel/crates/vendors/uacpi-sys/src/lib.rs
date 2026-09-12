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
    include!(concat!(env!("OUT_DIR"), "/bindings.rs"));
}

pub use bindings::*;

pub fn find_table(signature: &[u8; 5]) -> Result<uacpi_table, uacpi_status> {
    let mut table = uacpi_table {
        __bindgen_anon_1: uacpi_table__bindgen_ty_1 { virt_addr: 0 },
        index: 0,
    };

    let status =
        unsafe { uacpi_table_find_by_signature(signature.as_ptr().cast(), &raw mut table) };

    if status == uacpi_status::UACPI_STATUS_OK {
        Ok(table)
    } else {
        Err(status)
    }
}

pub fn get_table_address(signature: &[u8; 5]) -> Result<usize, uacpi_status> {
    let table = find_table(signature)?;
    Ok(unsafe { table.__bindgen_anon_1.virt_addr })
}
