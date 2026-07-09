use std::cell::RefCell;
use std::collections::HashMap;
use std::convert::TryFrom;
use std::os::fd::AsRawFd;

use io_uring::{opcode, squeue, types};

use crate::errors::{NifError, Result};
#[cfg(not(feature = "exec-strategy-sqpoll"))]
use crate::runtime::RecvRingMode;
use crate::runtime::{PendingStatx, ShardState, StatxOut, SubscribeOperation, WRITEV_BATCH};
use crate::runtime_adapter::ResultTarget;
use crate::runtime_arena::{ArenaClass, ArenaHandle};
use crate::runtime_reactor::{
    BufPoolKind, BufSlotId, Op, RingKind, BUF_SIZE, CONN_READ_POLL_MASK, CONN_WRITE_POLL_MASK,
    SUBSCRIBE_PAGE_SIZE,
};
use crate::runtime_session::PendingWrite;
use crate::runtime_stats::{monotonic_ns, stats_inc};

thread_local! {
    static RECV_MAILBOX_ON_SUBMIT: RefCell<HashMap<u64, bool>> = RefCell::new(HashMap::new());
}

fn retry_eintr<T, F>(mut op: F) -> Result<T>
where
    F: FnMut() -> std::io::Result<T>,
{
    loop {
        match op() {
            Ok(value) => return Ok(value),
            Err(err) if err.raw_os_error() == Some(libc::EINTR) => continue,
            Err(err) => {
                return Err(NifError::from_errno(
                    err.raw_os_error().unwrap_or(libc::EIO),
                ));
            }
        }
    }
}

#[inline]
fn read_fixed_buffer_index(
    read_fixed_base: u16,
    read_pool: &crate::runtime_reactor::LockedReadBufPool,
    slot_id: crate::runtime_reactor::BufSlotId,
) -> Option<u16> {
    let slot = read_pool.registered_read_index(slot_id)?;
    read_fixed_base.checked_add(slot)
}

#[inline]
fn subscribe_fixed_buffer_index(
    subscribe_fixed_base: Option<u16>,
    subscribe_fixed_slots: &[Option<u16>],
    page_slot: usize,
) -> Option<u16> {
    let base = subscribe_fixed_base?;
    let slot = (*subscribe_fixed_slots.get(page_slot)?)?;
    base.checked_add(slot)
}

#[inline]
fn is_aligned_for_statx(ptr: *const u8) -> bool {
    (ptr as usize).is_multiple_of(std::mem::align_of::<libc::statx>())
}

impl ShardState {
    #[inline]
    pub(super) fn set_recv_mailbox_snapshot(&self, session_id: u64, enabled: bool) {
        RECV_MAILBOX_ON_SUBMIT.with(|map| {
            map.borrow_mut().insert(session_id, enabled);
        });
    }

    #[inline]
    pub(super) fn recv_mailbox_snapshot(&self, session_id: u64) -> Option<bool> {
        RECV_MAILBOX_ON_SUBMIT.with(|map| map.borrow().get(&session_id).copied())
    }

    #[inline]
    pub(super) fn recv_mailbox_snapshot_for_conn_cqe(&self, session_id: u64) -> bool {
        let Some(snapshot) = self.recv_mailbox_snapshot(session_id) else {
            // Missing snapshots are treated as passive disabled for the CQE decision.
            // This keeps ConnRecv CQE behavior deterministic even if bookkeeping is
            // missing for some reason.
            return false;
        };
        // ConnRecv CQE handling must be snapshot-only. A missing snapshot is treated as
        // mailbox_passive disabled (false) so behavior is deterministic and testable.
        snapshot
    }

