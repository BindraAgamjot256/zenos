use std::{env, fs, path::Path};

fn main() {
    let out_dir = env::var_os("OUT_DIR").unwrap();

    let version_major: u16 = env!("CARGO_PKG_VERSION_MAJOR").parse().unwrap();
    let version_minor: u16 = env!("CARGO_PKG_VERSION_MINOR").parse().unwrap();
    let version_patch: u16 = env!("CARGO_PKG_VERSION_PATCH").parse().unwrap();
    let pre_release: bool = !env!("CARGO_PKG_VERSION_PRE").is_empty();

    fs::write(
        Path::new(&out_dir).join("version_info.rs"),
        format!(
            "
            pub const VERSION_MAJOR: u16 = {version_major};
            pub const VERSION_MINOR: u16 = {version_minor};
            pub const VERSION_PATCH: u16 = {version_patch};
            pub const VERSION_PRE: bool = {pre_release};
            "
        ),
    )
    .unwrap();
    println!("cargo:rerun-if-changed=Cargo.toml");
}
