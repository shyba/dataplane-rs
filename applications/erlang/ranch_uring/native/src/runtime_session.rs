use std::collections::VecDeque;
use std::os::fd::OwnedFd;
use std::sync::mpsc::SyncSender;

use rustler::{LocalPid, ResourceArc};

use crate::errors::Result;
use crate::runtime_adapter::ResultTarget;
use crate::runtime_arena::{ArenaHandle, RuntimeArenas};
use crate::runtime_protocol::{BatchOp, BatchResult};
use crate::runtime_reactor::{BufSlotId, LockedReadBufPool, RingKind};
use crate::socket::{ActiveMode, SocketRef};

pub(super) const SESSION_BUF_SIZE: usize = 64 * 1024;

pub(super) type ArenaChunkRef = ArenaHandle;

pub(super) enum RxChunk {
    Arena(ArenaChunkRef),
    Heap(Vec<u8>),
}

pub(super) enum PendingWrite {
    Arena(ArenaChunkRef),
    Heap { data: Vec<u8>, off: usize },
}

pub(super) enum PendingRecvReply {
    Async {
        request_id: u64,
        target: ResultTarget,
    },
    Sync(SyncSender<Result<Vec<u8>>>),
}

pub(super) struct PendingRecv {
    pub(super) len: usize,
    pub(super) enqueued_raw: u64,
    pub(super) reply: PendingRecvReply,
}

pub(super) struct PendingBatch {
    pub(super) request_id: u64,
    pub(super) target: ResultTarget,
    pub(super) ops: VecDeque<BatchOp>,
    pub(super) results: Vec<(u64, BatchResult)>,
}

pub(super) struct Connection {
    pub(super) fd: OwnedFd,
    pub(super) handle: ResourceArc<SocketRef>,
    pub(super) owner: LocalPid,
    pub(super) read_buf: ReadBufLease,
    pub(super) rx_chunks: VecDeque<RxChunk>,
    pub(super) rx_chunk_off: usize,
    pub(super) rx_bytes: usize,
    pub(super) pending_recv: Option<PendingRecv>,
    pub(super) canceled_recv_token: Option<u64>,
    pub(super) pending_batch: Option<PendingBatch>,
    pub(super) pending_writes: VecDeque<PendingWrite>,
    pub(super) tx_bytes: usize,
    pub(super) active_mode: ActiveMode,
    pub(super) read_poll_armed: bool,
    pub(super) write_poll_armed: bool,
    pub(super) read_in_flight: bool,
    pub(super) read_in_flight_token: Option<u64>,
    pub(super) read_in_flight_ring: Option<RingKind>,
    pub(super) stop_close_on_recv_cqe: bool,
    pub(super) stop_close_on_read_poll_cqe: bool,
    pub(super) stop_close_on_write_cqe: bool,
    pub(super) stop_close_on_write_poll_cqe: bool,
    pub(super) write_in_flight: bool,
    pub(super) write_ready_queued: bool,
    pub(super) write_iovecs: Vec<libc::iovec>,
}

pub(super) enum ReadBufLease {
    Fixed(BufSlotId),
    Heap(Vec<u8>),
}

impl ReadBufLease {
    pub(super) fn try_from_fixed_pool(pool: &mut LockedReadBufPool) -> Option<Self> {
        pool.acquire_fixed_slot().map(Self::Fixed)
    }

    pub(super) fn from_pool_or_heap_fallback(pool: &mut LockedReadBufPool) -> Self {
        Self::try_from_fixed_pool(pool).unwrap_or_else(|| Self::Heap(pool.fallback_buf()))
    }

    pub(super) fn as_mut_ptr(&mut self, pool: &mut LockedReadBufPool) -> *mut u8 {
        match self {
            Self::Fixed(_) => self
                .fixed_slot_mut_view(pool)
                .map(|(ptr, _)| ptr)
                .unwrap_or(std::ptr::null_mut()),
            Self::Heap(buf) => buf.as_mut_ptr(),
        }
    }

    pub(super) fn len(&mut self, pool: &mut LockedReadBufPool) -> usize {
        match self {
            Self::Fixed(_) => self
                .fixed_slot_mut_view(pool)
                .map(|(_, len)| len)
                .unwrap_or(0),
            Self::Heap(buf) => buf.len(),
        }
    }

    pub(super) fn fixed_slot_mut_view(
        &mut self,
        pool: &mut LockedReadBufPool,
    ) -> Option<(*mut u8, usize)> {
        let Self::Fixed(slot) = self else {
            return None;
        };
        let ptr = pool.fixed_slot_ptr(*slot)?;
        let len = pool.fixed_slot_len(*slot)?;
        Some((ptr, len))
    }