    #[inline]
    pub(super) fn clear_recv_mailbox_snapshot(&self, session_id: u64) {
        RECV_MAILBOX_ON_SUBMIT.with(|map| {
            map.borrow_mut().remove(&session_id);
        });
    }

    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    fn select_recv_ring_kind(&mut self, session_id: u64) -> RingKind {
        match self.recv_ring_mode {
            RecvRingMode::Latency => RingKind::Latency,
            RecvRingMode::Main => RingKind::Main,
            RecvRingMode::Both => {
                if (session_id & 1) == 0 {
                    RingKind::Latency
                } else {
                    RingKind::Main
                }
            }
        }
    }

    #[cfg(feature = "exec-strategy-sqpoll")]
    pub(super) fn queued_sqes(&mut self) -> usize {
        self.latency_ring.submission().len()
    }

    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    pub(super) fn queued_sqes(&mut self) -> usize {
        self.latency_ring.submission().len() + self.main_ring.submission().len()
    }

    #[cfg(feature = "exec-strategy-sqpoll")]
    pub(super) fn submit_queued(&mut self) -> Result<usize> {
        let mut submitted = 0usize;
        let mut sq = self.latency_ring.submission();
        sq.sync();
        let latency_len = sq.len();
        if latency_len > 0 {
            let needs_submit = sq.need_wakeup() || sq.cq_overflow();
            drop(sq);
            if needs_submit {
                submitted += retry_eintr(|| self.latency_ring.submitter().submit())?;
            }
        } else {
            drop(sq);
        }
        Ok(submitted)
    }

    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    pub(super) fn submit_queued(&mut self) -> Result<usize> {
        let mut submitted = 0usize;
        let latency_len = self.latency_ring.submission().len();
        if latency_len > 0 {
            submitted += retry_eintr(|| self.latency_ring.submitter().submit())?;
        }
        let main_len = self.main_ring.submission().len();
        if main_len > 0 {
            let now = monotonic_ns();
            let main_sqpoll = self.main_ring.params().is_setup_sqpoll();
            let should_submit = main_sqpoll
                || main_len >= self.main_submit_batch
                || self.submit_pressure
                // A pending receive can block the shard on the latency ring.
                // Do not leave a small main-ring batch below its threshold in
                // that state, or the peer can wait indefinitely for an echo.
                || self.has_latency_io_work()
                || now.saturating_sub(self.main_last_submit_ns) >= self.main_submit_max_delay_ns;
            if should_submit {
                submitted += retry_eintr(|| self.main_ring.submitter().submit())?;
                self.main_last_submit_ns = now;
            }
        }
        Ok(submitted)
    }

    pub(super) fn has_latency_io_work(&self) -> bool {
        self.latency_ops
            .values()
            .any(|op| !matches!(op, Op::Wakeup))
    }

    pub(super) fn provide_all_recv_buffers(&mut self) -> Result<()> {
        let Some(pool) = self.provided_recv_pool.as_ref() else {
            return Ok(());
        };
        let pool_len = pool.len();
        let mut slot_ids = Vec::new();
        slot_ids
            .try_reserve(pool_len)
            .map_err(|_| NifError::from_errno(libc::ENOMEM))?;
        for bid in 0..pool_len {
            if let Some(slot_id) = pool.slot_id_for_bid(bid as u16) {
                slot_ids.push(slot_id);
            }
        }
        for slot_id in slot_ids {
            self.reprovide_recv_slot(slot_id)?;
        }
        let _ = retry_eintr(|| self.latency_ring.submitter().submit());
        Ok(())
    }

    pub(super) fn enqueue_close_fd(&mut self, fd: i32) -> Result<u64> {
        let token = self.latency_ops.insert(Op::CloseFd { fd });
        let entry = opcode::Close::new(types::Fd(fd)).build().user_data(token);
        if let Err(err) = self.push_entry(RingKind::Latency, entry) {
            let _ = self.latency_ops.remove(token);
            return Err(err);
        }
        Ok(token)
    }

    pub(super) fn close_fd_or_defer(&mut self, fd: i32) {
        if self.enqueue_close_fd(fd).is_err() {
            self.deferred_close_fds.push_back(fd);
        }
    }

