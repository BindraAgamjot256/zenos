fn main() {
    // Link to prebuilt crt0.o and libc.a from libc/build/
    // Run `make` in ../libc first!

    let libc_build_dir = std::path::Path::new("../libc/build")
        .canonicalize()
        .expect("libc/build not found - run `make` in ../libc first");

    // Verify files exist
    let crt0_path = libc_build_dir.join("crt0.o");
    let libc_path = libc_build_dir.join("libc.a");

    if !crt0_path.exists() {
        panic!("crt0.o not found - run `make` in ../libc first");
    }
    if !libc_path.exists() {
        panic!("libc.a not found - run `make` in ../libc first");
    }

    // Link crt0.o first (entry point)
    println!("cargo:rustc-link-arg={}", crt0_path.display());

    // Link libc.a
    println!(
        "cargo:rustc-link-search=native={}",
        libc_build_dir.display()
    );
    println!("cargo:rustc-link-lib=static=c");

    // Rerun if libc changes
    println!("cargo:rerun-if-changed=../libc/build/crt0.o");
    println!("cargo:rerun-if-changed=../libc/build/libc.a");
}
