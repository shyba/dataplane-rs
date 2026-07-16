use std::collections::{HashMap, VecDeque};
use std::io;
use std::os::fd::RawFd;
use std::time::Duration;

use super::{
    DriverBackendKind, DriverCapabilities, EventContext, Handler, NetEvent, NetOp, NetOpKind,
    NetSubscription, NetSubscriptionEvent, OpToken, Reactor, ReactorDriver, ReactorDriverWait,
    SubscriptionToken,
};

fn last_errno() -> i32 {
    io::Error::last_os_error()
        .raw_os_error()
        .unwrap_or(libc::EIO)
}

fn is_would_block(errno: i32) -> bool {
    errno == libc::EAGAIN || errno == libc::EWOULDBLOCK || errno == libc::EINPROGRESS
}

#[derive(Debug)]
enum PendingState {
    SendAll {
        sent: usize,
    },
    UdpRecvBatch {
        iovecs: Box<[libc::iovec]>,
        msgs: Box<[libc::mmsghdr]>,
    },
}

#[derive(Debug)]
struct PendingOp {
    token: OpToken,
    kind: NetOpKind,
    op: NetOp,
    state: Option<PendingState>,
    watched_fd: Option<RawFd>,
}

struct SubscriptionState {
    kind: NetSubscription,
    handler: Box<dyn Handler<NetSubscriptionEvent>>,
    watched_fd: Option<RawFd>,
}

enum SendAllProgress {
    Pending(usize),
    Complete(i32),
}

pub struct SyscallReactor {
    next_token: u64,
    pending: VecDeque<PendingOp>,
    ready_events: VecDeque<NetEvent>,
    subscriptions: HashMap<SubscriptionToken, SubscriptionState>,
    epoll_fd: Option<RawFd>,
    watched_fds: HashMap<RawFd, usize>,
}

impl SyscallReactor {
    pub fn new() -> io::Result<Self> {
        // SAFETY: epoll_create1 is always valid with EPOLL_CLOEXEC; the kernel
        // returns a new epoll fd and owns it until closed. We own the returned fd
        // and close it in disable_epoll.
        let epoll_fd = unsafe { libc::epoll_create1(libc::EPOLL_CLOEXEC) };
        Ok(Self {
            next_token: 1,
            pending: VecDeque::new(),
            ready_events: VecDeque::new(),
            subscriptions: HashMap::new(),
            epoll_fd: if epoll_fd >= 0 { Some(epoll_fd) } else { None },
            watched_fds: HashMap::new(),
        })
    }

    fn alloc_token(&mut self) -> u64 {
        let out = self.next_token;
        self.next_token = self.next_token.wrapping_add(1).max(1);
        out
    }

    fn op_fd(op: &NetOp) -> RawFd {
        match op {
            NetOp::Connect { fd, .. }
            | NetOp::Send { fd, .. }
            | NetOp::SendAll { fd, .. }
            | NetOp::Recv { fd, .. }
            | NetOp::UdpSend { fd, .. }
            | NetOp::UdpRecv { fd, .. }
            | NetOp::UdpRecvBatch { fd, .. } => *fd,
        }
    }

    fn watch_fd(&mut self, fd: RawFd) -> Option<RawFd> {
        if fd < 0 {
            return None;
        }
        let epoll_fd = self.epoll_fd?;
        if let Some(refs) = self.watched_fds.get_mut(&fd) {
            *refs += 1;
            return Some(fd);
        }

        let mut event = libc::epoll_event {
            events: (libc::EPOLLIN | libc::EPOLLOUT | libc::EPOLLERR | libc::EPOLLHUP) as u32,
            u64: fd as u64,
        };
        // SAFETY: epoll_ctl is safe when epoll_fd is a valid epoll instance,
        // fd is a valid open file descriptor, and event points to a valid epoll_event.
        // The event is initialized above with the fd's interest mask.
        let rc = unsafe { libc::epoll_ctl(epoll_fd, libc::EPOLL_CTL_ADD, fd, &mut event) };
        if rc == 0 {
            self.watched_fds.insert(fd, 1);
            Some(fd)
        } else {
            None
        }
    }