    pub(super) fn enqueue_provided_recv_pool_remove(&mut self) -> Result<bool> {
        let Some(pool) = self.provided_recv_pool.as_mut() else {
            return Ok(false);
        };
        debug_assert!(pool.is_draining() || pool.is_removed());
        // `RemoveBuffers` is the kernel-side handoff for slots still owned by
        // the provided-buffer group. During stop, those slots do not produce a
        // recv CQE merely because no connection remains, so requiring an empty
        // local in-flight set would leave the shard in Draining forever.
        if !pool.mark_remove_queued() {
            return Ok(false);
        }
        let nbufs = u16::try_from(pool.len()).map_err(|_| NifError::from_errno(libc::EINVAL))?;
        let bgid = pool.bgid;
        let token = self.latency_ops.insert(Op::ProvidedRecvPoolRemove);
        let entry = opcode::RemoveBuffers::new(nbufs, bgid)
            .build()
            .user_data(token);
        if let Err(err) = self.push_entry(RingKind::Latency, entry) {
            let _ = self.latency_ops.remove(token);
            if let Some(pool) = self.provided_recv_pool.as_mut() {
                pool.clear_remove_queued();
            }
            return Err(err);
        }
        Ok(true)
    }

    pub(super) fn push_entry(
        &mut self,
        ring_kind: RingKind,
        entry: io_uring::squeue::Entry,
    ) -> Result<()> {
        let entry = if self.pending_soft_link_sqe {
            entry.flags(squeue::Flags::IO_LINK)
        } else {
            entry
        };
        #[cfg(feature = "exec-strategy-sqpoll")]
        let ring = &mut self.latency_ring;
        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let ring = match ring_kind {
            RingKind::Latency => &mut self.latency_ring,
            RingKind::Main => &mut self.main_ring,
        };
        #[cfg(feature = "exec-strategy-sqpoll")]
        let _ = ring_kind;
        // SAFETY: `io_uring::SubmissionQueue::push` is unsafe because the kernel may later
        // dereference pointers captured inside SQEs. This helper only accepts fully-built
        // entries from callsites that keep any referenced storage alive until CQE completion,
        // and we call `sync()` before push attempts to maintain io_uring queue invariants.
        unsafe {
            let mut sq = ring.submission();
            sq.sync();
            if sq.push(&entry).is_err() {
                drop(sq);
                #[cfg(feature = "exec-strategy-sqpoll")]
                {
                    let mut sq = ring.submission();
                    sq.sync();
                    let need_submit = sq.need_wakeup() || sq.cq_overflow();
                    drop(sq);
                    if need_submit {
                        // We only call submit when the SQPOLL thread is known to be asleep or
                        // the SQ/CQ state requires explicit processing.
                        retry_eintr(|| ring.submitter().submit())?;
                    }
                    // When the SQ is full we must wait for a free slot before retrying the push.
                    retry_eintr(|| ring.submitter().squeue_wait())?;
                }
                #[cfg(not(feature = "exec-strategy-sqpoll"))]
                {
                    retry_eintr(|| ring.submit())?;
                }
                let mut sq = ring.submission();
                sq.sync();
                sq.push(&entry)
                    .map_err(|_| NifError::from_errno(libc::EBUSY))?;
            }
        }
        Ok(())
    }

    pub(super) fn arm_wakeup_poll(&mut self) -> Result<()> {
        let token = self.latency_ops.insert(Op::Wakeup);
        let entry = opcode::PollAdd::new(types::Fd(self.wakeup_fd), libc::POLLIN as _)
            .build()
            .user_data(token);
        if let Err(err) = self.push_entry(RingKind::Latency, entry) {
            let _ = self.latency_ops.remove(token);
            return Err(err);
        }
        Ok(())
    }

