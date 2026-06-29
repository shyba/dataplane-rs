use io_uring::cqueue;
use std::os::fd::{FromRawFd, OwnedFd};

use crate::errors::NifError;
use crate::runtime::{
    advance_active_mode, stat_size_via_fstat, stop_drain_profile_enabled, ReadyOp, ShardState,
    StatxOut, SubscribeOperation, CQE_BUDGET, RX_QUEUE_MAX_BYTES,
};
use crate::runtime_adapter::{send_active_data_message, target_owner_pid};
use crate::runtime_reactor::{Op, RingKind};
use crate::runtime_session::{active_enabled, rx_push_from_read_buf, rx_push_slice, PendingWrite};
use crate::runtime_stats::{monotonic_ns, stats_add, stats_inc, stats_max};

impl ShardState {
    pub(super) fn resolve_provided_recv_slot(
        &self,
        bid: u16,
    ) -> Option<crate::runtime_reactor::BufSlotId> {
        let pool = self.provided_recv_pool.as_ref()?;
        pool.resolve_inflight_slot(bid)
    }

    fn finish_stop_pending_recv(
        &mut self,
        session_id: u64,
        selected_slot: Option<crate::runtime_reactor::BufSlotId>,
    ) {
        if let Some(slot_id) = selected_slot {
            if let Some(pool) = self.provided_recv_pool.as_mut() {
                let _ = pool.complete_recv(slot_id);
            }
        }
        self.close_connection(session_id, NifError::Closed);
    }

    pub(super) fn handle_recv_enobufs(&mut self, session_id: u64) {
        if let Some(pool) = self.provided_recv_pool.as_mut() {
            pool.begin_draining();
        }
        self.enqueue_latency_task(ReadyOp::Conn(session_id));
    }

    #[cfg_attr(not(feature = "exec-strategy-sqpoll"), allow(dead_code))]
    pub(super) fn process_completions(&mut self, ring_kind: RingKind) -> usize {
        self.process_completions_budget(ring_kind, CQE_BUDGET)
    }

    #[cfg(not(feature = "exec-strategy-sqpoll"))]
    pub(super) fn process_completions_fair_budget(&mut self, budget: usize) -> (usize, usize) {
        const FAIR_CQE_CHUNK: usize = 32;
        let mut latency_cqes = 0usize;
        let mut main_cqes = 0usize;
        let mut remaining = budget;
        let mut main_first = self.next_cqe_main_first;
        self.next_cqe_main_first = !self.next_cqe_main_first;

        while remaining > 0 {
            let chunk = remaining.min(FAIR_CQE_CHUNK);
            let (first_ring, second_ring) = if main_first {
                (RingKind::Main, RingKind::Latency)
            } else {
                (RingKind::Latency, RingKind::Main)
            };

            let first_done = self.process_completions_budget(first_ring, chunk);
            match first_ring {
                RingKind::Latency => latency_cqes += first_done,
                RingKind::Main => main_cqes += first_done,
            }
            remaining = remaining.saturating_sub(first_done);
            if remaining == 0 {
                break;
            }

            let second_budget = chunk.min(remaining);
            let second_done = self.process_completions_budget(second_ring, second_budget);
            match second_ring {
                RingKind::Latency => latency_cqes += second_done,
                RingKind::Main => main_cqes += second_done,
            }
            remaining = remaining.saturating_sub(second_done);

            if first_done == 0 && second_done == 0 {
                break;
            }
            main_first = !main_first;
        }

        (latency_cqes, main_cqes)
    }

    #[allow(dead_code)]
    #[cfg(feature = "exec-strategy-sqpoll")]
    pub(super) fn process_completions_fair_budget(&mut self, budget: usize) -> (usize, usize) {
        (
            self.process_completions_budget(RingKind::Latency, budget),
            0,
        )
    }

