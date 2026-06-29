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
}

pub(super) struct TokenMeta {
    pub(super) kind: TokenKind,
    pub(super) udp_batch: Option<super::udp_batch::UdpBatchState>,
    pub(super) _connect_addr: Option<Box<socket2::SockAddr>>,
}