    pub(super) fn arm_listener_accept(&mut self, listener_id: u64) -> Result<()> {
        let Some(listener_fd) = self.listeners.get(&listener_id).map(|l| l.fd) else {
            return Ok(());
        };
        if self
            .listeners
            .get(&listener_id)
            .map(|l| l.accept_armed)
            .unwrap_or(false)
        {
            return Ok(());
        }
        let token = self.latency_ops.insert(Op::ListenerAccept {
            listener_id,
            armed_ns: monotonic_ns(),
        });
        let entry = opcode::Accept::new(
            types::Fd(listener_fd),
            std::ptr::null_mut(),
            std::ptr::null_mut(),
        )
        .build()
        .user_data(token);
        if let Err(err) = self.push_entry(RingKind::Latency, entry) {
            let _ = self.latency_ops.remove(token);
            return Err(err);
        }
        if let Some(listener) = self.listeners.get_mut(&listener_id) {
            listener.accept_armed = true;
            listener.accept_token = Some(token);
        }
        Ok(())
    }

    pub(super) fn cancel_listener_accept(&mut self, target_token: u64) -> Result<()> {
        let cancel_token = self
            .latency_ops
            .insert(Op::CancelListenerAccept(target_token));
        let cancel = opcode::AsyncCancel::new(target_token)
            .build()
            .user_data(cancel_token);
        if let Err(err) = self.push_entry(RingKind::Latency, cancel) {
            let _ = self.latency_ops.remove(cancel_token);
            return Err(err);
        }
        Ok(())
    }

    pub(super) fn arm_statx(
        &mut self,
        fd: i32,
        request_id: u64,
        target: ResultTarget,
    ) -> Result<()> {
        let statx_size = std::mem::size_of::<libc::statx>();
        let (out, statx_ptr) = if let Some(out) = self.arenas.acquire_class(ArenaClass::Small4K) {
            let out_buf = self.arenas.slice_mut_full(out);
            if out_buf.len() < statx_size {
                self.arenas.release(out);
                // SAFETY: `libc::statx` is a plain-old-data output struct written by the kernel.
                // Zero-initialization is valid and provides deterministic bytes before submission.
                let mut boxed = Box::new(unsafe { std::mem::zeroed::<libc::statx>() });
                let ptr = (&mut *boxed) as *mut libc::statx;
                (StatxOut::Heap(boxed), ptr)
            } else {
                out_buf[..statx_size].fill(0);
                let arena_ptr = out_buf.as_mut_ptr();
                if !is_aligned_for_statx(arena_ptr as *const u8) {
                    self.arenas.release(out);
                    // SAFETY: `libc::statx` is kernel output-only POD, so all-zero bytes are valid.
                    let mut boxed = Box::new(unsafe { std::mem::zeroed::<libc::statx>() });
                    let ptr = (&mut *boxed) as *mut libc::statx;
                    (StatxOut::Heap(boxed), ptr)
                } else {
                    (StatxOut::Arena(out), arena_ptr as *mut libc::statx)
                }
            }
        } else {
            // SAFETY: `libc::statx` is kernel output-only POD, so all-zero bytes are valid.
            let mut boxed = Box::new(unsafe { std::mem::zeroed::<libc::statx>() });
            let ptr = (&mut *boxed) as *mut libc::statx;
            (StatxOut::Heap(boxed), ptr)
        };
        debug_assert!(
            is_aligned_for_statx(statx_ptr as *const u8),
            "statx output pointer must satisfy libc::statx alignment",
        );
        let token = self.latency_ops.insert(Op::Statx(request_id));
        self.pending_statx.insert(
            request_id,
            PendingStatx {
                fd,
                request_id,
                target,
                out,
            },
        );
        let entry = opcode::Statx::new(types::Fd(fd), c"".as_ptr(), statx_ptr as *mut types::statx)
            .flags(libc::AT_EMPTY_PATH)
            .mask(libc::STATX_SIZE)
            .build()
            .user_data(token);

        if let Err(err) = self.push_entry(RingKind::Latency, entry) {
            let _ = self.latency_ops.remove(token);
            if let Some(pending) = self.pending_statx.remove(&request_id) {
                if let StatxOut::Arena(handle) = pending.out {
                    self.arenas.release(handle);
                }
            }
            return Err(err);
        }

        Ok(())
    }

