//! Erlang/Ranch integration over the dataplane runtime.
//!
//! `nif` is the VM entry boundary; `runtime` owns startup and shard execution.
//! Session/registration modules adapt shared `dataplane-uring` primitives to
//! Erlang resources. See the crate README for lifecycle and test requirements.

#![deny(clippy::undocumented_unsafe_blocks)]

pub mod local_boundary;
pub mod local_exec;
pub mod local_exec_counts;
pub mod local_ingress;
mod nif;
pub mod replay_protocol;
mod runtime;
// runtime_command/config are #[path] children of runtime, which re-exports
// their types. Keep the filesystem layout separate from that module namespace.
pub(crate) use dataplane_uring::id_map as runtime_id_map;
mod runtime_launch;
mod runtime_pending_reply;
mod runtime_registration;
mod runtime_result_queue {
    //! Concrete instantiation of the generic dataplane-uring result queue
    //! over the Erlang delivery enums.
    pub(crate) use dataplane_uring::result_queue::{
        ResultCallbackOutcome, ResultFacet, ResultReduceTrigger, RESULT_SEND_BATCH_LIMIT,
    };

    use crate::runtime_adapter::{AsyncReplyPayload, ResultTarget};

    pub(crate) type ResultEvent =
        dataplane_uring::result_queue::ResultEvent<ResultTarget, AsyncReplyPayload>;
    pub(crate) type ResultReduceState =
        dataplane_uring::result_queue::ResultReduceState<ResultTarget, AsyncReplyPayload>;
    pub(crate) type ResultBatchSlot =
        dataplane_uring::result_queue::ResultBatchSlot<ResultTarget, AsyncReplyPayload>;
}
mod runtime_startup;
#[path = "runtime_stop_state.rs"]
mod runtime_stop_state;
pub(crate) use dataplane_nif::runtime_adapter;
pub(crate) use dataplane_uring::arena as runtime_arena;
mod runtime_boundary;
mod runtime_connection_table;
mod runtime_dispatch;
mod runtime_driver;
mod runtime_ingress;
mod runtime_listener_table;
pub(crate) use dataplane_uring::reactor as runtime_reactor;
pub(crate) use dataplane_uring::ring_pair as runtime_ring_pair;
mod runtime_session;
mod runtime_stats;
#[allow(unused_imports)]
pub(crate) use dataplane_reactor::reactor;
pub(crate) use dataplane_runtime::errors;
pub(crate) use dataplane_runtime::runtime_protocol;
pub(crate) use dataplane_runtime::runtime_scheduler;
pub(crate) use dataplane_runtime::runtime_topology;
pub(crate) use dataplane_runtime::socket;

rustler::init! {
    "ranch_uring_nif",
    load = on_load
}

fn on_load(env: rustler::Env, _info: rustler::Term) -> bool {
    env.register::<socket::SocketRef>().is_ok()
}
