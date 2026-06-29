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