    pub(super) fn arm_conn_read_poll(&mut self, session_id: u64) -> Result<()> {
        let Some(fd) = self.conns.get(&session_id).map(|c| c.fd.as_raw_fd()) else {
            return Ok(());
        };
        if self
            .conns
            .get(&session_id)
            .map(|c| c.read_poll_armed)
            .unwrap_or(false)
        {
            return Ok(());
        }
        let token = self.latency_ops.insert(Op::ConnReadPoll(session_id));
        let entry = opcode::PollAdd::new(types::Fd(fd), CONN_READ_POLL_MASK)
            .build()
            .user_data(token);
        if let Err(err) = self.push_entry(RingKind::Latency, entry) {
            let _ = self.latency_ops.remove(token);
            return Err(err);
        }
        stats_inc!(self.stats, read_polls);
        if let Some(conn) = self.conns.get_mut(&session_id) {
            conn.read_poll_armed = true;
        }
        Ok(())
    }

    pub(super) fn arm_conn_recv(&mut self, session_id: u64) -> Result<()> {
        let use_fixed = self.read_fixed_enabled;
        let use_provided = self
            .provided_recv_pool
            .as_ref()
            .map(|pool| pool.is_active())
            .unwrap_or(false);
        let Some((fd, buf_ptr, buf_len, fixed_slot_id)) =
            self.conns.get_mut(&session_id).and_then(|c| {
                c.read_in_flight = true;
                c.read_in_flight_token = None;
                c.read_in_flight_ring = None;
                let fixed_slot_id = c.read_buf.slot_id();
                let (buf_ptr, buf_len) = match fixed_slot_id {
                    Some(_) => c.read_buf.fixed_slot_mut_view(&mut self.read_pool)?,
                    None => (
                        c.read_buf.as_mut_ptr(&mut self.read_pool),
                        c.read_buf.len(&mut self.read_pool),
                    ),
                };
                Some((c.fd.as_raw_fd(), buf_ptr, buf_len as u32, fixed_slot_id))
            })
        else {
            return Ok(());
        };
        if use_fixed && fixed_slot_id.is_some() && (buf_ptr.is_null() || buf_len == 0) {
            if let Some(conn) = self.conns.get_mut(&session_id) {
                conn.read_in_flight = false;
                conn.read_in_flight_token = None;
                conn.read_in_flight_ring = None;
            }
            return Err(NifError::from_errno(libc::EIO));
        }
        #[cfg(feature = "exec-strategy-sqpoll")]
        let recv_ring = RingKind::Latency;
        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let recv_ring = self.select_recv_ring_kind(session_id);

        #[cfg(feature = "exec-strategy-sqpoll")]
        let token = self.latency_ops.insert(Op::ConnRecv(session_id));
        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let token = match recv_ring {
            RingKind::Latency => self.latency_ops.insert(Op::ConnRecv(session_id)),
            RingKind::Main => self.main_ops.insert(Op::ConnRecv(session_id)),
        };
        let entry = if use_provided {
            let bgid = self
                .provided_recv_pool
                .as_ref()
                .map(|pool| pool.bgid)
                .unwrap_or(0);
            opcode::Recv::new(types::Fd(fd), std::ptr::null_mut(), BUF_SIZE as u32)
                .buf_group(bgid)
                .build()
                .flags(squeue::Flags::BUFFER_SELECT)
                .user_data(token)
        } else {
            match (use_fixed, fixed_slot_id) {
                (true, Some(slot_id)) => {
                    match read_fixed_buffer_index(self.read_fixed_base, &self.read_pool, slot_id) {
                        Some(buffer_index) => {
                            opcode::ReadFixed::new(types::Fd(fd), buf_ptr, buf_len, buffer_index)
                                .offset(0)
                                .build()
                                .user_data(token)
                        }
                        None => {
                            #[cfg(feature = "exec-strategy-sqpoll")]
                            let _ = self.latency_ops.remove(token);
                            #[cfg(not(feature = "exec-strategy-sqpoll"))]
                            match recv_ring {
                                RingKind::Latency => {
                                    let _ = self.latency_ops.remove(token);
                                }
                                RingKind::Main => {
                                    let _ = self.main_ops.remove(token);
                                }
                            }
                            if let Some(conn) = self.conns.get_mut(&session_id) {
                                conn.read_in_flight = false;
                                conn.read_in_flight_token = None;
                                conn.read_in_flight_ring = None;
                            }
                            return Err(NifError::from_errno(libc::EIO));
                        }
                    }
                }
                _ => opcode::Recv::new(types::Fd(fd), buf_ptr, buf_len)
                    .build()
                    .user_data(token),
            }
        };
        if let Err(err) = self.push_entry(recv_ring, entry) {
            #[cfg(feature = "exec-strategy-sqpoll")]
            let _ = self.latency_ops.remove(token);
            #[cfg(not(feature = "exec-strategy-sqpoll"))]
            match recv_ring {
                RingKind::Latency => {
                    let _ = self.latency_ops.remove(token);
                }
                RingKind::Main => {
                    let _ = self.main_ops.remove(token);
                }
            }
            if let Some(conn) = self.conns.get_mut(&session_id) {
                conn.read_in_flight = false;
                conn.read_in_flight_token = None;
                conn.read_in_flight_ring = None;
            }
            return Err(err);
        }
        if let Some(conn) = self.conns.get_mut(&session_id) {
            conn.read_in_flight_token = Some(token);
            conn.read_in_flight_ring = Some(recv_ring);
        }
        stats_inc!(self.stats, recv_sqes);
        Ok(())
    }

