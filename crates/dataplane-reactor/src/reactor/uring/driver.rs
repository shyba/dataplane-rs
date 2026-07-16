use crate::reactor::{
    DriverBackendKind, DriverCapabilities, EventContext, Handler, NetEvent, NetOp, NetOpKind,
    NetSubscription, NetSubscriptionEvent, OpToken, Reactor, ReactorDriver, ReactorDriverWait,
    SubscriptionToken,
};

use super::entries::{cqe_more, send_entry};
use super::reactor::UringReactor;
use super::subscriptions::SubscriptionState;
use super::tokens::{OpState, TokenKind};
use super::udp_batch::{fill_udp_batch_slot_from_multishot, UdpBatchMode};

impl Reactor for UringReactor {
    type Error = std::io::Error;

    fn submit(&mut self, op: NetOp) -> Result<OpToken, Self::Error> {
        let raw = self.alloc_raw_token();
        let token = OpToken(raw);
        self.submit_with_token(op, token)?;
        Ok(token)
    }

    fn subscribe<H>(
        &mut self,
        op: NetSubscription,
        handler: H,
    ) -> Result<SubscriptionToken, Self::Error>
    where
        H: Handler<NetSubscriptionEvent> + 'static,
    {
        let token = SubscriptionToken(self.alloc_raw_token());
        self.subscriptions.insert(
            token,
            SubscriptionState {
                kind: op,
                handler: Box::new(handler),
            },
        );
        self.rearm_subscription(token)?;
        Ok(token)
    }

    fn cancel(&mut self, token: SubscriptionToken) -> Result<(), Self::Error> {
        let removed = self.subscriptions.remove(&token);
        // Exhaustive on purpose: CancelledSubscription's completion handling
        // assumes accept semantics (results are fds). A new subscription kind
        // must get its own cancelled-token kind.
        if let Some(state) = &removed {
            match state.kind {
                NetSubscription::AcceptMulti { .. } => {}
            }
        }
        if let Some(meta) = self.token_meta.get_mut(&token.0) {
            // The kernel-side multishot op is still armed: ask the kernel to
            // cancel it and keep the token mapped so late completions (which
            // may carry accepted fds) are closed instead of leaked. The
            // mapping is removed on the final no-MORE completion.
            meta.kind = TokenKind::CancelledSubscription;
            let entry = io_uring::opcode::AsyncCancel::new(token.0).build().user_data(0);
            let _ = self.queue_entry(entry);
            let _ = self.ring.submit();
        }
        Ok(())
    }

    fn submit_pending(&mut self) -> Result<usize, Self::Error> {
        self.ring.submit()
    }