    fn unwatch_fd(&mut self, watched_fd: Option<RawFd>) {
        let Some(fd) = watched_fd else {
            return;
        };
        let Some(epoll_fd) = self.epoll_fd else {
            return;
        };
        let Some(refs) = self.watched_fds.get_mut(&fd) else {
            return;
        };
        if *refs > 1 {
            *refs -= 1;
            return;
        }

        self.watched_fds.remove(&fd);
        // SAFETY: epoll_ctl DEL is safe with a valid epoll_fd and closed fd.
        // The null event is allowed for DELETE; the kernel ignores the event field.
        let _ = unsafe { libc::epoll_ctl(epoll_fd, libc::EPOLL_CTL_DEL, fd, std::ptr::null_mut()) };
    }

    fn disable_epoll(&mut self) {
        if let Some(fd) = self.epoll_fd.take() {
            // SAFETY: closing our own epoll fd is safe; we only close what we created.
            let _ = unsafe { libc::close(fd) };
        }
        self.watched_fds.clear();
    }

    fn wait_for_epoll(&mut self) -> bool {
        self.wait_for_epoll_timeout(-1)
    }

    fn wait_for_epoll_timeout(&mut self, timeout_ms: i32) -> bool {
        let Some(epoll_fd) = self.epoll_fd else {
            return false;
        };
        if self.watched_fds.is_empty() {
            return false;
        }

        let mut events = [libc::epoll_event { events: 0, u64: 0 }; 64];
        // SAFETY: epoll_wait is safe when epoll_fd is valid and events points to
        // a valid slice of epoll_event. The kernel writes up to 64 events into the
        // buffer and returns the count. We own the epoll_fd and close it in drop.
        let rc = unsafe {
            libc::epoll_wait(epoll_fd, events.as_mut_ptr(), events.len() as i32, timeout_ms)
        };
        if rc >= 0 {
            return true;
        }
        let errno = last_errno();
        if errno == libc::EINTR {
            return true;
        }
        self.disable_epoll();
        false
    }

    fn try_simple(op: &NetOp) -> Option<i32> {
        let res: isize = match op {
            // SAFETY: connect is safe when fd is a valid socket and addr points to
            // a valid sockaddr with correct length. The kernel reads addr during the call.
            NetOp::Connect { fd, addr } => unsafe {
                libc::connect(*fd, addr.as_ptr(), addr.len()) as isize
            },
            // SAFETY: send is safe when fd is a valid socket, ptr points to valid memory
            // of at least len bytes, and MSG_DONTWAIT makes it non-blocking.
            NetOp::Send { fd, ptr, len } => unsafe {
                libc::send(*fd, ptr.cast(), *len, libc::MSG_DONTWAIT)
            },
            // SAFETY: recv is safe when fd is a valid socket, ptr points to a writable
            // buffer of at least len bytes, and MSG_DONTWAIT makes it non-blocking.
            NetOp::Recv { fd, ptr, len } => unsafe {
                libc::recv(*fd, ptr.cast(), *len, libc::MSG_DONTWAIT)
            },
            // SAFETY: send for UDP is safe when fd is a valid UDP socket, ptr points to
            // valid memory of at least len bytes, and MSG_DONTWAIT makes it non-blocking.
            NetOp::UdpSend { fd, ptr, len } => unsafe {
                libc::send(*fd, ptr.cast(), *len, libc::MSG_DONTWAIT)
            },
            // SAFETY: recv for UDP is safe when fd is a valid UDP socket, ptr points to
            // a writable buffer of at least len bytes, and MSG_DONTWAIT makes it non-blocking.
            NetOp::UdpRecv { fd, ptr, len } => unsafe {
                libc::recv(*fd, ptr.cast(), *len, libc::MSG_DONTWAIT)
            },
            NetOp::UdpRecvBatch { .. } => unreachable!("udp recv batch handled separately"),
            NetOp::SendAll { .. } => unreachable!("send-all handled separately"),
        };
        if res >= 0 {
            return Some(res as i32);
        }
        let errno = last_errno();
        // EINTR: the op made no progress; keep it pending and retry on the
        // next poll rather than surfacing a signal as a hard completion error.
        if is_would_block(errno) || errno == libc::EINTR {
            None
        } else {
            Some(-errno)
        }
    }