    pub(super) fn cancel_conn_recv(&mut self, session_id: u64) -> Result<()> {
        let Some((target_token, ring_kind)) = self.conns.get(&session_id).and_then(|conn| {
            if !conn.read_in_flight {
                return None;
            }
            Some((conn.read_in_flight_token?, conn.read_in_flight_ring?))
        }) else {
            return Ok(());
        };
        let cancel_token = match ring_kind {
            RingKind::Latency => self.latency_ops.insert(Op::CancelRecv(session_id)),
            RingKind::Main => self.main_ops.insert(Op::CancelRecv(session_id)),
        };
        let cancel = opcode::AsyncCancel::new(target_token)
            .build()
            .user_data(cancel_token);
        if let Err(err) = self.push_entry(ring_kind, cancel) {
            match ring_kind {
                RingKind::Latency => {
                    let _ = self.latency_ops.remove(cancel_token);
                }
                RingKind::Main => {
                    let _ = self.main_ops.remove(cancel_token);
                }
            }
            return Err(err);
        }
        Ok(())
    }

    pub(super) fn arm_subscription(
        &mut self,
        subscription_id: u64,
        slot_idx: usize,
        fd: i32,
        operation: SubscribeOperation,
        page_slot: Option<ArenaHandle>,
    ) -> Result<()> {
        let op = match operation {
            SubscribeOperation::Read => Op::SubscribeRead {
                subscription_id,
                slot_idx: slot_idx as u8,
            },
            SubscribeOperation::Accept => Op::SubscribeAccept {
                subscription_id,
                slot_idx: slot_idx as u8,
                armed_ns: monotonic_ns(),
            },
        };
        let token = self.latency_ops.insert(op);
        let entry = match operation {
            SubscribeOperation::Read => {
                let Some(page_handle) = page_slot else {
                    let _ = self.latency_ops.remove(token);
                    return Err(NifError::from_errno(libc::EINVAL));
                };
                if page_handle.class != ArenaClass::Small4K {
                    let _ = self.latency_ops.remove(token);
                    return Err(NifError::from_errno(libc::EINVAL));
                }
                let buf = self.arenas.slice_mut_full(page_handle);
                if self.subscribe_fixed_enabled {
                    if let Some(buffer_index) = subscribe_fixed_buffer_index(
                        self.subscribe_fixed_base,
                        &self.subscribe_fixed_slots,
                        page_handle.slot,
                    ) {
                        opcode::ReadFixed::new(
                            types::Fd(fd),
                            buf.as_mut_ptr(),
                            SUBSCRIBE_PAGE_SIZE as u32,
                            buffer_index,
                        )
                        .offset(0)
                        .build()
                        .user_data(token)
                    } else {
                        let _ = self.latency_ops.remove(token);
                        return Err(NifError::from_errno(libc::EIO));
                    }
                } else {
                    opcode::Read::new(types::Fd(fd), buf.as_mut_ptr(), SUBSCRIBE_PAGE_SIZE as u32)
                        .offset(u64::MAX)
                        .build()
                        .user_data(token)
                }
            }
            SubscribeOperation::Accept => {
                opcode::Accept::new(types::Fd(fd), std::ptr::null_mut(), std::ptr::null_mut())
                    .build()
                    .user_data(token)
            }
        };
        if let Err(err) = self.push_entry(RingKind::Latency, entry) {
            let _ = self.latency_ops.remove(token);
            return Err(err);
        }
        Ok(())
    }