    fn poll(&mut self, wait: bool) -> Result<Vec<NetEvent>, Self::Error> {
        if !self.ready_events.is_empty() {
            return Ok(self.ready_events.drain(..).collect());
        }
        if wait {
            self.ring.submit_and_wait(1)?;
        }
        let mut completions = Vec::new();
        {
            let cq = self.ring.completion();
            for cqe in cq {
                completions.push((cqe.user_data(), cqe.result(), cqe.flags()));
            }
        }

        let mut out = Vec::with_capacity(completions.len());
        let mut queued_followups = 0usize;
        for (raw, result, flags) in completions {
            let Some(kind) = self.token_meta.get(&raw).map(|m| m.kind) else {
                continue;
            };
            match kind {
                TokenKind::RetiredBuffers => {
                    if result >= 0 {
                        // Kernel confirmed RemoveBuffers: the provided
                        // backing store can be freed now.
                        self.token_meta.remove(&raw);
                    }
                    // On failure the entry (and the buffer it keeps alive)
                    // is retained deliberately: a bounded leak is safe,
                    // freeing memory the kernel may still reference is not.
                }
                TokenKind::CancelledSubscription => {
                    // INVARIANT: this close is only valid because AcceptMulti
                    // is the sole subscription kind (its results are fds).
                    // cancel() matches exhaustively on NetSubscription; adding
                    // a non-accept subscription kind must extend this dispatch
                    // rather than reuse it.
                    if result >= 0 {
                        // Late accept completion after cancel: close the fd
                        // instead of leaking it.
                        // SAFETY: result >= 0 from an accept CQE is a freshly
                        // accepted fd owned by no one else; closing it here is
                        // the only cleanup path.
                        let _ = unsafe { libc::close(result) };
                    }
                    if !cqe_more(flags) {
                        self.token_meta.remove(&raw);
                    }
                }
                TokenKind::Op(op_state) => match op_state {
                    OpState::Simple(kind) => {
                        self.token_meta.remove(&raw);
                        out.push(NetEvent::OpComplete {
                            token: OpToken(raw),
                            kind,
                            result,
                            flags,
                        });
                    }
                    OpState::SendAll { fd, ptr, len, sent } => {
                        if result < 0 {
                            self.token_meta.remove(&raw);
                            out.push(NetEvent::OpComplete {
                                token: OpToken(raw),
                                kind: NetOpKind::SendAll,
                                result,
                                flags,
                            });
                            continue;
                        }
                        let n = result as usize;
                        let new_sent = sent.saturating_add(n).min(len);
                        if new_sent >= len {
                            self.token_meta.remove(&raw);
                            out.push(NetEvent::OpComplete {
                                token: OpToken(raw),
                                kind: NetOpKind::SendAll,
                                result: len as i32,
                                flags,
                            });
                        } else {
                            if let Some(meta) = self.token_meta.get_mut(&raw) {
                                meta.kind = TokenKind::Op(OpState::SendAll {
                                    fd,
                                    ptr,
                                    len,
                                    sent: new_sent,
                                });
                            }
                            // SAFETY: ptr is valid for at least len bytes; new_sent < len
                            // (since the previous send returned result < len). next_ptr is
                            // within the original buffer at offset new_sent.
                            let next_ptr = unsafe { ptr.add(new_sent) };
                            let entry = send_entry(fd, next_ptr, len - new_sent, raw);
                            self.queue_entry(entry)?;
                            queued_followups += 1;
                        }
                    }
                    OpState::UdpRecvBatch => {
                        if result < 0 {
                            let errno = -result;
                            if errno == libc::EINTR
                                || errno == libc::EAGAIN
                                || errno == libc::EWOULDBLOCK
                                || errno == libc::ENOBUFS
                            {
                                let filled = self
                                    .token_meta
                                    .get(&raw)
                                    .and_then(|m| m.udp_batch.as_ref())
                                    .map(|s| s.filled)
                                    .unwrap_or(0);
                                if errno == libc::ENOBUFS {
                                    if let Some(state) = self
                                        .token_meta
                                        .get_mut(&raw)
                                        .and_then(|m| m.udp_batch.as_mut())
                                    {
                                        state.replenish_buffers = true;
                                    }
                                }
                                if filled > 0 {
                                    if let Some(meta) = self.token_meta.remove(&raw) {
                                        if let Some(state) = meta.udp_batch {
                                            let _ = self.cleanup_udp_batch(state);
                                        }
                                    }
                                    out.push(NetEvent::OpComplete {
                                        token: OpToken(raw),
                                        kind: NetOpKind::UdpRecvBatch,
                                        result: filled as i32,
                                        flags,
                                    });
                                } else {
                                    self.queue_next_udp_batch_recv(raw)?;
                                    queued_followups += 1;
                                }
                                continue;
                            }
                            if let Some(meta) = self.token_meta.remove(&raw) {
                                if let Some(state) = meta.udp_batch {
                                    let _ = self.cleanup_udp_batch(state);
                                }
                            }
                            out.push(NetEvent::OpComplete {
                                token: OpToken(raw),
                                kind: NetOpKind::UdpRecvBatch,
                                result,
                                flags,
                            });
                            continue;
                        }

                        let mut queue_next = false;
                        let mut complete_result = None::<i32>;
                        if let Some(meta) = self.token_meta.get_mut(&raw) {
                            if let Some(state) = meta.udp_batch.as_mut() {
                                match &state.mode {
                                    UdpBatchMode::Single { msgs, .. } => {
                                        let idx = state.next_slot;
                                        if state.slots_ptr.is_null() || idx >= state.slots_len {
                                            complete_result = Some(-libc::EIO);
                                        } else {
                                            // SAFETY: slots_ptr is non-null and slots_len > idx (enforced above).
                                            let slots = unsafe {
                                                std::slice::from_raw_parts_mut(
                                                    state.slots_ptr,
                                                    state.slots_len,
                                                )
                                            };
                                            slots[idx].recv_len = result as usize;
                                            slots[idx].addr_len = msgs[idx].msg_namelen;
                                            state.filled += 1;
                                            state.next_slot += 1;
                                            if state.next_slot < state.slots_len {
                                                queue_next = true;
                                            } else {
                                                complete_result = Some(state.filled as i32);
                                            }
                                        }
                                    }
                                    UdpBatchMode::Multi { .. } => {
                                        if fill_udp_batch_slot_from_multishot(state, result, flags)
                                            .is_err()
                                        {
                                            complete_result = Some(-libc::EIO);
                                        } else if state.next_slot < state.slots_len {
                                            queue_next = !cqe_more(flags);
                                        } else {
                                            complete_result = Some(state.filled as i32);
                                        }
                                    }
                                }
                            } else {
                                complete_result = Some(-libc::EIO);
                            }
                        }

                        if queue_next {
                            self.queue_next_udp_batch_recv(raw)?;
                            queued_followups += 1;
                        } else if let Some(done) = complete_result {
                            if let Some(meta) = self.token_meta.remove(&raw) {
                                if let Some(state) = meta.udp_batch {
                                    let _ = self.cleanup_udp_batch(state);
                                }
                            }
                            out.push(NetEvent::OpComplete {
                                token: OpToken(raw),
                                kind: NetOpKind::UdpRecvBatch,
                                result: done,
                                flags,
                            });
                        }
                    }
                },
                TokenKind::Subscription(sub_token) => {
                    let mut remove_sub = false;
                    let mut should_rearm = false;
                    if let Some(state) = self.subscriptions.get_mut(&sub_token) {
                        let mut cx = EventContext::default();
                        if result >= 0 {
                            match state.kind {
                                NetSubscription::AcceptMulti { .. } => state.handler.on_event(
                                    &mut cx,
                                    NetSubscriptionEvent::Accepted { fd: result },
                                ),
                            }
                        } else {
                            state
                                .handler
                                .on_error(&mut cx, std::io::Error::from_raw_os_error(-result));
                        }
                        for action in cx.actions {
                            if let crate::reactor::FollowupAction::Cancel(tok) = action {
                                if tok == sub_token {
                                    remove_sub = true;
                                }
                            }
                        }
                    }
                    if remove_sub {
                        self.subscriptions.remove(&sub_token);
                        self.token_meta.remove(&raw);
                        continue;
                    }
                    if !cqe_more(flags) {
                        self.token_meta.remove(&raw);
                        should_rearm = self.subscriptions.contains_key(&sub_token);
                    }
                    if should_rearm {
                        self.rearm_subscription(sub_token)?;
                    }
                }
            }
        }
        if queued_followups > 0 {
            let _ = self.ring.submit()?;
        }
        Ok(out)
    }
}