    pub(super) fn slot_id(&self) -> Option<BufSlotId> {
        if let Self::Fixed(slot) = self {
            Some(*slot)
        } else {
            None
        }
    }

    #[cfg_attr(not(test), allow(dead_code))]
    pub(super) fn slot(&self) -> Option<u16> {
        self.slot_id().map(|slot| slot.index)
    }

    pub(super) fn as_slice<'a>(&'a self, pool: &'a LockedReadBufPool) -> Option<&'a [u8]> {
        match self {
            Self::Fixed(slot) => pool.fixed_slot_slice(*slot),
            Self::Heap(buf) => Some(buf.as_slice()),
        }
    }

    pub(super) fn release(self, pool: &mut LockedReadBufPool) -> bool {
        match self {
            Self::Fixed(slot) => pool.release_fixed_slot(slot).is_ok(),
            Self::Heap(_) => true,
        }
    }
}

pub(super) fn rx_push_slice(conn: &mut Connection, arenas: &mut RuntimeArenas, chunk: &[u8]) {
    if chunk.is_empty() {
        return;
    }
    let len = chunk.len();
    if len <= SESSION_BUF_SIZE {
        if let Some(mut handle) = arenas.acquire_for_len(len) {
            arenas
                .slice_mut_prefix(handle.class, handle.slot, len)
                .copy_from_slice(chunk);
            handle.off = 0;
            handle.len = len;
            conn.rx_bytes += len;
            conn.rx_chunks.push_back(RxChunk::Arena(handle));
            return;
        }
    }
    conn.rx_bytes += len;
    conn.rx_chunks.push_back(RxChunk::Heap(chunk.to_vec()));
}

pub(super) fn rx_push_from_read_buf(
    conn: &mut Connection,
    read_pool: &LockedReadBufPool,
    arenas: &mut RuntimeArenas,
    off: usize,
    len: usize,
) -> bool {
    if len == 0 {
        return true;
    }
    let Some(read_src) = conn.read_buf.as_slice(read_pool) else {
        return false;
    };
    if off >= read_src.len() || off.saturating_add(len) > read_src.len() {
        return false;
    }
    if len <= SESSION_BUF_SIZE {
        if let Some(mut handle) = arenas.acquire_for_len(len) {
            let src = &read_src[off..off + len];
            arenas
                .slice_mut_prefix(handle.class, handle.slot, len)
                .copy_from_slice(src);
            handle.off = 0;
            handle.len = len;
            conn.rx_bytes += len;
            conn.rx_chunks.push_back(RxChunk::Arena(handle));
            return true;
        }
    }
    conn.rx_bytes += len;
    conn.rx_chunks
        .push_back(RxChunk::Heap(read_src[off..off + len].to_vec()));
    true
}

pub(super) fn rx_take(conn: &mut Connection, arenas: &mut RuntimeArenas, n: usize) -> Vec<u8> {
    let mut remaining = n.min(conn.rx_bytes);
    let mut out = Vec::with_capacity(remaining);

    while remaining > 0 {
        let Some(front) = conn.rx_chunks.front() else {
            break;
        };
        let available = match front {
            RxChunk::Arena(chunk) => chunk.len - conn.rx_chunk_off,
            RxChunk::Heap(data) => data.len() - conn.rx_chunk_off,
        };
        let take = remaining.min(available);
        match front {
            RxChunk::Arena(chunk) => {
                let start = chunk.off + conn.rx_chunk_off;
                out.extend_from_slice(arenas.slice(chunk.class, chunk.slot, start, take));
            }
            RxChunk::Heap(data) => {
                out.extend_from_slice(&data[conn.rx_chunk_off..conn.rx_chunk_off + take]);
            }
        }
        conn.rx_chunk_off += take;
        conn.rx_bytes -= take;
        remaining -= take;

        let front_len = match front {
            RxChunk::Arena(chunk) => chunk.len,
            RxChunk::Heap(data) => data.len(),
        };
        if conn.rx_chunk_off == front_len {
            if let Some(RxChunk::Arena(chunk)) = conn.rx_chunks.pop_front() {
                arenas.release(chunk);
            }
            conn.rx_chunk_off = 0;
        }
    }

    out
}

pub(super) fn rx_front_len(conn: &Connection) -> Option<usize> {
    conn.rx_chunks.front().map(|front| match front {
        RxChunk::Arena(chunk) => chunk.len.saturating_sub(conn.rx_chunk_off),
        RxChunk::Heap(data) => data.len().saturating_sub(conn.rx_chunk_off),
    })
}