    pub(super) fn reprovide_recv_slot(&mut self, slot_id: BufSlotId) -> Result<()> {
        let Some(entry) = self
            .provided_recv_pool
            .as_mut()
            .and_then(|pool| pool.provide_entry_for_slot(slot_id))
        else {
            return if slot_id.pool == BufPoolKind::ProvidedRecv {
                Ok(())
            } else {
                Err(NifError::from_errno(libc::EINVAL))
            };
        };
        stats_inc!(self.stats, reprovide_sqes);
        self.push_entry(RingKind::Latency, entry)
    }

    pub(super) fn arm_conn_write_poll(&mut self, session_id: u64) -> Result<()> {
        let Some(fd) = self.conns.get(&session_id).map(|c| c.fd.as_raw_fd()) else {
            return Ok(());
        };
        if self
            .conns
            .get(&session_id)
            .map(|c| c.write_poll_armed)
            .unwrap_or(false)
        {
            return Ok(());
        }
        #[cfg(feature = "exec-strategy-sqpoll")]
        let token = self.latency_ops.insert(Op::ConnWritePoll(session_id));
        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let token = self.main_ops.insert(Op::ConnWritePoll(session_id));
        let entry = opcode::PollAdd::new(types::Fd(fd), CONN_WRITE_POLL_MASK)
            .build()
            .user_data(token);
        if let Err(err) = self.push_entry(RingKind::Main, entry) {
            #[cfg(feature = "exec-strategy-sqpoll")]
            let _ = self.latency_ops.remove(token);
            #[cfg(not(feature = "exec-strategy-sqpoll"))]
            let _ = self.main_ops.remove(token);
            return Err(err);
        }
        stats_inc!(self.stats, write_polls);
        if let Some(conn) = self.conns.get_mut(&session_id) {
            conn.write_poll_armed = true;
        }
        Ok(())
    }

