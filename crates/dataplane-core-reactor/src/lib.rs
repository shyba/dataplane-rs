#![cfg_attr(target_os = "none", no_std)]

#[cfg(any(feature = "host-core", feature = "rp2040-compile"))]
extern crate alloc;

#[cfg(feature = "host-core")]
pub mod balanced_profile;
#[cfg(all(feature = "host-core", test))]
mod balanced_profile_tests;
#[cfg(any(feature = "host-core", feature = "rp2040-compile"))]
pub mod future_task;
#[cfg(feature = "host-core")]
pub mod local_boundary;
#[cfg(any(feature = "host-core", feature = "rp2040-compile"))]
pub mod local_exec;
#[cfg(any(feature = "host-core", feature = "rp2040-compile"))]
pub mod local_exec_counts;
#[cfg(feature = "host-core")]
pub mod native_future_task;
#[cfg(feature = "host-core")]
pub mod native_task;
#[cfg(feature = "host-core")]
mod shared_wake;
#[cfg(feature = "host-core")]
pub use shared_wake::SharedTaskWakeHandle;

#[cfg(feature = "host-core")]
pub mod host_loop;
#[cfg(feature = "host-core")]
pub mod inflight_table;
#[cfg(feature = "host-core")]
pub mod local_ingress;
#[cfg(feature = "host-core")]
pub mod local_scheduler;
#[cfg(feature = "host-core")]
pub mod mailbox_future;
#[cfg(any(feature = "host-core", feature = "noalloc"))]
pub mod noalloc;
#[cfg(any(feature = "host-core", feature = "noalloc"))]
pub mod noalloc_primitives;
#[cfg(feature = "host-core")]
pub mod reactor_driver;
#[cfg(feature = "host-core")]
pub mod reactor_model;
#[cfg(feature = "host-core")]
pub mod reactor_runtime;
#[cfg(feature = "host-core")]
pub mod io_fairness;
#[cfg(feature = "host-core")]
#[deprecated(note = "renamed to `io_fairness`; this alias will be removed in a future revision")]
pub mod runtime_scheduler {
    pub use crate::io_fairness::*;
}
#[cfg(feature = "host-core")]
pub mod runtime_tls;
#[cfg(feature = "host-core")]
pub(crate) mod submission_handle;
#[cfg(feature = "host-core")]
pub mod wake_handle;
#[cfg(any(feature = "host-core", feature = "rp2040-compile"))]
pub mod replay_protocol {
    pub use dataplane_core_reactor_alloc::replay_protocol::*;
}
#[cfg(any(feature = "host-core", feature = "rp2040-compile"))]
pub mod settings {
    pub use dataplane_core_reactor_alloc::settings::*;
}
#[cfg(any(feature = "host-core", feature = "rp2040-compile"))]
pub mod wait_tag {
    pub use dataplane_core_reactor_alloc::wait_tag::*;
}

#[cfg(feature = "scheduler-tracing")]
#[macro_export]
macro_rules! scheduler_trace {
    ($($arg:tt)*) => {
        ::tracing::trace!($($arg)*);
    };
}

#[cfg(not(feature = "scheduler-tracing"))]
#[macro_export]
macro_rules! scheduler_trace {
    ($($arg:tt)*) => {{
        let _ = ();
    }};
}
