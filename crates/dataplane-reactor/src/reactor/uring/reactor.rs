use io_uring::{opcode, squeue, types, Probe};
use std::collections::{HashMap, VecDeque};
use std::os::fd::RawFd;

use crate::reactor::{
    NetEvent, NetOp, NetOpKind, NetSubscription, OpToken, SubscriptionToken, UdpRecvSlot,
};

use super::entries::{accept_multi_entry, connect_entry, recv_entry, recvmsg_entry, send_entry};
use super::subscriptions::SubscriptionState;
use super::tokens::{OpState, TokenKind, TokenMeta};
use super::udp_batch::{UdpBatchMode, UdpBatchState};

pub struct UringReactor {
    pub(super) ring: io_uring::IoUring,
    next_token: u64,
    next_buf_group: u16,
    recvmsg_multi_supported: bool,
    fixed_buffers_supported: bool,
    pub(super) ready_events: VecDeque<NetEvent>,
    pub(super) token_meta: HashMap<u64, TokenMeta>,
    pub(super) subscriptions: HashMap<SubscriptionToken, SubscriptionState>,
}

impl UringReactor {
    pub fn new(entries: u32) -> std::io::Result<Self> {
        let ring = io_uring::IoUring::new(entries)?;
        let mut recvmsg_multi_supported = false;
        let mut fixed_buffers_supported = false;
        let mut probe = Probe::new();
        if ring.submitter().register_probe(&mut probe).is_ok() {
            recvmsg_multi_supported = probe.is_supported(opcode::RecvMsgMulti::CODE);
            fixed_buffers_supported = probe.is_supported(opcode::ReadFixed::CODE);
        }
        Ok(Self {
            ring,
            next_token: 1,
            next_buf_group: 1,
            recvmsg_multi_supported,
            fixed_buffers_supported,
            ready_events: VecDeque::new(),
            token_meta: HashMap::new(),
            subscriptions: HashMap::new(),
        })
    }

    pub fn supports_recvmsg_multishot(&self) -> bool {
        self.recvmsg_multi_supported
    }

    pub fn supports_fixed_buffers(&self) -> bool {
        self.fixed_buffers_supported
    }

    /// High bit namespaces internally-minted tokens away from caller-supplied
    /// `OpToken`s (InflightTable tokens are `generation << 32 | index` and
    /// only reach the high bit after 2^31 slot generations).
    pub(super) const INTERNAL_TOKEN_BIT: u64 = 1 << 63;

    pub(super) fn alloc_raw_token(&mut self) -> u64 {
        let out = self.next_token;
        self.next_token = self.next_token.wrapping_add(1).max(1);
        out | Self::INTERNAL_TOKEN_BIT
    }

    pub(super) fn queue_entry(&mut self, entry: squeue::Entry) -> std::io::Result<()> {
        // Bounded: a full SQ that submit() cannot drain (CQ overflow
        // backpressure) must surface as an error, not a busy-loop.
        let mut stalled_submits = 0usize;
        loop {
            // SAFETY: io_uring's SubmissionQueue::push is safe when entry is a valid
            // squeue::Entry. The push either succeeds (entry is copied into the SQ ring)
            // or returns Err — either way entry remains valid. We submit after push.
            let pushed = unsafe { self.ring.submission().push(&entry).is_ok() };
            if pushed {
                return Ok(());
            }
            let submitted = self.ring.submit()?;
            if submitted == 0 {
                stalled_submits += 1;
                if stalled_submits >= 64 {
                    return Err(std::io::Error::new(
                        std::io::ErrorKind::WouldBlock,
                        "submission queue full and submit() makes no progress",
                    ));
                }
            } else {
                stalled_submits = 0;
            }
        }
    }

    pub(super) fn rearm_subscription(&mut self, token: SubscriptionToken) -> std::io::Result<()> {
        let Some(state) = self.subscriptions.get(&token) else {
            return Ok(());
        };
        let raw = token.0;
        let entry = match state.kind {
            NetSubscription::AcceptMulti { listener_fd, flags } => {
                accept_multi_entry(listener_fd, flags, raw)
            }
        };
        self.queue_entry(entry)?;
        self.token_meta.insert(
            raw,
            TokenMeta {
                kind: TokenKind::Subscription(token),
                udp_batch: None,
                _connect_addr: None,
            },
        );
        Ok(())
    }

    fn alloc_buf_group(&mut self) -> u16 {
        let out = self.next_buf_group.max(1);
        self.next_buf_group = self.next_buf_group.wrapping_add(1).max(1);
        out
    }

