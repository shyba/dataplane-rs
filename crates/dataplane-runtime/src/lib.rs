#![forbid(unsafe_code)]
#![cfg_attr(target_os = "none", no_std)]

//! Runtime crate boundaries and feature-gated integration surfaces.
//!
//! The ESP32 integration module must stay absent unless the
//! `esp32-integration` feature is enabled.
//!
#![cfg_attr(
    not(feature = "esp32-integration"),
    doc = "
```compile_fail
use dataplane_runtime::esp32::Esp32IntegrationPlaceholder;
let _ = Esp32IntegrationPlaceholder::new();
```
"
)]

#[cfg(all(target_os = "none", feature = "host-runtime"))]
compile_error!("dataplane-runtime host-runtime feature is not available for target_os=none");

#[cfg(feature = "host-runtime")]
pub mod embedded_host_loop;
#[cfg(all(test, feature = "host-runtime"))]
mod embedded_host_loop_test_support;
#[cfg(feature = "host-runtime")]
pub mod errors;
#[cfg(all(feature = "esp32-integration", feature = "host-runtime"))]
pub mod esp32;
#[cfg(feature = "host-runtime")]
pub mod net_addr;
#[cfg(feature = "host-runtime")]
pub mod runtime_profiles;
#[cfg(feature = "host-runtime")]
pub mod runtime_protocol;
#[cfg(feature = "host-runtime")]
pub mod runtime_scheduler;
#[cfg(feature = "host-runtime")]
pub mod runtime_topology;
#[cfg(feature = "host-runtime")]
pub mod runtime_trace;
#[cfg(feature = "erlang-nif")]
pub mod socket;

#[cfg(all(
    any(
        feature = "rp2040-integration",
        feature = "rp2040-hil-recovery",
        feature = "rp2040-usb-auto-bootsel"
    ),
    target_os = "none"
))]
pub mod rp2040;

#[cfg(any(test, all(feature = "rp2040-hil-recovery", target_os = "none")))]
mod rp2040_recovery;

#[cfg(all(
    any(feature = "noalloc", feature = "rp2040-noalloc"),
    target_os = "none"
))]
pub mod noalloc {
    pub use dataplane_core_reactor::noalloc::*;
}