    fn try_send_all(
        fd: RawFd,
        ptr: *const u8,
        len: usize,
        mut sent: usize,
    ) -> Result<SendAllProgress, i32> {
        while sent < len {
            // SAFETY: ptr is valid for at least len bytes; we advance by sent < len each
            // iteration, so next_ptr is always within the original buffer.
            let next_ptr = unsafe { ptr.add(sent) };
            // SAFETY: send with DONTWAIT is safe when fd is a valid socket, next_ptr
            // points to valid memory of at least len-sent bytes.
            let rc = unsafe { libc::send(fd, next_ptr.cast(), len - sent, libc::MSG_DONTWAIT) };
            if rc > 0 {
                sent += rc as usize;
                continue;
            }
            if rc == 0 {
                return Err(libc::EPIPE);
            }
            let errno = last_errno();
            if is_would_block(errno) {
                return Ok(SendAllProgress::Pending(sent));
            }
            return Err(errno);
        }
        Ok(SendAllProgress::Complete(len as i32))
    }

    fn poll_once(&mut self) -> Vec<NetEvent> {
        let mut out = Vec::new();

        let mut remaining = self.pending.len();
        while remaining > 0 {
            remaining -= 1;
            let Some(mut pending) = self.pending.pop_front() else {
                break;
            };

            match (&pending.op, pending.state.as_ref()) {
                (NetOp::SendAll { fd, ptr, len }, state) => {
                    let sent = match state {
                        Some(PendingState::SendAll { sent }) => *sent,
                        None => 0,
                        Some(PendingState::UdpRecvBatch { .. }) => 0,
                    };
                    match Self::try_send_all(*fd, *ptr, *len, sent) {
                        Ok(SendAllProgress::Complete(result)) => {
                            self.unwatch_fd(pending.watched_fd);
                            out.push(NetEvent::OpComplete {
                                token: pending.token,
                                kind: pending.kind,
                                result,
                                flags: 0,
                            });
                        }
                        Ok(SendAllProgress::Pending(sent)) => {
                            pending.state = Some(PendingState::SendAll { sent });
                            self.pending.push_back(pending);
                        }
                        Err(errno) => {
                            self.unwatch_fd(pending.watched_fd);
                            out.push(NetEvent::OpComplete {
                                token: pending.token,
                                kind: pending.kind,
                                result: -errno,
                                flags: 0,
                            });
                        }
                    }
                }
                (
                    NetOp::UdpRecvBatch {
                        fd,
                        slots_ptr,
                        slots_len,
                        flags,
                        ..
                    },
                    Some(PendingState::UdpRecvBatch { .. }),
                ) => {
                    let state = pending
                        .state
                        .as_mut()
                        .expect("udp recv batch state must exist");
                    match Self::try_udp_recv_batch(*fd, *slots_ptr, *slots_len, *flags, state) {
                        Ok(Some(count)) => {
                            self.unwatch_fd(pending.watched_fd);
                            out.push(NetEvent::OpComplete {
                                token: pending.token,
                                kind: pending.kind,
                                result: count as i32,
                                flags: 0,
                            });
                        }
                        Ok(None) => self.pending.push_back(pending),
                        Err(errno) => {
                            self.unwatch_fd(pending.watched_fd);
                            out.push(NetEvent::OpComplete {
                                token: pending.token,
                                kind: pending.kind,
                                result: -errno,
                                flags: 0,
                            });
                        }
                    }
                }
                _ => match Self::try_simple(&pending.op) {
                    Some(result) => {
                        self.unwatch_fd(pending.watched_fd);
                        out.push(NetEvent::OpComplete {
                            token: pending.token,
                            kind: pending.kind,
                            result,
                            flags: 0,
                        });
                    }
                    None => self.pending.push_back(pending),
                },
            }
        }

        let tokens: Vec<_> = self.subscriptions.keys().copied().collect();
        for token in tokens {
            let mut should_cancel = false;
            if let Some(state) = self.subscriptions.get_mut(&token) {
                match state.kind {
                    NetSubscription::AcceptMulti { listener_fd, flags } => loop {
                        // SAFETY: accept4 is safe when listener_fd is a valid listening socket.
                        // The null pointers for addr and addrlen are allowed — we don't need
                        // peer address information here. SOCK_NONBLOCK is already set so the
                        // result fd is non-blocking.
                        let accepted = unsafe {
                            libc::accept4(
                                listener_fd,
                                std::ptr::null_mut(),
                                std::ptr::null_mut(),
                                flags | libc::SOCK_NONBLOCK,
                            )
                        };
                        if accepted >= 0 {
                            let mut cx = EventContext::default();
                            state
                                .handler
                                .on_event(&mut cx, NetSubscriptionEvent::Accepted { fd: accepted });
                            for action in cx.actions {
                                if let super::FollowupAction::Cancel(cancel_token) = action {
                                    if cancel_token == token {
                                        should_cancel = true;
                                    }
                                }
                            }
                            if should_cancel {
                                break;
                            }
                            continue;
                        }
                        let errno = last_errno();
                        if is_would_block(errno) || errno == libc::EINTR {
                            break;
                        }
                        let mut cx = EventContext::default();
                        state
                            .handler
                            .on_error(&mut cx, io::Error::from_raw_os_error(errno));
                        break;
                    },
                }
            }
            if should_cancel {
                if let Some(state) = self.subscriptions.remove(&token) {
                    self.unwatch_fd(state.watched_fd);
                }
            }
        }

        out
    }