    fn init_udp_batch_state(
        &mut self,
        fd: RawFd,
        slots_ptr: *mut UdpRecvSlot,
        slots_len: usize,
        flags: i32,
        prefer_multishot: bool,
    ) -> std::io::Result<UdpBatchState> {
        if slots_len == 0 || slots_ptr.is_null() {
            return Err(std::io::Error::from_raw_os_error(libc::EINVAL));
        }
        // SAFETY: slots_ptr is non-null and slots_len > 0. The caller owns the memory
        // and guarantees it is valid for slots_len UdpRecvSlot elements.
        let slots = unsafe { std::slice::from_raw_parts_mut(slots_ptr, slots_len) };
        for slot in slots.iter_mut().take(slots_len) {
            slot.recv_len = 0;
            slot.addr_len = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
        }

        let mode = if prefer_multishot && self.recvmsg_multi_supported {
            let nbufs = slots_len.min(u16::MAX as usize) as u16;
            let max_payload = slots.iter().map(|s| s.buf_len.max(1)).max().unwrap_or(1);
            let name_len = std::mem::size_of::<libc::sockaddr_storage>();
            let recvmsg_out_len = 64usize;
            let buf_len = (recvmsg_out_len + name_len + max_payload)
                .next_power_of_two()
                .max(256);
            let provided = vec![0u8; buf_len * (nbufs as usize)].into_boxed_slice();
            let msg_template = libc::msghdr {
                msg_name: std::ptr::null_mut(),
                msg_namelen: name_len as libc::socklen_t,
                msg_iov: std::ptr::null_mut(),
                msg_iovlen: 0,
                msg_control: std::ptr::null_mut(),
                msg_controllen: 0,
                msg_flags: 0,
            };
            UdpBatchMode::Multi {
                provided,
                buf_len,
                bgid: self.alloc_buf_group(),
                nbufs,
                msg_template,
            }
        } else {
            let mut iovecs = vec![
                libc::iovec {
                    iov_base: std::ptr::null_mut(),
                    iov_len: 0,
                };
                slots_len
            ];
            let mut msgs = vec![
                libc::msghdr {
                    msg_name: std::ptr::null_mut(),
                    msg_namelen: 0,
                    msg_iov: std::ptr::null_mut(),
                    msg_iovlen: 0,
                    msg_control: std::ptr::null_mut(),
                    msg_controllen: 0,
                    msg_flags: 0,
                };
                slots_len
            ];
            for idx in 0..slots_len {
                iovecs[idx].iov_base = slots[idx].buf_ptr.cast();
                iovecs[idx].iov_len = slots[idx].buf_len;
                msgs[idx].msg_name = (&mut slots[idx].addr as *mut libc::sockaddr_storage).cast();
                msgs[idx].msg_namelen = slots[idx].addr_len;
                msgs[idx].msg_iov = &mut iovecs[idx] as *mut libc::iovec;
                msgs[idx].msg_iovlen = 1;
                msgs[idx].msg_control = std::ptr::null_mut();
                msgs[idx].msg_controllen = 0;
                msgs[idx].msg_flags = 0;
            }
            UdpBatchMode::Single {
                iovecs: iovecs.into_boxed_slice(),
                msgs: msgs.into_boxed_slice(),
            }
        };

        Ok(UdpBatchState {
            fd,
            slots_ptr,
            slots_len,
            flags,
            next_slot: 0,
            filled: 0,
            replenish_buffers: false,
            mode,
        })
    }

    pub(super) fn queue_next_udp_batch_recv(&mut self, raw: u64) -> std::io::Result<()> {
        let mut entries: Vec<squeue::Entry> = Vec::with_capacity(2);
        {
            let Some(meta) = self.token_meta.get_mut(&raw) else {
                return Ok(());
            };
            let Some(state) = meta.udp_batch.as_mut() else {
                return Ok(());
            };
            if state.next_slot >= state.slots_len {
                return Ok(());
            }
            if state.slots_ptr.is_null() {
                return Err(std::io::Error::from_raw_os_error(libc::EINVAL));
            }
            match &mut state.mode {
                UdpBatchMode::Single { iovecs, msgs } => {
                    let idx = state.next_slot;
                    // SAFETY: state.slots_ptr is non-null and state.slots_len > 0
                    // (enforced by init and the null/is_null checks above).
                    let slots =
                        unsafe { std::slice::from_raw_parts_mut(state.slots_ptr, state.slots_len) };
                    slots[idx].recv_len = 0;
                    slots[idx].addr_len =
                        std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
                    iovecs[idx].iov_base = slots[idx].buf_ptr.cast();
                    iovecs[idx].iov_len = slots[idx].buf_len;
                    msgs[idx].msg_name =
                        (&mut slots[idx].addr as *mut libc::sockaddr_storage).cast();
                    msgs[idx].msg_namelen = slots[idx].addr_len;
                    msgs[idx].msg_iov = &mut iovecs[idx] as *mut libc::iovec;
                    msgs[idx].msg_iovlen = 1;
                    msgs[idx].msg_control = std::ptr::null_mut();
                    msgs[idx].msg_controllen = 0;
                    msgs[idx].msg_flags = 0;
                    entries.push(recvmsg_entry(
                        state.fd,
                        (&msgs[idx] as *const libc::msghdr).cast_mut(),
                        state.flags,
                        raw,
                    ));
                }
                UdpBatchMode::Multi {
                    provided,
                    buf_len,
                    bgid,
                    nbufs,
                    msg_template,
                } => {
                    if state.next_slot == 0 || state.replenish_buffers {
                        state.replenish_buffers = false;
                        entries.push(
                            opcode::ProvideBuffers::new(
                                provided.as_mut_ptr(),
                                *buf_len as i32,
                                *nbufs,
                                *bgid,
                                0,
                            )
                            .build()
                            .user_data(0),
                        );
                    }
                    entries.push(
                        opcode::RecvMsgMulti::new(
                            types::Fd(state.fd),
                            (msg_template as *const libc::msghdr).cast(),
                            *bgid,
                        )
                        .flags((state.flags | libc::MSG_DONTWAIT) as u32)
                        .build()
                        .user_data(raw),
                    );
                }
            }
        }
        for entry in entries {
            self.queue_entry(entry)?;
        }
        Ok(())
    }