    pub(super) fn process_completions_budget(
        &mut self,
        ring_kind: RingKind,
        budget: usize,
    ) -> usize {
        let mut processed = 0usize;
        loop {
            if processed >= budget {
                break;
            }
            let cqe = match ring_kind {
                RingKind::Latency => self.latency_ring.completion().next(),
                #[cfg(not(feature = "exec-strategy-sqpoll"))]
                RingKind::Main => self.main_ring.completion().next(),
                #[cfg(feature = "exec-strategy-sqpoll")]
                RingKind::Main => None,
            };
            let Some(cqe) = cqe else {
                break;
            };
            processed += 1;
            match ring_kind {
                RingKind::Latency => stats_inc!(self.stats, latency_cqes),
                #[cfg(not(feature = "exec-strategy-sqpoll"))]
                RingKind::Main => stats_inc!(self.stats, main_cqes),
                #[cfg(feature = "exec-strategy-sqpoll")]
                RingKind::Main => {}
            }
            let token = cqe.user_data();
            let op = match ring_kind {
                RingKind::Latency => self.latency_ops.remove(token),
                #[cfg(not(feature = "exec-strategy-sqpoll"))]
                RingKind::Main => self.main_ops.remove(token),
                #[cfg(feature = "exec-strategy-sqpoll")]
                RingKind::Main => None,
            };
            let Some(op) = op else {
                continue;
            };
            match op {
                Op::ListenerAccept {
                    listener_id,
                    armed_ns,
                } => self.handle_accept_cqe(listener_id, cqe.result(), armed_ns),
                Op::ConnRecv(session_id) => {
                    let mailbox_passive = self.recv_mailbox_snapshot_for_conn_cqe(session_id);
                    self.handle_recv_cqe(
                        session_id,
                        cqe.result(),
                        cqe.flags(),
                        mailbox_passive,
                        token,
                    )
                }
                Op::CancelRecv(_session_id) => {
                    if stop_drain_profile_enabled() {
                        stats_inc!(self.stats, cancel_recv_cqes);
                    }
                    let _ = cqe.result();
                }
                Op::CancelListenerAccept(_token) => {
                    if stop_drain_profile_enabled() {
                        stats_inc!(self.stats, cancel_listener_accept_cqes);
                    }
                    let _ = cqe.result();
                }
                Op::ConnWrite(session_id) => self.handle_write_cqe(session_id, cqe.result(), token),
                Op::ConnReadPoll(session_id) => {
                    if let Some(conn) = self.conns.get_mut(&session_id) {
                        let stop_close_pending = conn.stop_close_on_read_poll_cqe;
                        if stop_close_pending {
                            conn.stop_close_on_read_poll_cqe = false;
                        }
                        conn.read_poll_armed = false;
                        if stop_close_pending {
                            self.close_connection(session_id, NifError::Closed);
                        } else {
                            self.enqueue_latency_task(ReadyOp::Conn(session_id));
                        }
                    }
                }
                Op::ConnWritePoll(session_id) => {
                    if let Some(conn) = self.conns.get_mut(&session_id) {
                        let stop_close_pending = conn.stop_close_on_write_poll_cqe;
                        if stop_close_pending {
                            conn.stop_close_on_write_poll_cqe = false;
                        }
                        conn.write_poll_armed = false;
                        if stop_close_pending {
                            self.close_connection(session_id, NifError::Closed);
                        } else {
                            self.enqueue_latency_task(ReadyOp::Conn(session_id));
                        }
                    }
                }
                Op::ProvidedRecvPoolRemove => {
                    self.handle_provided_recv_pool_remove_cqe(cqe.result())
                }
                Op::CloseFd { fd } => self.handle_close_fd_cqe(fd, cqe.result()),
                Op::SubscribeRead {
                    subscription_id,
                    slot_idx,
                } => {
                    self.handle_subscribe_read_cqe(subscription_id, slot_idx as usize, cqe.result())
                }
                Op::SubscribeAccept {
                    subscription_id,
                    slot_idx,
                    armed_ns,
                } => self.handle_subscribe_accept_cqe(
                    subscription_id,
                    slot_idx as usize,
                    cqe.result(),
                    armed_ns,
                ),
                Op::Statx(request_id) => self.handle_statx_cqe(request_id, cqe.result()),
                Op::Wakeup => {
                    loop {
                        // SAFETY: `wakeup_fd` is created as an eventfd and kept open for shard
                        // lifetime, and `wakeup_buf` has at least 8 bytes for eventfd reads.
                        let rc = unsafe {
                            libc::read(
                                self.wakeup_fd,
                                self.wakeup_buf.as_mut_ptr() as *mut libc::c_void,
                                8,
                            )
                        };
                        if rc == 8 {
                            continue;
                        }
                        if rc < 0 {
                            let err = std::io::Error::last_os_error().raw_os_error().unwrap_or(0);
                            if err == libc::EINTR {
                                continue;
                            }
                            if err == libc::EAGAIN || err == libc::EWOULDBLOCK {
                                break;
                            }
                        }
                        break;
                    }
                    let _ = self.arm_wakeup_poll();
                }
            }
        }
        processed
    }

