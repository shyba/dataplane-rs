//! Subscription types for io_uring operations.
//!
//! Contains the operation and control enums used across the subscribe
//! path, plus the Subscription struct for tracking in-flight subscriptions.
//!
//! # Extracted from
//! - `runtime.rs` SubscribeOperation, SubscribeControl, Subscription (DP-CS-0165)

use crate::runtime::{ArenaHandle, RawFd, ResultTarget};

/// Operation type for a subscription.
#[derive(Clone, Copy, Debug)]
pub enum SubscribeOperation {
    Read,
    Accept,
}

/// Control command for an existing subscription.
#[derive(Clone, Copy, Debug)]
pub enum SubscribeControl {
    Choke,
    Unchoke,
    Stop,
}

/// Active subscription state for a single fd.
#[derive(Clone)]
pub struct Subscription {
    pub(crate) target: ResultTarget,
    pub(crate) accept_consumers: Vec<ResultTarget>,
    pub(crate) accept_rr_next: usize,
    pub(crate) operation: SubscribeOperation,
    pub(crate) fd: RawFd,
    pub(crate) choked: bool,
    pub(crate) stopped: bool,
    pub(crate) in_flight: usize,
    pub(crate) page_slots: [Option<ArenaHandle>; crate::runtime::SUBSCRIBE_INFLIGHT],
    pub(crate) slot_in_flight: [bool; crate::runtime::SUBSCRIBE_INFLIGHT],
}
