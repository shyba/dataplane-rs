use std::os::fd::RawFd;

use crate::reactor::{NetOpKind, SubscriptionToken};

#[derive(Clone, Copy, Debug)]
pub(super) enum OpState {
    Simple(NetOpKind),
    SendAll {
        fd: RawFd,
        ptr: *const u8,
        len: usize,
        sent: usize,
    },
    UdpRecvBatch,
}

#[derive(Clone, Copy, Debug)]
pub(super) enum TokenKind {
    Op(OpState),
    Subscription(SubscriptionToken),
    /// A cancelled multishot subscription whose kernel op may still emit
    /// CQEs; accepted fds are closed until the final (no-MORE) CQE arrives.
    CancelledSubscription,
    /// A queued RemoveBuffers op whose meta keeps the provided buffer
    /// backing store alive until the kernel confirms removal.
    RetiredBuffers,
}

pub(super) struct TokenMeta {
    pub(super) kind: TokenKind,
    pub(super) udp_batch: Option<super::udp_batch::UdpBatchState>,
    pub(super) _connect_addr: Option<Box<socket2::SockAddr>>,
}