    pub(super) fn handle_accept_cqe(&mut self, listener_id: u64, result: i32, armed_ns: u64) {
        if result >= 0 {
            self.stats
                .record_listener_accept_wait(monotonic_ns().saturating_sub(armed_ns));
        }
        let Some((owner, accept_tx)) = self
            .listeners
            .get(&listener_id)
            .map(|listener| (listener.owner, listener.accept_tx.clone()))
        else {
            return;
        };
        if let Some(listener) = self.listeners.get_mut(&listener_id) {
            listener.accept_armed = false;
            listener.accept_token = None;
        }

        if result >= 0 {
            self.install_connection(owner, accept_tx, result);
            let _ = self.arm_listener_accept(listener_id);
            return;
        }

        let errno = -result;
        if errno == libc::ECANCELED || errno == libc::EBADF {
            return;
        }
        let _ = self.arm_listener_accept(listener_id);
    }

    pub(super) fn handle_recv_cqe(
        &mut self,
        session_id: u64,
        result: i32,
        flags: u32,
        mailbox_passive_on_submit: bool,
        cqe_ref: u64,
    ) {
        self.clear_recv_mailbox_snapshot(session_id);

        let Some(conn) = self.conns.get_mut(&session_id) else {
            return;
        };
        stats_inc!(self.stats, recv_cqes);
        let cqe_cancels_pending_recv = conn
            .canceled_recv_token
            .is_some_and(|token| token == cqe_ref);
        if cqe_cancels_pending_recv {
            conn.canceled_recv_token = None;
        }
        let stop_close_pending = conn.stop_close_on_recv_cqe;
        if stop_close_pending {
            conn.stop_close_on_recv_cqe = false;
        }
        conn.read_in_flight = false;
        conn.read_in_flight_token = None;
        conn.read_in_flight_ring = None;

        if result == 0 {
            self.close_connection(session_id, NifError::Closed);
            return;
        }
        if result < 0 {
            let errno = -result;
            if stop_close_pending {
                self.close_connection(session_id, NifError::Closed);
                return;
            }
            if errno == libc::EINTR {
                self.enqueue_latency_task(ReadyOp::Conn(session_id));
                return;
            }
            if errno == libc::ENOBUFS {
                self.handle_recv_enobufs(session_id);
                return;
            }
            if errno == libc::EAGAIN || errno == libc::EWOULDBLOCK {
                stats_inc!(self.stats, recv_wouldblock);
                stats_inc!(self.stats, read_poll_from_recv_eagain);
                let _ = self.arm_conn_read_poll(session_id);
                return;
            }
            if errno == libc::ECANCELED || errno == libc::EBADF {
                // Cancel CQEs only clear the in-flight marker here. Buffer reclamation
                // still happens on the terminal recv/close path so cancellation cannot
                // free provided or fixed receive storage ahead of the real completion.
                if !cqe_cancels_pending_recv {
                    if let Some(pending) = conn.pending_recv.take() {
                        self.reply_pending_recv(pending, Err(NifError::from_errno(errno)));
                    }
                }
                return;
            }
            self.close_connection(session_id, NifError::from_errno(errno));
            return;
        }

        let n = result as usize;
        stats_add!(self.stats, recv_bytes, n as u64);
        let selected_bid = cqueue::buffer_select(flags);
        let selected_slot = match selected_bid {
            Some(bid) => match self.resolve_provided_recv_slot(bid) {
                Some(slot_id) => Some(slot_id),
                None => {
                    self.close_connection(session_id, NifError::from_errno(libc::EIO));
                    return;
                }
            },
            None => None,
        };
        if stop_close_pending {
            self.finish_stop_pending_recv(session_id, selected_slot);
            return;
        }
        let mut rearm_immediately = false;
        let mut had_pending_batch = false;
        let mut pending_reply = None;
        let mut enqueue_conn_ready = false;
        if let Some(conn) = self.conns.get_mut(&session_id) {
            had_pending_batch = conn.pending_batch.is_some();
            if had_pending_batch {
                if let Some(slot_id) = selected_slot {
                    let data = self
                        .provided_recv_pool
                        .as_ref()
                        .and_then(|pool| pool.slice(slot_id, n));
                    let Some(data) = data else {
                        self.close_connection(session_id, NifError::from_errno(libc::EIO));
                        return;
                    };
                    stats_inc!(self.stats, recv_buf_append_count);
                    stats_add!(self.stats, recv_buf_append_bytes, data.len() as u64);
                    rx_push_slice(conn, &mut self.arenas, data);
                } else {
                    stats_inc!(self.stats, recv_buf_append_count);
                    stats_add!(self.stats, recv_buf_append_bytes, n as u64);
                    if !rx_push_from_read_buf(conn, &self.read_pool, &mut self.arenas, 0, n) {
                        self.close_connection(session_id, NifError::from_errno(libc::EIO));
                        return;
                    }
                }
                stats_max!(self.stats, rx_queue_peak, conn.rx_bytes);
                rearm_immediately = conn.rx_bytes < RX_QUEUE_MAX_BYTES;
            } else if let Some(pending) = if cqe_cancels_pending_recv {
                None
            } else {
                conn.pending_recv.take()
            } {
                if let Some(slot_id) = selected_slot {
                    let data = self
                        .provided_recv_pool
                        .as_ref()
                        .and_then(|pool| pool.slice(slot_id, n));
                    let Some(data) = data else {
                        self.close_connection(session_id, NifError::from_errno(libc::EIO));
                        return;
                    };
                    let take = if pending.len == 0 {
                        n
                    } else {
                        pending.len.min(n)
                    };
                    let out = data[..take].to_vec();
                    if take < n {
                        let tail = &data[take..];
                        stats_inc!(self.stats, recv_buf_append_count);
                        stats_add!(self.stats, recv_buf_append_bytes, tail.len() as u64);
                        stats_inc!(self.stats, recv_buf_tail_append_count);
                        stats_add!(self.stats, recv_buf_tail_append_bytes, tail.len() as u64);
                        rx_push_slice(conn, &mut self.arenas, tail);
                    }
                    pending_reply = Some((pending, Ok(out)));
                } else {
                    let take = if pending.len == 0 {
                        n
                    } else {
                        pending.len.min(n)
                    };
                    let Some(read_data) = conn.read_buf.as_slice(&self.read_pool) else {
                        self.close_connection(session_id, NifError::from_errno(libc::EIO));
                        return;
                    };
                    if read_data.len() < take {
                        self.close_connection(session_id, NifError::from_errno(libc::EIO));
                        return;
                    }
                    let out = read_data[..take].to_vec();
                    if take < n {
                        let tail_len = n - take;
                        stats_inc!(self.stats, recv_buf_append_count);
                        stats_add!(self.stats, recv_buf_append_bytes, tail_len as u64);
                        stats_inc!(self.stats, recv_buf_tail_append_count);
                        stats_add!(self.stats, recv_buf_tail_append_bytes, tail_len as u64);
                        if !rx_push_from_read_buf(
                            conn,
                            &self.read_pool,
                            &mut self.arenas,
                            take,
                            tail_len,
                        ) {
                            self.close_connection(session_id, NifError::from_errno(libc::EIO));
                            return;
                        }
                    }
                    pending_reply = Some((pending, Ok(out)));
                }
                rearm_immediately = if active_enabled(&conn.active_mode) {
                    active_enabled(&conn.active_mode)
                } else {
                    conn.rx_bytes < RX_QUEUE_MAX_BYTES
                };
            } else if active_enabled(&conn.active_mode) {
                if let Some(slot_id) = selected_slot {
                    let data = self
                        .provided_recv_pool
                        .as_ref()
                        .and_then(|pool| pool.slice(slot_id, n));
                    let Some(data) = data else {
                        self.close_connection(session_id, NifError::from_errno(libc::EIO));
                        return;
                    };
                    send_active_data_message(&conn.handle, &conn.owner, data);
                } else {
                    let Some(read_data) = conn.read_buf.as_slice(&self.read_pool) else {
                        self.close_connection(session_id, NifError::from_errno(libc::EIO));
                        return;
                    };
                    if read_data.len() < n {
                        self.close_connection(session_id, NifError::from_errno(libc::EIO));
                        return;
                    }
                    send_active_data_message(&conn.handle, &conn.owner, &read_data[..n]);
                }
                advance_active_mode(&conn.handle, &conn.owner, &mut conn.active_mode);
                rearm_immediately = active_enabled(&conn.active_mode);
            } else if mailbox_passive_on_submit {
                if let Some(slot_id) = selected_slot {
                    let data = self
                        .provided_recv_pool
                        .as_ref()
                        .and_then(|pool| pool.slice(slot_id, n));
                    let Some(data) = data else {
                        self.close_connection(session_id, NifError::from_errno(libc::EIO));
                        return;
                    };
                    stats_inc!(self.stats, recv_buf_append_count);
                    stats_add!(self.stats, recv_buf_append_bytes, data.len() as u64);
                    rx_push_slice(conn, &mut self.arenas, data);
                } else {
                    stats_inc!(self.stats, recv_buf_append_count);
                    stats_add!(self.stats, recv_buf_append_bytes, n as u64);
                    if !rx_push_from_read_buf(conn, &self.read_pool, &mut self.arenas, 0, n) {
                        self.close_connection(session_id, NifError::from_errno(libc::EIO));
                        return;
                    }
                }
                stats_max!(self.stats, rx_queue_peak, conn.rx_bytes);
                enqueue_conn_ready = true;
                rearm_immediately = conn.rx_bytes < RX_QUEUE_MAX_BYTES;
            } else {
                if let Some(slot_id) = selected_slot {
                    let data = self
                        .provided_recv_pool
                        .as_ref()
                        .and_then(|pool| pool.slice(slot_id, n));
                    let Some(data) = data else {
                        self.close_connection(session_id, NifError::from_errno(libc::EIO));
                        return;
                    };
                    stats_inc!(self.stats, recv_buf_append_count);
                    stats_add!(self.stats, recv_buf_append_bytes, data.len() as u64);
                    rx_push_slice(conn, &mut self.arenas, data);
                } else {
                    stats_inc!(self.stats, recv_buf_append_count);
                    stats_add!(self.stats, recv_buf_append_bytes, n as u64);
                    if !rx_push_from_read_buf(conn, &self.read_pool, &mut self.arenas, 0, n) {
                        self.close_connection(session_id, NifError::from_errno(libc::EIO));
                        return;
                    }
                }
                stats_max!(self.stats, rx_queue_peak, conn.rx_bytes);
                rearm_immediately = conn.rx_bytes < RX_QUEUE_MAX_BYTES;
            }
        }

        if let Some((pending, reply)) = pending_reply {
            self.reply_pending_recv(pending, reply);
        }

        if enqueue_conn_ready {
            self.enqueue_latency_task(ReadyOp::Conn(session_id));
        }

        if let Some(slot_id) = selected_slot {
            let reprovision_slot = if let Some(pool) = self.provided_recv_pool.as_mut() {
                if !pool.complete_recv(slot_id) {
                    self.close_connection(session_id, NifError::from_errno(libc::EIO));
                    return;
                }
                pool.slot_id_for_bid(slot_id.index)
            } else {
                None
            };
            if let Some(reprovision_slot) = reprovision_slot {
                let _ = self.reprovide_recv_slot(reprovision_slot);
            }
        }

        if had_pending_batch {
            self.resume_pending_batch(session_id);
        }

        if rearm_immediately {
            self.set_recv_mailbox_snapshot(session_id, mailbox_passive_on_submit);
            if self.arm_conn_recv(session_id).is_err() {
                self.clear_recv_mailbox_snapshot(session_id);
            }
        }
        let _ = cqe_ref;
    }

