//! Host-neutral embedded runtime host-loop boundary for M9.
//!
//! This facade preserves the public `crate::embedded_host_loop::{...}` surface
//! while the host-neutral config, adapter, loop-core, drive, and summary pieces
//! live in focused submodules.
//!
//! Boundary contract: no target HAL imports, no mandatory wait/sleep model, no
//! target-specific executor model, and no runtime-owned idle policy.

mod adapter;
mod config;
mod drive;
mod loop_core;
mod summary;

pub use adapter::EmbeddedHostAdapter;
pub use config::{EmbeddedDriveConfig, EmbeddedDriveResult, EmbeddedHostLoopConfig};
pub use loop_core::EmbeddedHostLoop;
pub use summary::EmbeddedPolicySummary;

#[cfg(test)]
#[path = "embedded_host_loop_tests.rs"]
mod embedded_host_loop_tests;
