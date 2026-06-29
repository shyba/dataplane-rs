//! Runtime command types extracted from runtime.rs
//!
//! ## Canonical Wiring Style
//!
//! This module is a `#[path]` submodule of `runtime.rs` (not a direct child
//! module of `lib.rs`).  It cannot be promoted to a top-level `mod` in
//! `lib.rs` because:
//!
//!   - `runtime_shard.rs` imports `Command` via `use crate::runtime::Command`
//!     (not `crate::runtime_command::Command`).
//!   - `runtime_launch.rs` re-exports `Command` from `crate::runtime`.
//!
//! Moving this module to `lib.rs` would break those imports and introduce a
//! cycle.  The `#[path]` attribute keeps the file physically adjacent to
//! `runtime.rs` while preserving the crate-internal visibility that the
//! existing import paths depend on.
//!
//! All public items use `pub(crate)` visibility — no type crosses the
//! crate boundary publicly.
//!
//! ## Cycle Boundary
//!
//! The only external import in this module is:
//!
//! ```ignore
//! use crate::runtime::{SubscribeControl, SubscribeOperation};
//! ```
//!
//! This reaches back into the parent `runtime.rs` module (which re-exports
//! these from `runtime_reactor`).  This dependency is intentional and
//! necessary — `Command` variants reference those types.
//!
//! ## Visibility Rules
//!
//! - `Command`, `ReactorClass`, `ListenerShard`, `CommandLane`, `ReadyOp`,
//!   `LatencyItem`, `DeferredSubmitCommand`, `CqeRef` — `pub(crate)`
//! - Fields on `ListenerShard` — `pub(crate)` (crate-internal shard state)
//! - No `pub` items that cross the NIF/Erlang boundary here
// DP-CS-0243 guard: runtime_command.rs has no module-wide allow(dead_code)
// This file uses targeted #[allow(dead_code)] per variant only.

use crossbeam_channel::Sender;
use std::os::fd::RawFd;
use std::sync::mpsc::SyncSender;

use rustler::{LocalPid, ResourceArc};

use crate::errors::{NifError, Result};
use crate::runtime::{SubscribeControl, SubscribeOperation};
use crate::runtime_adapter::ResultTarget;
use crate::runtime_protocol::{BatchOp, RuntimeStatsSnapshot};
use crate::socket::{ActiveMode, SocketRef};

// -----------------------------------------------------------------------------
// Command
// -----------------------------------------------------------------------------

pub(crate) enum Command {
    Linked {
        link: u64,
        cmd: Box<Command>,
    },
    InstallListener {
        listener_id: u64,
        fd: RawFd,
        accept_tx: Sender<ResourceArc<SocketRef>>,
        owner: LocalPid,
        reply: SyncSender<Result<()>>,
    },
    CloseListener {
        listener_id: u64,
        reply: SyncSender<Result<()>>,
    },
    RecvAsync {
        session_id: u64,
        len: usize,
        request_id: u64,
        enqueue_raw: u64,
        target: ResultTarget,
    },
    RecvSync {
        session_id: u64,
        len: usize,
        enqueue_raw: u64,
        reply: SyncSender<Result<Vec<u8>>>,
    },
    CancelRecv {
        session_id: u64,
        timed_out: bool,
    },
    Subscribe {
        subscription_id: u64,
        target: ResultTarget,
        operation: SubscribeOperation,
        fd: RawFd,
    },
    SubscribeControl {
        subscription_id: u64,
        control: SubscribeControl,
    },
    SubscribeAddConsumer {
        subscription_id: u64,
        target: ResultTarget,
    },
    SendAsync {
        session_id: u64,
        data: Vec<u8>,
        request_id: u64,
        target: ResultTarget,
        sync_enqueue_raw: Option<u64>,
    },
    SendEnqueue {
        session_id: u64,
        data: Vec<u8>,
    },
    BatchAsync {
        session_id: u64,
        request_id: u64,
        target: ResultTarget,
        ops: Vec<BatchOp>,
    },
    SetActiveAsync {
        session_id: u64,
        mode: ActiveMode,
        request_id: u64,
        target: ResultTarget,
    },
    SetActiveEnqueue {
        session_id: u64,
        mode: ActiveMode,
    },
    SetActive {
        session_id: u64,
        mode: ActiveMode,
        reply: SyncSender<Result<()>>,
    },
    SetNoDelayAsync {
        session_id: u64,
        enabled: bool,
        request_id: u64,
        target: ResultTarget,
    },
    SetNoDelayEnqueue {
        session_id: u64,
        enabled: bool,
    },
    SetMailboxPassiveEnqueue {
        session_id: u64,
        enabled: bool,
    },
    SetNoDelay {
        session_id: u64,
        enabled: bool,
        reply: SyncSender<Result<()>>,
    },
    UpdateOwner {
        session_id: u64,
        pid: LocalPid,
        reply: SyncSender<Result<()>>,
    },
    OwnerDown {
        session_id: u64,
    },
    ShutdownAsync {
        session_id: u64,
        how: i32,
        request_id: u64,
        target: ResultTarget,
    },
    ShutdownEnqueue {
        session_id: u64,
        how: i32,
    },
    Shutdown {
        session_id: u64,
        how: i32,
        reply: SyncSender<Result<()>>,
    },
    Stop,
    CloseAsync {
        session_id: u64,
        reason: NifError,
        request_id: u64,
        target: ResultTarget,
    },
    CloseEnqueue {
        session_id: u64,
        reason: NifError,
    },
    Close {
        session_id: u64,
        reason: NifError,
        reply: Option<SyncSender<Result<()>>>,
    },
    GetStats {
        reply: SyncSender<RuntimeStatsSnapshot>,
    },
    NoopAsync {
        request_id: u64,
        target: ResultTarget,
    },
    StatAsync {
        fd: RawFd,
        request_id: u64,
        target: ResultTarget,
    },
}

