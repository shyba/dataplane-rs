//! Host-side twin of the `noalloc_primitives` replay types. Kept separate
//! because the no_std/no-alloc targets cannot depend on this crate; keep the
//! two definitions field- and derive-aligned when changing either.
#[derive(Debug, Clone)]
pub enum TraceEvent {
    RecvReq {
        shard: usize,
        session: u64,
        len: usize,
    },
    SendReq {
        shard: usize,
        session: u64,
        bytes: usize,
    },
    AcceptReq {
        listener: u64,
        timeout_ms: i64,
    },
    ReplyOk {
        request_id: u64,
        bytes: usize,
    },
    ReplyError {
        request_id: u64,
    },
    ReplyTimeout {
        request_id: u64,
    },
    Other,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayKind {
    Recv { len: usize },
    Send { bytes: usize },
    Accept { timeout_ms: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledOp {
    pub slot: usize,
    pub kind: ReplayKind,
}
