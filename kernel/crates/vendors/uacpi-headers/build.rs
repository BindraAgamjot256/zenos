use std::env;
use std::path::PathBuf;

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());
    let uacpi = manifest_dir.join("..").join("uacpi-sys").join("uacpi");

    let include: PathBuf = uacpi.join("include");

    let include_str: std::borrow::Cow<'_, str> = include.to_string_lossy();
    let bindings = bindgen::Builder::default()
        .header(include.join("uacpi/uacpi.h").to_string_lossy())
        .header(include.join("uacpi/tables.h").to_string_lossy())
        .header(include.join("uacpi/acpi.h").to_string_lossy())
        .header(include.join("uacpi/utilities.h").to_string_lossy())
        .header(include.join("uacpi/resources.h").to_string_lossy())
        .clang_arg("--target=x86_64-unknown-none-elf")
        .clang_arg("-ffreestanding")
        .clang_arg("-I")
        .clang_arg(include_str)
        .use_core()
        .ctypes_prefix("core::ffi")
        .newtype_enum("uacpi_log_level")
        .newtype_enum("uacpi_status")
        .newtype_enum("acpi_madt_entry_type")
        .bitfield_enum("uacpi_object_type_bits")
        .newtype_enum("uacpi_resource_type")
        .derive_default(true)
        .generate()
        .expect("failed to generate uACPI bindings");

    let out_path = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("failed to write uACPI bindings");

    println!("cargo:rerun-if-changed={}", include.display());
    println!("cargo:rerun-if-changed={}", uacpi.join("source").display());
    println!("cargo:rerun-if-changed=.");
}
