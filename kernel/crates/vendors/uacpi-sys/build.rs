use std::path::PathBuf;
use std::{env, fs};

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());

    let uacpi = manifest_dir.join("uacpi");
    let include = uacpi.join("include");

    let entries = fs::read_dir(uacpi.join("source")).unwrap();

    let sources: Vec<String> = entries
        .filter_map(|entry| {
            let path = entry.ok()?.path();

            // Check if it's a file (skips folders)
            if path.is_file() && path.extension() == Some(std::ffi::OsStr::new("c")) {
                // Convert OsStr to String, ignoring non-UTF8 names
                path.file_name()?.to_str().map(|s| s.to_owned())
            } else {
                None
            }
        })
        .collect();

    let mut build = cc::Build::new();

    build
        .compiler("clang")
        .target("x86_64-unknown-none-elf")
        .flag("-ffreestanding")
        .flag("-mno-red-zone")
        .flag("-fno-stack-protector")
        .flag("-fpic")
        .flag("-DUACPI_END_OF_LOG_MSG=\"\"")
        .flag("-DUACPI_DEFAULT_LOG_LEVEL=UACPI_LOG_TRACE")
        .flag("-DUACPI_SIZED_FREES=1")
        .include(&include);

    for source in sources {
        println!("{}", uacpi.join("source").join(&source).display());
        build.file(uacpi.join("source").join(source));
    }

    build.compile("uacpi");

    let include_str = include.to_string_lossy();
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
