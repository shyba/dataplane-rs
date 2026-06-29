use std::{env, path::PathBuf};

fn main() {
    println!("cargo:rerun-if-changed=memory.x");

    let target = env::var("TARGET").expect("TARGET");
    if !target.ends_with("-none-eabi") {
        return;
    }

    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").expect("CARGO_MANIFEST_DIR"));
    println!("cargo:rustc-link-search={}", manifest_dir.display());
    println!("cargo:rustc-link-arg=-Tlink.x");
}