    fn init_udp_recv_batch_state(
        slots_ptr: *mut super::UdpRecvSlot,
        slots_len: usize,
    ) -> io::Result<PendingState> {
        if slots_len == 0 {
            return Ok(PendingState::UdpRecvBatch {
                iovecs: Box::new([]),
                msgs: Box::new([]),
            });
        }
        if slots_ptr.is_null() {
            return Err(io::Error::from_raw_os_error(libc::EINVAL));
        }
        // SAFETY: slots_ptr is non-null and slots_len > 0. The caller owns the memory
        // and guarantees it is valid for slots_len UdpRecvSlot elements.
        let slots = unsafe { std::slice::from_raw_parts_mut(slots_ptr, slots_len) };
        let mut iovecs = vec![
            libc::iovec {
                iov_base: std::ptr::null_mut(),
                iov_len: 0,
            };
            slots_len
        ];
        let mut msgs = vec![
            libc::mmsghdr {
                msg_hdr: libc::msghdr {
                    msg_name: std::ptr::null_mut(),
                    msg_namelen: 0,
                    msg_iov: std::ptr::null_mut(),
                    msg_iovlen: 0,
                    msg_control: std::ptr::null_mut(),
                    msg_controllen: 0,
                    msg_flags: 0,
                },
                msg_len: 0,
            };
            slots_len
        ];

        for idx in 0..slots_len {
            let slot = &mut slots[idx];
            slot.recv_len = 0;
            slot.addr_len = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;

            iovecs[idx].iov_base = slot.buf_ptr.cast();
            iovecs[idx].iov_len = slot.buf_len;
            msgs[idx].msg_len = 0;
            msgs[idx].msg_hdr.msg_name = (&mut slot.addr as *mut libc::sockaddr_storage).cast();
            msgs[idx].msg_hdr.msg_namelen = slot.addr_len;
            msgs[idx].msg_hdr.msg_iov = &mut iovecs[idx] as *mut libc::iovec;
            msgs[idx].msg_hdr.msg_iovlen = 1;
            msgs[idx].msg_hdr.msg_control = std::ptr::null_mut();
            msgs[idx].msg_hdr.msg_controllen = 0;
            msgs[idx].msg_hdr.msg_flags = 0;
        }

        Ok(PendingState::UdpRecvBatch {
            iovecs: iovecs.into_boxed_slice(),
            msgs: msgs.into_boxed_slice(),
        })
    }