    pub(super) fn cleanup_udp_batch(&mut self, mut state: UdpBatchState) -> std::io::Result<()> {
        if let UdpBatchMode::Multi { bgid, nbufs, .. } = &mut state.mode {
            // The kernel buffer ring still references `state.mode.provided`;
            // park the state under a retire token until the RemoveBuffers
            // completion confirms the kernel no longer owns the memory.
            let retire_raw = self.alloc_raw_token();
            let remove = opcode::RemoveBuffers::new(*nbufs, *bgid)
                .build()
                .user_data(retire_raw);
            self.token_meta.insert(
                retire_raw,
                TokenMeta {
                    kind: TokenKind::RetiredBuffers,
                    udp_batch: Some(state),
                    _connect_addr: None,
                },
            );
            let _ = self.queue_entry(remove);
        }
        Ok(())
    }

    pub(super) fn submit_with_token(
        &mut self,
        op: NetOp,
        token: OpToken,
    ) -> Result<(), std::io::Error> {
        if token.0 == 0 {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "reactor driver token must be non-zero",
            ));
        }
        let raw = token.0;
        if self.token_meta.contains_key(&raw) {
            return Err(std::io::Error::new(
                std::io::ErrorKind::InvalidInput,
                "reactor driver token already in flight",
            ));
        }
        let (entry, meta) = match op {
            NetOp::Connect { fd, addr } => {
                let meta = TokenMeta {
                    kind: TokenKind::Op(OpState::Simple(NetOpKind::Connect)),
                    udp_batch: None,
                    _connect_addr: Some(Box::new(addr)),
                };
                let addr_ref = meta._connect_addr.as_ref().expect("connect addr in meta");
                let entry = connect_entry(fd, addr_ref.as_ref(), raw);
                (entry, meta)
            }
            NetOp::Send { fd, ptr, len } => (
                send_entry(fd, ptr, len, raw),
                TokenMeta {
                    kind: TokenKind::Op(OpState::Simple(NetOpKind::Send)),
                    udp_batch: None,
                    _connect_addr: None,
                },
            ),
            NetOp::SendAll { fd, ptr, len } => (
                send_entry(fd, ptr, len, raw),
                TokenMeta {
                    kind: TokenKind::Op(OpState::SendAll {
                        fd,
                        ptr,
                        len,
                        sent: 0,
                    }),
                    udp_batch: None,
                    _connect_addr: None,
                },
            ),
            NetOp::Recv { fd, ptr, len } => (
                recv_entry(fd, ptr, len, raw),
                TokenMeta {
                    kind: TokenKind::Op(OpState::Simple(NetOpKind::Recv)),
                    udp_batch: None,
                    _connect_addr: None,
                },
            ),
            NetOp::UdpSend { fd, ptr, len } => (
                send_entry(fd, ptr, len, raw),
                TokenMeta {
                    kind: TokenKind::Op(OpState::Simple(NetOpKind::UdpSend)),
                    udp_batch: None,
                    _connect_addr: None,
                },
            ),
            NetOp::UdpRecv { fd, ptr, len } => (
                recv_entry(fd, ptr, len, raw),
                TokenMeta {
                    kind: TokenKind::Op(OpState::Simple(NetOpKind::UdpRecv)),
                    udp_batch: None,
                    _connect_addr: None,
                },
            ),
            NetOp::UdpRecvBatch {
                fd,
                slots_ptr,
                slots_len,
                flags,
                prefer_multishot,
            } => {
                let state =
                    self.init_udp_batch_state(fd, slots_ptr, slots_len, flags, prefer_multishot)?;
                let meta = TokenMeta {
                    kind: TokenKind::Op(OpState::UdpRecvBatch),
                    udp_batch: Some(state),
                    _connect_addr: None,
                };
                self.token_meta.insert(raw, meta);
                self.queue_next_udp_batch_recv(raw)?;
                return Ok(());
            }
        };
        self.queue_entry(entry)?;
        self.token_meta.insert(raw, meta);
        Ok(())
    }

    pub(super) fn drain_ready_events<F>(&mut self, max_events: usize, mut on_event: F) -> usize
    where
        F: FnMut(NetEvent),
    {
        let mut drained = 0usize;
        let limit = max_events;
        while drained < limit {
            let Some(event) = self.ready_events.pop_front() else {
                break;
            };
            on_event(event);
            drained += 1;
        }
        drained
    }
}
