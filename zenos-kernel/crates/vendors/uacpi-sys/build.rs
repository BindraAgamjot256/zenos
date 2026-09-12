use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest_dir = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap());

    let mut git = Command::new("git");
    git.arg("pull")
        .arg("--rebase")
        .current_dir(manifest_dir.join("uacpi"))
        .status()
        .unwrap();

    let uacpi = manifest_dir.join("uacpi");
    let include = uacpi.join("include");

    let sources = [
        "source/default_handlers.c",
        "source/event.c",
        "source/interpreter.c",
        "source/io.c",
        "source/mutex.c",
        "source/namespace.c",
        "source/notify.c",
        "source/opcodes.c",
        "source/opregion.c",
        "source/osi.c",
        "source/registers.c",
        "source/resources.c",
        "source/shareable.c",
        "source/sleep.c",
        "source/stdlib.c",
        "source/tables.c",
        "source/types.c",
        "source/uacpi.c",
        "source/utilities.c",
    ];

    let mut build = cc::Build::new();

    build
        .compiler("clang")
        .target("x86_64-unknown-none-elf")
        .flag("-ffreestanding")
        .flag("-mno-red-zone")
        .flag("-fno-stack-protector")
        .flag("-fpic")
        .flag("-DUACPI_END_OF_LOG_MSG=\"\"")
        .flag("-DUACPI_DEFAULT_LOG_LEVEL=UACPI_LOG_DEBUG")
        .flag("-DUACPI_SIZED_FREES=1")
        .include(&include);

    for source in sources {
        build.file(uacpi.join(source));
    }

    build.compile("uacpi");

    let include_str = include.to_string_lossy();
    let bindings = bindgen::Builder::default()
        .header(include.join("uacpi/uacpi.h").to_string_lossy())
        .header(include.join("uacpi/tables.h").to_string_lossy())
        .header(include.join("uacpi/acpi.h").to_string_lossy())
        .clang_arg("--target=x86_64-unknown-none-elf")
        .clang_arg("-ffreestanding")
        .clang_arg("-I")
        .clang_arg(include_str)
        .use_core()
        .ctypes_prefix("core::ffi")
        .newtype_enum("uacpi_log_level")
        .newtype_enum("uacpi_status")
        .newtype_enum("acpi_madt_entry_type")
        .generate()
        .expect("failed to generate uACPI bindings");

    let out_path = PathBuf::from(env::var_os("OUT_DIR").unwrap());

    bindings
        .write_to_file(out_path.join("bindings.rs"))
        .expect("failed to write uACPI bindings");

    println!("cargo:rerun-if-changed={}", include.display());
    println!("cargo:rerun-if-changed={}", uacpi.join("source").display());
}