pub(super) fn active_enabled(mode: &ActiveMode) -> bool {
    !matches!(mode, ActiveMode::False)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::runtime_reactor::BUF_SIZE;

    #[test]
    fn read_buf_lease_exposes_fixed_slot_identity() {
        let mut pool = LockedReadBufPool::new(1).expect("locked read pool");
        let lease = ReadBufLease::from_pool_or_heap_fallback(&mut pool);

        let slot = lease.slot_id().expect("fixed slot id");
        assert_eq!(lease.slot(), Some(slot.index));
        assert_eq!(lease.as_slice(&pool).map(|buf| buf.len()), Some(BUF_SIZE));
    }

    #[test]
    fn read_buf_lease_uses_heap_when_fixed_slots_are_exhausted() {
        let mut pool = LockedReadBufPool::new(1).expect("locked read pool");
        let first = ReadBufLease::from_pool_or_heap_fallback(&mut pool);
        let second = ReadBufLease::from_pool_or_heap_fallback(&mut pool);

        assert!(first.slot_id().is_some());
        assert!(second.slot_id().is_none());
        assert_eq!(second.as_slice(&pool).map(|buf| buf.len()), Some(BUF_SIZE));
        assert_eq!(second.slot(), None);
    }

    #[test]
    fn stale_fixed_slot_reports_invalid_storage_after_release() {
        let mut pool = LockedReadBufPool::new(1).expect("locked read pool");
        let mut lease = ReadBufLease::from_pool_or_heap_fallback(&mut pool);
        let slot = lease.slot_id().expect("fixed slot id");

        pool.release_fixed_slot(slot).expect("release slot");

        assert!(lease.fixed_slot_mut_view(&mut pool).is_none());
        assert_eq!(lease.len(&mut pool), 0);
        assert_eq!(lease.slot_id(), Some(slot));
        assert!(lease.as_slice(&pool).is_none());
        assert!(lease.as_mut_ptr(&mut pool).is_null());
    }

    #[test]
    fn stale_fixed_slot_keeps_identity_instead_of_downgrading_to_heap() {
        let mut pool = LockedReadBufPool::new(1).expect("locked read pool");
        let mut lease = ReadBufLease::from_pool_or_heap_fallback(&mut pool);
        let slot = lease.slot_id().expect("fixed slot id");

        pool.release_fixed_slot(slot).expect("release slot");

        assert!(lease.fixed_slot_mut_view(&mut pool).is_none());
        let ptr = lease.as_mut_ptr(&mut pool);
        assert!(ptr.is_null());
        assert_eq!(lease.slot_id(), Some(slot));
    }

    #[test]
    fn try_from_fixed_pool_returns_none_when_slots_are_exhausted() {
        let mut pool = LockedReadBufPool::new(1).expect("locked read pool");
        let first = ReadBufLease::try_from_fixed_pool(&mut pool);
        let second = ReadBufLease::try_from_fixed_pool(&mut pool);

        assert!(matches!(first, Some(ReadBufLease::Fixed(_))));
        assert!(second.is_none());
    }

    #[test]
    fn fixed_slot_stays_leased_until_completion_release_after_cancel() {
        let mut pool = LockedReadBufPool::new(1).expect("locked read pool");
        let lease = ReadBufLease::from_pool_or_heap_fallback(&mut pool);
        let slot = lease.slot_id().expect("fixed slot id");

        // Cancel intent alone must not free/reuse the fixed slot before CQE completion.
        assert!(pool.fixed_slot_slice(slot).is_some());
        assert!(pool.acquire_fixed_slot().is_none());

        // Completion+close path releases the lease exactly once.
        assert!(lease.release(&mut pool));
        let reacquired = pool
            .acquire_fixed_slot()
            .expect("slot must be reusable after release");
        assert_eq!(reacquired.index, slot.index);
        assert_ne!(reacquired.generation, slot.generation);
    }

    #[test]
    fn stale_slot_identity_cannot_free_after_release_reuse() {
        let mut pool = LockedReadBufPool::new(1).expect("locked read pool");
        let lease = ReadBufLease::from_pool_or_heap_fallback(&mut pool);
        let stale = lease.slot_id().expect("fixed slot id");

        assert!(lease.release(&mut pool));
        let reacquired = pool.acquire_fixed_slot().expect("reacquired slot");
        assert_eq!(reacquired.index, stale.index);
        assert_ne!(reacquired.generation, stale.generation);
        assert_eq!(
            pool.release_fixed_slot(stale),
            Err(crate::runtime_reactor::LockedReadReleaseError::InvalidSlotIdentity)
        );
    }
}