// -----------------------------------------------------------------------------
// ReactorClass
// -----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum ReactorClass {
    LowLatency,
    Throughput,
}

// -----------------------------------------------------------------------------
// Command impl
// -----------------------------------------------------------------------------

impl Command {
    pub(crate) fn linked(link: u64, cmd: Command) -> Self {
        Self::Linked {
            link,
            cmd: Box::new(cmd),
        }
    }

    pub(crate) fn unwrap_linked(self) -> (Option<u64>, Command) {
        match self {
            Self::Linked { link, cmd } => (Some(link), *cmd),
            cmd => (None, cmd),
        }
    }

    pub(crate) fn reactor_class(&self) -> ReactorClass {
        match self {
            Self::Linked { cmd, .. } => cmd.reactor_class(),
            Self::RecvSync { session_id, .. } | Self::RecvAsync { session_id, .. } => {
                if (session_id & 1) == 0 {
                    ReactorClass::LowLatency
                } else {
                    ReactorClass::Throughput
                }
            }
            Self::SendEnqueue { .. }
            | Self::SetActive { .. }
            | Self::SetActiveAsync { .. }
            | Self::SetNoDelay { .. }
            | Self::SetNoDelayAsync { .. }
            | Self::Shutdown { .. }
            | Self::ShutdownAsync { .. }
            | Self::Stop
            | Self::Close { .. }
            | Self::CloseAsync { .. }
            | Self::CloseEnqueue { .. }
            | Self::GetStats { .. }
            | Self::NoopAsync { .. }
            | Self::StatAsync { .. }
            | Self::InstallListener { .. }
            | Self::CloseListener { .. }
            | Self::CancelRecv { .. }
            | Self::Subscribe { .. }
            | Self::SubscribeControl { .. }
            | Self::SubscribeAddConsumer { .. }
            | Self::SetMailboxPassiveEnqueue { .. } => ReactorClass::LowLatency,

            Self::SetActiveEnqueue { .. }
            | Self::SetNoDelayEnqueue { .. }
            | Self::ShutdownEnqueue { .. }
            | Self::UpdateOwner { .. }
            | Self::OwnerDown { .. } => ReactorClass::Throughput,

            Self::SendAsync { .. } | Self::BatchAsync { .. } => ReactorClass::LowLatency,
        }
    }
}

// -----------------------------------------------------------------------------
// ListenerShard
// -----------------------------------------------------------------------------

pub(crate) struct ListenerShard {
    pub(crate) fd: RawFd,
    pub(crate) accept_tx: Sender<ResourceArc<SocketRef>>,
    pub(crate) owner: LocalPid,
    pub(crate) accept_armed: bool,
    pub(crate) accept_token: Option<u64>,
}

// -----------------------------------------------------------------------------
// CommandLane
// -----------------------------------------------------------------------------

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CommandLane {
    Submit,
    Exec,
}

// -----------------------------------------------------------------------------
// DeferredSubmitCommand
// -----------------------------------------------------------------------------

pub(crate) struct DeferredSubmitCommand {
    pub(crate) link: Option<u64>,
    pub(crate) cmd: Command,
}

// -----------------------------------------------------------------------------
// CqeRef
// -----------------------------------------------------------------------------

pub(crate) type CqeRef = u64;

// -----------------------------------------------------------------------------
// ReadyOp
// -----------------------------------------------------------------------------

#[derive(Clone, Copy)]
pub(crate) enum ReadyOp {
    Conn(u64),
    ReduceResults,
    SendResults(usize),
}

// -----------------------------------------------------------------------------
// LatencyItem
// -----------------------------------------------------------------------------

pub(crate) enum LatencyItem {
    Task(ReadyOp),
    Command {
        cmd: Command,
        cqe_ref: Option<CqeRef>,
        soft_link: bool,
    },
}