    pub(super) fn handle_provided_recv_pool_remove_cqe(&mut self, result: i32) {
        let Some(pool) = self.provided_recv_pool.as_mut() else {
            return;
        };
        if result >= 0 {
            // This only records that the kernel-side remove request completed; it
            // does not imply the whole pool teardown is finished yet.
            debug_assert!(!pool.is_teardown_complete());
            pool.mark_remove_completed();
        } else {
            let errno = -result;
            if errno == libc::ENOENT || errno == libc::ECANCELED {
                // Terminal remove CQEs still only complete this local bookkeeping.
                debug_assert!(!pool.is_teardown_complete());
                pool.mark_remove_completed();
            } else {
                // Retry state can be cleared here, but the pool may still need later
                // lifecycle steps before teardown is complete.
                pool.clear_remove_queued();
                return;
            }
        }
        self.maybe_release_provided_recv_pool();
    }

    pub(super) fn handle_close_fd_cqe(&mut self, _fd: i32, result: i32) {
        if stop_drain_profile_enabled() {
            stats_inc!(self.stats, close_fd_cqes);
        }
        // Close CQEs are terminal bookkeeping only; by the time they arrive, the
        // connection has already been detached and its buffers returned.
        if result < 0 {
            let _ = result;
        }
    }

    pub(super) fn handle_write_cqe(&mut self, session_id: u64, result: i32, cqe_ref: u64) {
        let Some(conn) = self.conns.get_mut(&session_id) else {
            return;
        };
        stats_inc!(self.stats, write_cqes);
        let stop_close_pending = conn.stop_close_on_write_cqe;
        if stop_close_pending {
            conn.stop_close_on_write_cqe = false;
        }
        conn.write_in_flight = false;
        conn.write_iovecs.clear();

        if result == 0 {
            // A completed write can immediately close the connection because the
            // detach/cleanup path runs from `close_connection`.
            self.close_connection(session_id, NifError::Closed);
            return;
        }
        if result < 0 {
            let errno = -result;
            if stop_close_pending {
                self.close_connection(session_id, NifError::Closed);
                return;
            }
            if errno == libc::EINTR {
                self.enqueue_latency_task(ReadyOp::Conn(session_id));
                return;
            }
            if errno == libc::EAGAIN || errno == libc::EWOULDBLOCK {
                let _ = self.arm_conn_write_poll(session_id);
                return;
            }
            if errno == libc::ECANCELED || errno == libc::EBADF {
                return;
            }
            self.close_connection(session_id, NifError::from_errno(errno));
            return;
        }

        let mut written = result as usize;
        if let Some(conn) = self.conns.get_mut(&session_id) {
            while written > 0 {
                let Some(front) = conn.pending_writes.front_mut() else {
                    break;
                };
                let remaining = match front {
                    PendingWrite::Arena(chunk) => chunk.len,
                    PendingWrite::Heap { data, off } => data.len() - *off,
                };
                if written >= remaining {
                    written -= remaining;
                    conn.tx_bytes = conn.tx_bytes.saturating_sub(remaining);
                    if let Some(PendingWrite::Arena(chunk)) = conn.pending_writes.pop_front() {
                        self.arenas.release(chunk);
                    }
                } else {
                    match front {
                        PendingWrite::Arena(chunk) => {
                            chunk.off += written;
                            chunk.len -= written;
                        }
                        PendingWrite::Heap { off, .. } => {
                            *off += written;
                        }
                    }
                    written = 0;
                }
            }
        }

        if stop_close_pending {
            self.close_connection(session_id, NifError::Closed);
            return;
        }

        self.enqueue_latency_task(ReadyOp::Conn(session_id));
        let _ = cqe_ref;
    }