impl ReactorDriver for UringReactor {
    type Error = std::io::Error;
    type Token = OpToken;
    type Submit = NetOp;
    type Event = NetEvent;

    fn submit(&mut self, op: Self::Submit, token: Self::Token) -> Result<(), Self::Error> {
        self.submit_with_token(op, token)
    }

    fn flush(&mut self) -> Result<usize, Self::Error> {
        self.submit_pending()
    }

    fn drain<F>(&mut self, max_events: usize, mut on_event: F) -> Result<usize, Self::Error>
    where
        F: FnMut(Self::Event),
    {
        let mut drained = self.drain_ready_events(max_events, &mut on_event);
        let limit = max_events.max(1);
        if drained >= limit {
            return Ok(drained);
        }

        let events = self.poll(false)?;
        let remaining = limit - drained;
        for (idx, event) in events.into_iter().enumerate() {
            if idx < remaining {
                on_event(event);
                drained += 1;
            } else {
                self.ready_events.push_back(event);
            }
        }
        Ok(drained)
    }

    fn outstanding(&self) -> usize {
        self.token_meta
            .values()
            .filter(|meta| matches!(meta.kind, TokenKind::Op(_)))
            .count()
    }

    fn capabilities(&self) -> DriverCapabilities {
        DriverCapabilities {
            backend: DriverBackendKind::IoUring,
            supports_accept_multi: true,
            supports_multishot: self.supports_recvmsg_multishot(),
            supports_fixed_buffers: self.supports_fixed_buffers(),
            // The ring is not built with SQPOLL; do not advertise it.
            supports_sqpoll: false,
        }
    }
}

impl ReactorDriverWait for UringReactor {
    type Error = std::io::Error;
    type Readiness = ();

    fn readiness(&self) -> Option<Self::Readiness> {
        None
    }

    fn wait(&mut self, _min_events: usize) -> Result<usize, Self::Error> {
        if !self.ready_events.is_empty() {
            return Ok(self.ready_events.len());
        }

        let events = self.poll(true)?;
        let count = events.len();
        self.ready_events.extend(events);
        Ok(count)
    }

    fn wait_deadline(
        &mut self,
        min_events: usize,
        timeout_ns: Option<u64>,
    ) -> Result<usize, Self::Error> {
        let Some(timeout_ns) = timeout_ns else {
            return self.wait(min_events);
        };
        if !self.ready_events.is_empty() {
            return Ok(self.ready_events.len());
        }

        let timespec = io_uring::types::Timespec::new()
            .sec(timeout_ns / 1_000_000_000)
            .nsec((timeout_ns % 1_000_000_000) as u32);
        let args = io_uring::types::SubmitArgs::new().timespec(&timespec);
        match self
            .ring
            .submitter()
            .submit_with_args(min_events.max(1), &args)
        {
            Ok(_) => {}
            Err(err)
                if err.raw_os_error() == Some(libc::ETIME)
                    || err.raw_os_error() == Some(libc::EINTR) => {}
            Err(err) => return Err(err),
        }
        let events = self.poll(false)?;
        let count = events.len();
        self.ready_events.extend(events);
        Ok(count)
    }
}