    fn try_udp_recv_batch(
        fd: RawFd,
        slots_ptr: *mut super::UdpRecvSlot,
        slots_len: usize,
        flags: i32,
        state: &mut PendingState,
    ) -> Result<Option<usize>, i32> {
        if slots_len == 0 {
            return Ok(Some(0));
        }
        let (iovecs, msgs) = match state {
            PendingState::UdpRecvBatch { iovecs, msgs } => (iovecs, msgs),
            _ => return Err(libc::EINVAL),
        };
        if iovecs.len() != slots_len || msgs.len() != slots_len {
            return Err(libc::EINVAL);
        }
        if slots_ptr.is_null() {
            return Err(libc::EINVAL);
        }
        // SAFETY: slots_ptr is non-null and slots_len > 0. The caller owns the memory
        // and guarantees it is valid for slots_len UdpRecvSlot elements.
        let slots = unsafe { std::slice::from_raw_parts_mut(slots_ptr, slots_len) };
        for idx in 0..slots_len {
            slots[idx].recv_len = 0;
            slots[idx].addr_len = std::mem::size_of::<libc::sockaddr_storage>() as libc::socklen_t;
            iovecs[idx].iov_base = slots[idx].buf_ptr.cast();
            iovecs[idx].iov_len = slots[idx].buf_len;
            msgs[idx].msg_len = 0;
            msgs[idx].msg_hdr.msg_name =
                (&mut slots[idx].addr as *mut libc::sockaddr_storage).cast();
            msgs[idx].msg_hdr.msg_namelen = slots[idx].addr_len;
            msgs[idx].msg_hdr.msg_iov = &mut iovecs[idx] as *mut libc::iovec;
            msgs[idx].msg_hdr.msg_iovlen = 1;
            msgs[idx].msg_hdr.msg_control = std::ptr::null_mut();
            msgs[idx].msg_hdr.msg_controllen = 0;
            msgs[idx].msg_hdr.msg_flags = 0;
        }
        // SAFETY: recvmmsg is safe when fd is a valid socket, msgs points to a valid
        // slice of mmsghdr with iovecs pointing into valid buffers, and flags contains
        // MSG_DONTWAIT. The kernel fills in to every msgs entry atomically.
        let rc = unsafe {
            libc::recvmmsg(
                fd,
                msgs.as_mut_ptr(),
                slots_len as u32,
                flags | libc::MSG_DONTWAIT,
                std::ptr::null_mut(),
            )
        };
        if rc < 0 {
            let errno = last_errno();
            if errno == libc::EINTR || is_would_block(errno) {
                return Ok(None);
            }
            return Err(errno);
        }
        let received = rc as usize;
        for idx in 0..received {
            slots[idx].recv_len = msgs[idx].msg_len as usize;
            slots[idx].addr_len = msgs[idx].msg_hdr.msg_namelen;
        }
        Ok(Some(received))
    }