    pub(super) fn handle_subscribe_read_cqe(
        &mut self,
        subscription_id: u64,
        slot_idx: usize,
        result: i32,
    ) {
        let (target, page_slot, stopped, in_flight_now) = {
            let Some(sub) = self.subscriptions.get_mut(&subscription_id) else {
                return;
            };
            if slot_idx >= sub.slot_in_flight.len() {
                return;
            }
            if sub.slot_in_flight[slot_idx] {
                sub.slot_in_flight[slot_idx] = false;
                sub.in_flight = sub.in_flight.saturating_sub(1);
            }
            (
                sub.target.clone(),
                sub.page_slots[slot_idx],
                sub.stopped,
                sub.in_flight,
            )
        };

        if stopped {
            if in_flight_now == 0 {
                self.drop_subscription(subscription_id);
            }
            return;
        }

        if result == 0 {
            self.queue_reply_error(target, subscription_id, NifError::Closed);
            self.request_stop_subscription(subscription_id);
            return;
        }
        if result < 0 {
            let errno = -result;
            if errno == libc::EINTR || errno == libc::EAGAIN || errno == libc::EWOULDBLOCK {
                let _ = self.fill_subscription_inflight(subscription_id);
                return;
            }
            if errno != libc::ECANCELED && errno != libc::EBADF {
                self.queue_reply_error(target, subscription_id, NifError::from_errno(errno));
            }
            self.request_stop_subscription(subscription_id);
            return;
        }

        let n = result as usize;
        let Some(page_slot) = page_slot else {
            self.queue_reply_error(target, subscription_id, NifError::from_errno(libc::EIO));
            self.request_stop_subscription(subscription_id);
            return;
        };
        self.queue_reply_data_subscribe_slot(target, subscription_id, page_slot, n);
        let _ = self.fill_subscription_inflight(subscription_id);
    }

