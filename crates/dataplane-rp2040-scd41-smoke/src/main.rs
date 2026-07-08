#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

#[cfg(target_os = "none")]
mod firmware;

#[cfg(not(target_os = "none"))]
fn main() {
    println!("dataplane-rp2040-scd41-smoke is only active for target_os=none");
}