    fn submit_with_token(&mut self, op: NetOp, token: OpToken) -> Result<(), io::Error> {
        let kind = match &op {
            NetOp::Connect { .. } => NetOpKind::Connect,
            NetOp::Send { .. } => NetOpKind::Send,
            NetOp::SendAll { .. } => NetOpKind::SendAll,
            NetOp::Recv { .. } => NetOpKind::Recv,
            NetOp::UdpSend { .. } => NetOpKind::UdpSend,
            NetOp::UdpRecv { .. } => NetOpKind::UdpRecv,
            NetOp::UdpRecvBatch { .. } => NetOpKind::UdpRecvBatch,
        };
        let state = match &op {
            NetOp::SendAll { .. } => Some(PendingState::SendAll { sent: 0 }),
            NetOp::UdpRecvBatch {
                slots_ptr,
                slots_len,
                ..
            } => Some(Self::init_udp_recv_batch_state(*slots_ptr, *slots_len)?),
            _ => None,
        };
        let watched_fd = self.watch_fd(Self::op_fd(&op));
        self.pending.push_back(PendingOp {
            token,
            kind,
            op,
            state,
            watched_fd,
        });
        Ok(())
    }

    fn drain_ready_events<F>(&mut self, max_events: usize, mut on_event: F) -> usize
    where
        F: FnMut(NetEvent),
    {
        let mut drained = 0usize;
        let limit = max_events.max(1);
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

impl Reactor for SyscallReactor {
    type Error = io::Error;

    fn submit(&mut self, op: NetOp) -> Result<OpToken, Self::Error> {
        let raw = self.alloc_token();
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
        let token = SubscriptionToken(self.alloc_token());
        let watched_fd = match op {
            NetSubscription::AcceptMulti { listener_fd, .. } => self.watch_fd(listener_fd),
        };
        self.subscriptions.insert(
            token,
            SubscriptionState {
                kind: op,
                handler: Box::new(handler),
                watched_fd,
            },
        );
        Ok(token)
    }

    fn cancel(&mut self, token: SubscriptionToken) -> Result<(), Self::Error> {
        if let Some(state) = self.subscriptions.remove(&token) {
            self.unwatch_fd(state.watched_fd);
        }
        Ok(())
    }

    fn submit_pending(&mut self) -> Result<usize, Self::Error> {
        Ok(self.pending.len())
    }

    fn poll(&mut self, wait: bool) -> Result<Vec<NetEvent>, Self::Error> {
        if !self.ready_events.is_empty() {
            return Ok(self.ready_events.drain(..).collect());
        }
        loop {
            let events = self.poll_once();
            if !events.is_empty() || !wait {
                return Ok(events);
            }
            if self.pending.is_empty() && self.subscriptions.is_empty() {
                return Ok(events);
            }
            if !self.wait_for_epoll() {
                std::thread::sleep(Duration::from_micros(50));
            }
        }
    }
}

impl ReactorDriver for SyscallReactor {
    type Error = io::Error;
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
        self.pending.len()
    }

    fn capabilities(&self) -> DriverCapabilities {
        DriverCapabilities {
            backend: DriverBackendKind::Syscall,
            supports_accept_multi: true,
            supports_multishot: false,
            supports_fixed_buffers: false,
            supports_sqpoll: false,
        }
    }
}

impl ReactorDriverWait for SyscallReactor {
    type Error = io::Error;
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

        let events = self.poll_once();
        if events.is_empty() {
            let timeout_ms = timeout_ns
                .div_ceil(1_000_000)
                .min(i32::MAX as u64) as i32;
            if !self.wait_for_epoll_timeout(timeout_ms) {
                // No epoll: sleep a bounded slice of the deadline. 1ms keeps
                // pending-op retries responsive without the previous 50us
                // busy-spin across long deadlines.
                std::thread::sleep(Duration::from_nanos(timeout_ns.min(1_000_000)));
            }
            let events = self.poll_once();
            let count = events.len();
            self.ready_events.extend(events);
            return Ok(count);
        }
        let count = events.len();
        self.ready_events.extend(events);
        Ok(count)
    }
}