    pub(super) fn handle_subscribe_accept_cqe(
        &mut self,
        subscription_id: u64,
        slot_idx: usize,
        result: i32,
        armed_ns: u64,
    ) {
        let mut accepted_fd = if result >= 0 {
            // SAFETY: `result` is a successful accept CQE fd that transfers ownership
            // into this function; it is consumed once by install or dropped on error.
            Some(unsafe { OwnedFd::from_raw_fd(result) })
        } else {
            None
        };
        if result >= 0 {
            self.stats
                .record_subscribe_accept_wait(monotonic_ns().saturating_sub(armed_ns));
        }
        let (target, stopped, in_flight_now) = {
            let Some(sub) = self.subscriptions.get_mut(&subscription_id) else {
                return;
            };
            if slot_idx >= sub.slot_in_flight.len() {
                return;
            }
            if sub.slot_in_flight[slot_idx] {
                sub.slot_in_flight[slot_idx] = false;
                sub.in_flight = sub.in_flight.saturating_sub(1);
            }
            let target = if matches!(sub.operation, SubscribeOperation::Accept)
                && !sub.accept_consumers.is_empty()
            {
                let idx = sub.accept_rr_next % sub.accept_consumers.len();
                let out = sub.accept_consumers[idx].clone();
                sub.accept_rr_next = (idx + 1) % sub.accept_consumers.len();
                out
            } else {
                sub.target.clone()
            };
            (target, sub.stopped, sub.in_flight)
        };

        if stopped {
            if in_flight_now == 0 {
                self.drop_subscription(subscription_id);
            }
            return;
        }

        if result < 0 {
            let errno = -result;
            if errno == libc::EINTR || errno == libc::EAGAIN || errno == libc::EWOULDBLOCK {
                let _ = self.fill_subscription_inflight(subscription_id);
                return;
            }
            if errno != libc::ECANCELED && errno != libc::EBADF {
                self.queue_reply_error(target, subscription_id, NifError::from_errno(errno));
            }
            self.request_stop_subscription(subscription_id);
            return;
        }

        let owner = target_owner_pid(&target);
        let Some(fd) = accepted_fd.take() else {
            return;
        };
        let session = self.install_connection_handle(owner, fd);
        let Some(session) = session else {
            self.queue_reply_error(target, subscription_id, NifError::from_errno(libc::ENOSPC));
            let _ = self.fill_subscription_inflight(subscription_id);
            return;
        };
        self.queue_reply_session(target, subscription_id, session);
        let _ = self.fill_subscription_inflight(subscription_id);
    }

