fn main() {
    let mut build = cc::Build::new();
    build
        .file("../libc/src/syscall.c")
        .file("../libc/src/unistd.c")
        .file("../libc/src/fcntl.c")
        .file("../libc/src/string.c")
        .include("../libc/include")
        .flag("-ffreestanding")
        .flag("-nostdlib")
        .flag("-mno-red-zone");

    if std::env::var("TARGET").unwrap().contains("x86_64") {
        build.target("x86_64-unknown-none-elf");
    }

    build.compile("libc");
}