impl Drop for SyscallReactor {
    fn drop(&mut self) {
        self.disable_epoll();
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::reactor::UdpRecvSlot;
    use std::net::UdpSocket;

    fn socket_pair_stream_nonblocking() -> (RawFd, RawFd) {
        let mut fds = [0i32; 2];
        let rc = unsafe { libc::socketpair(libc::AF_UNIX, libc::SOCK_STREAM, 0, fds.as_mut_ptr()) };
        assert_eq!(rc, 0, "socketpair failed");
        let set_nonblocking = |fd: RawFd| {
            let flags = unsafe { libc::fcntl(fd, libc::F_GETFL) };
            assert!(flags >= 0, "fcntl(F_GETFL) failed");
            let rc = unsafe { libc::fcntl(fd, libc::F_SETFL, flags | libc::O_NONBLOCK) };
            assert_eq!(rc, 0, "fcntl(F_SETFL) failed");
        };
        set_nonblocking(fds[0]);
        set_nonblocking(fds[1]);
        (fds[0], fds[1])
    }

    #[test]
    fn send_zero_completes() {
        let (tx_fd, rx_fd) = socket_pair_stream_nonblocking();
        let mut reactor = SyscallReactor::new().expect("new reactor");
        let zero = [0u8; 1];
        let token = Reactor::submit(
            &mut reactor,
            NetOp::Send {
                fd: tx_fd,
                ptr: zero.as_ptr(),
                len: 0,
            },
        )
        .expect("submit");
        let _ = reactor.submit_pending().expect("submit pending");
        let mut got = None;
        for _ in 0..64 {
            let events = reactor.poll(false).expect("poll");
            for event in events {
                let NetEvent::OpComplete {
                    token: done,
                    result,
                    ..
                } = event;
                if done == token {
                    got = Some(result);
                    break;
                }
            }
            if got.is_some() {
                break;
            }
        }
        let _ = unsafe { libc::close(tx_fd) };
        let _ = unsafe { libc::close(rx_fd) };
        assert_eq!(got, Some(0));
    }

    #[test]
    fn udp_recv_batch_completes_with_single_datagram() {
        let server = UdpSocket::bind("127.0.0.1:0").expect("bind server");
        server
            .set_nonblocking(true)
            .expect("set nonblocking server");
        let server_addr = server.local_addr().expect("server addr");
        let client = UdpSocket::bind("127.0.0.1:0").expect("bind client");
        let payload = b"ping";
        let sent = client.send_to(payload, server_addr).expect("send datagram");
        assert_eq!(sent, payload.len());

        let mut recv_buf = [0u8; 64];
        let mut slot = UdpRecvSlot {
            buf_ptr: recv_buf.as_mut_ptr(),
            buf_len: recv_buf.len(),
            recv_len: 0,
            addr: unsafe { std::mem::zeroed::<libc::sockaddr_storage>() },
            addr_len: 0,
        };

        let fd = std::os::fd::AsRawFd::as_raw_fd(&server);
        let mut reactor = SyscallReactor::new().expect("new reactor");
        let token = Reactor::submit(
            &mut reactor,
            NetOp::UdpRecvBatch {
                fd,
                slots_ptr: &mut slot as *mut UdpRecvSlot,
                slots_len: 1,
                flags: libc::MSG_DONTWAIT,
                prefer_multishot: false,
            },
        )
        .expect("submit udp recv batch");
        let _ = reactor.submit_pending().expect("submit pending");

        let mut got = None;
        for _ in 0..128 {
            let events = reactor.poll(true).expect("poll");
            for event in events {
                let NetEvent::OpComplete {
                    token: done,
                    kind,
                    result,
                    ..
                } = event;
                if done == token {
                    assert_eq!(kind, NetOpKind::UdpRecvBatch);
                    got = Some(result);
                    break;
                }
            }
            if got.is_some() {
                break;
            }
        }
        assert_eq!(got, Some(1));
        assert_eq!(slot.recv_len, payload.len());
        assert_eq!(&recv_buf[..payload.len()], payload);
        assert!(slot.addr_len > 0);
    }
}
