fn main() {
    let mut build = cc::Build::new();
    build
        .include("../libc/include")
        .flag("-ffreestanding")
        .flag("-nostdlib")
        .flag("-mno-red-zone")
        .debug(false);

    add_recursively(&mut build, "../libc/src".as_ref());
    println!("cargo:rerun-if-changed=../libc/src");
    if std::env::var("TARGET").unwrap().contains("x86_64") {
        build.target("x86_64-unknown-none-elf");
    } else {
        panic!("Unsupported target architecture");
    }

    build.compile("libc");
}

fn add_recursively(build: &mut cc::Build, path: &std::path::Path) {
    for entry in std::fs::read_dir(path).unwrap() {
        let entry = entry.unwrap();
        let path = entry.path();
        if path.is_dir() {
            add_recursively(build, &path);
        } else if let Some(ext) = path.extension() {
            if ext == "c" {
                build.file(path);
            }
        }
    }
}
