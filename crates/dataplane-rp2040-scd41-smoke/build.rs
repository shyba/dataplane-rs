use std::env;
use std::fs;
use std::path::PathBuf;

fn main() {
    let out_dir = PathBuf::from(env::var_os("OUT_DIR").unwrap());
    fs::copy("memory.x", out_dir.join("memory.x")).unwrap();
    fs::copy("link-rp2040.x", out_dir.join("link-rp2040.x")).unwrap();
    println!("cargo:rustc-link-search={}", out_dir.display());
    println!("cargo:rustc-link-arg-bins=-Tlink.x");
    println!("cargo:rustc-link-arg-bins=-Tlink-rp2040.x");
    println!("cargo:rerun-if-changed=memory.x");
    println!("cargo:rerun-if-changed=link-rp2040.x");
}
