//! Stop and drain state types for runtime shard lifecycle.
//!
//! This module captures the snapshot of in-flight resources when a shard is
//! initiating graceful shutdown, and the enum encoding the shard's current
//! stop-state phase.

#[derive(Clone, Copy, Default, PartialEq, Eq)]
pub(crate) struct StopDrainSnapshot {
    pub(crate) listeners: usize,
    pub(crate) conns: usize,
    pub(crate) subscriptions: usize,
    pub(crate) inflight_ops: usize,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ShardStopState {
    Running,
    StopPending,
    Draining,
}
