//! Advanced runtime-profile facade over `dataplane-core-reactor`.
//!
//! This module intentionally exposes selected core-reactor layout, policy, and
//! runtime types because the profile builders and dispatch helpers are typed
//! over those core implementations. Treat this as the advanced/core API: stable
//! callers may use the builders, `ProfiledRuntime`, and `RuntimeLoopHandle`,
//! while direct dependence on re-exported core types couples callers to the
//! runtime/core version set documented in `docs/architecture-target-validation.md`.
//! Re-exports here are not an accidental general runtime boundary.

pub use dataplane_core_reactor::balanced_profile::{
    BalancedCompletionTick, BalancedHostPolicy, BalancedProfileBudgets, BalancedProfileError,
    BalancedProfileLayout, BalancedRecordingHostPolicy, BalancedRuntime, BalancedShardRole,
    EmbeddedProfileLayout, EmbeddedProfilePolicy, EmbeddedResourceError, PerformanceProfileLayout,
};
pub use dataplane_topology::{ProfileKind, TopologyProfile};

mod builders;
mod dispatch;
mod loop_handle;
mod profiled_runtime;

pub use builders::*;
pub use dispatch::*;
pub use loop_handle::{RuntimeLoop, RuntimeLoopHandle};
pub use profiled_runtime::ProfiledRuntime;

#[cfg(test)]
#[path = "runtime_profiles_tests.rs"]
mod runtime_profiles_tests;
