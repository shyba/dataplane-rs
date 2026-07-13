//! Cross-shard messaging: remote futures, result cells, and shard signals.
//!
//! # Shutdown semantics
//!
//! There is no orderly drain protocol. When a shard stops, work still queued
//! for it is dropped un-run when the rings/`GlobalContext` drop; each dropped
//! task's `RemoteValueSender` closes its result cell and fires its signal, so
//! waiting receivers observe `RemoteTaskRecvError` rather than hanging.
//! Senders to a stopped shard observe `ShardMeshPushError::Abandoned`.
//! Callers that need completed-before-stop guarantees must quiesce
//! submissions and drain shard loops themselves before dropping.

mod global_context;
mod remote_future;
mod remote_task;
mod result_cell;
mod shard_handle;
mod signal;

pub use global_context::{GlobalContext, ShardWaitTimeout};
pub use remote_task::{
    BoxedRuntimeFuture, QueuedRuntimeFuture, RemoteTaskReceiver, RemoteValueSender,
    RuntimeFutureTask,
};
pub use result_cell::{RemoteTaskRecvError, RemoteWaitCapacityError};
pub use shard_handle::ShardRuntimeHandle;
pub use signal::{RemoteSignalHandle, SignalKey};

#[cfg(test)]
pub(crate) use result_cell::{RemoteTaskResult, RESULT_CLOSED, RESULT_READY, RESULT_WAITING};
#[cfg(test)]
pub(crate) use signal::SignalEntry;

#[cfg(test)]
#[path = "mailbox_future_tests.rs"]
mod tests;