    pub(super) fn arm_conn_write(&mut self, session_id: u64) -> Result<()> {
        let Some((fd, single_buf, iov_ptr, iov_count)) = self.conns.get_mut(&session_id).map(|c| {
            c.write_iovecs.clear();
            for write in c.pending_writes.iter().take(WRITEV_BATCH) {
                let (ptr, len) = match write {
                    PendingWrite::Arena(chunk) => {
                        let data = self.arenas.slice_handle(*chunk);
                        (data.as_ptr(), data.len())
                    }
                    PendingWrite::Heap { data, off } => (data[*off..].as_ptr(), data.len() - *off),
                };
                c.write_iovecs.push(libc::iovec {
                    iov_base: ptr as *mut libc::c_void,
                    iov_len: len,
                });
            }
            c.write_in_flight = true;
            let single_buf = if c.write_iovecs.len() == 1 {
                let only = &c.write_iovecs[0];
                Some((only.iov_base as *const u8, only.iov_len as u32))
            } else {
                None
            };
            (
                c.fd.as_raw_fd(),
                single_buf,
                c.write_iovecs.as_ptr(),
                c.write_iovecs.len() as u32,
            )
        }) else {
            return Ok(());
        };
        #[cfg(feature = "exec-strategy-sqpoll")]
        let token = self.latency_ops.insert(Op::ConnWrite(session_id));
        #[cfg(not(feature = "exec-strategy-sqpoll"))]
        let token = self.main_ops.insert(Op::ConnWrite(session_id));
        let entry = if let Some((buf_ptr, buf_len)) = single_buf {
            opcode::Send::new(types::Fd(fd), buf_ptr, buf_len)
                .flags(libc::MSG_NOSIGNAL)
                .build()
                .user_data(token)
        } else {
            opcode::Writev::new(types::Fd(fd), iov_ptr, iov_count)
                .build()
                .user_data(token)
        };
        if let Err(err) = self.push_entry(RingKind::Main, entry) {
            #[cfg(feature = "exec-strategy-sqpoll")]
            let _ = self.latency_ops.remove(token);
            #[cfg(not(feature = "exec-strategy-sqpoll"))]
            let _ = self.main_ops.remove(token);
            if let Some(conn) = self.conns.get_mut(&session_id) {
                conn.write_in_flight = false;
                conn.write_iovecs.clear();
            }
            return Err(err);
        }
        stats_inc!(self.stats, write_sqes);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::{read_fixed_buffer_index, subscribe_fixed_buffer_index};
    use crate::runtime_reactor::LockedReadBufPool;

    #[test]
    fn read_fixed_buffer_index_applies_explicit_base() {
        let mut pool = LockedReadBufPool::new(8).expect("locked read pool");
        for _ in 0..7 {
            let _ = pool.acquire_fixed_slot().expect("skip slot");
        }
        let slot = pool.acquire_fixed_slot().expect("slot");
        assert_eq!(read_fixed_buffer_index(11, &pool, slot), Some(18));
    }

    #[test]
    fn read_fixed_buffer_index_rejects_stale_slot_or_overflow() {
        let mut pool = LockedReadBufPool::new(2).expect("locked read pool");
        let slot = pool.acquire_fixed_slot().expect("slot");
        pool.release_fixed_slot(slot)
            .expect("release stale-check slot");
        assert_eq!(read_fixed_buffer_index(11, &pool, slot), None);

        let mut pool = LockedReadBufPool::new(2).expect("locked read pool");
        let _ = pool.acquire_fixed_slot().expect("first slot");
        let slot = pool.acquire_fixed_slot().expect("second slot");
        assert_eq!(read_fixed_buffer_index(u16::MAX, &pool, slot), None);
    }

    #[test]
    fn subscribe_fixed_buffer_index_uses_explicit_slot_map() {
        assert_eq!(
            subscribe_fixed_buffer_index(Some(11), &[Some(2), Some(4), Some(6)], 1),
            Some(15)
        );
    }

    #[test]
    fn subscribe_fixed_buffer_index_rejects_missing_slot_or_overflow() {
        assert_eq!(subscribe_fixed_buffer_index(Some(11), &[Some(2)], 9), None);
        assert_eq!(
            subscribe_fixed_buffer_index(Some(u16::MAX), &[Some(1)], 0),
            None
        );
        assert_eq!(subscribe_fixed_buffer_index(None, &[Some(1)], 0), None);
    }
}
