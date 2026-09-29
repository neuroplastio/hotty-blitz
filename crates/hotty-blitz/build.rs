//! The shared library names itself, so a program linked against it records
//! `libhotty_blitz.so` (or `@rpath/libhotty_blitz.dylib`), not the path it
//! was built at, and finds it through its rpath once installed.

fn main() {
    match std::env::var("CARGO_CFG_TARGET_OS").as_deref() {
        Ok("linux") => println!("cargo:rustc-cdylib-link-arg=-Wl,-soname,libhotty_blitz.so"),
        Ok("macos") => {
            println!("cargo:rustc-cdylib-link-arg=-Wl,-install_name,@rpath/libhotty_blitz.dylib")
        }
        _ => {}
    }
}
