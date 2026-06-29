//! Default local fast path for shard-local execution.
//!
//! This module keeps a small public shorthand for the native task engine used
//! by the in-tree benchmark callers.

pub use dataplane_core_reactor::native_future_task::{NativeFutureTask, PendingAction};
pub use dataplane_core_reactor::native_task::{
    NativeTask, NativeTaskCx, NativeTaskEngine, StepResult, TaskRef,
};
pub use dataplane_core_reactor::settings::DataPlaneSettings;