    pub(super) fn handle_statx_cqe(&mut self, request_id: u64, result: i32) {
        let Some(pending) = self.pending_statx.remove(&request_id) else {
            return;
        };
        let crate::runtime::PendingStatx {
            fd,
            request_id,
            target,
            out,
        } = pending;
        let statx_size = std::mem::size_of::<libc::statx>();

        if result == 0 {
            let size = {
                match &out {
                    StatxOut::Arena(handle) => {
                        let bytes = self.arenas.slice(handle.class, handle.slot, 0, statx_size);
                        // SAFETY: `arm_statx` allocated and zero-initialized this output region
                        // as a `libc::statx`-sized/aligned buffer before kernel submission, and
                        // this branch only reads it on a successful statx completion (`result=0`).
                        let stx = unsafe { &*(bytes.as_ptr() as *const libc::statx) };
                        stx.stx_size as u64
                    }
                    StatxOut::Heap(stx) => stx.stx_size,
                }
            };
            if let StatxOut::Arena(handle) = out {
                self.arenas.release(handle);
            }
            self.queue_reply_u64(target, request_id, size);
            return;
        }

        let errno = -result;
        if errno == libc::ENOSYS || errno == libc::EOPNOTSUPP || errno == libc::EINVAL {
            if let StatxOut::Arena(handle) = out {
                self.arenas.release(handle);
            }
            match stat_size_via_fstat(fd) {
                Ok(size) => self.queue_reply_u64(target, request_id, size),
                Err(err) => self.queue_reply_error(target, request_id, err),
            }
            return;
        }

        if let StatxOut::Arena(handle) = out {
            self.arenas.release(handle);
        }
        self.queue_reply_error(target, request_id, NifError::from_errno(errno));
    }
}
