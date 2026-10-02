use std::os::fd::RawFd;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReactorClass {
    LowLatency,
    Throughput,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct SubscriptionToken(pub u64);

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct OpToken(pub u64);

pub trait Operation {
    type Output;
    const CLASS: ReactorClass;
}

pub trait SubscriptionOp {
    type Event;
    const CLASS: ReactorClass;
}

pub trait Handler<E>: Send {
    fn on_event(&mut self, cx: &mut EventContext<'_>, event: E);
    fn on_error(&mut self, _cx: &mut EventContext<'_>, _error: std::io::Error) {}
    fn on_closed(&mut self, _cx: &mut EventContext<'_>) {}
}

#[derive(Default)]
pub struct EventContext<'a> {
    pub actions: Vec<FollowupAction<'a>>,
}

pub enum FollowupAction<'a> {
    Cancel(SubscriptionToken),
    Tag(&'a str),
}

/// Caller-owned receive descriptor. Both this slot and its writable buffer must
/// stay at stable addresses until the batch's terminal completion.
#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct UdpRecvSlot {
    pub buf_ptr: *mut u8,
    pub buf_len: usize,
    pub recv_len: usize,
    pub addr: libc::sockaddr_storage,
    pub addr_len: libc::socklen_t,
}

/// Low-level, caller-owned network operation description.
///
/// This is not an owned-buffer API: cloning an operation copies pointers and fd
/// numbers, not the resources. Before submission, the caller must arrange for fds,
/// buffers, and batch descriptors to remain valid and at stable addresses until
/// terminal completion or confirmed backend teardown. Send buffers must remain
/// readable and unmodified; receive buffers must remain exclusively writable by
/// the backend. A cancellation request alone does not end these requirements.
///
/// These lifetime/aliasing obligations are not encoded in this legacy raw-pointer
/// interface. Prefer an owned transport adapter for safe application-level I/O.
#[derive(Clone, Debug)]
pub enum NetOp {
    Connect {
        fd: RawFd,
        addr: socket2::SockAddr,
    },
    Send {
        fd: RawFd,
        ptr: *const u8,
        len: usize,
    },
    SendAll {
        fd: RawFd,
        ptr: *const u8,
        len: usize,
    },
    Recv {
        fd: RawFd,
        ptr: *mut u8,
        len: usize,
    },
    UdpSend {
        fd: RawFd,
        ptr: *const u8,
        len: usize,
    },
    UdpRecv {
        fd: RawFd,
        ptr: *mut u8,
        len: usize,
    },
    UdpRecvBatch {
        fd: RawFd,
        slots_ptr: *mut UdpRecvSlot,
        slots_len: usize,
        flags: i32,
        prefer_multishot: bool,
    },
}

#[cfg(test)]
mod raw_net_op_tests {
    use super::{NetOp, RawNetOp};

    #[test]
    fn wrapper_preserves_raw_operation() {
        let bytes = [1u8, 2, 3];
        // SAFETY: bytes stays in scope and unchanged throughout this synchronous test.
        let validated = unsafe { RawNetOp::new(NetOp::Send { fd: 7, ptr: bytes.as_ptr(), len: bytes.len() }) };
        assert!(matches!(validated.into_inner(), NetOp::Send { fd: 7, len: 3, .. }));
    }
}


/// An operation whose external resources have been validated for asynchronous use.
///
/// This wrapper owns only the operation description, not the referenced fds or buffers.
/// It is intentionally not Clone so the submission token has a single admission value.
pub struct RawNetOp {
    op: NetOp,
}

impl RawNetOp {
    /// Creates an asynchronous operation from a raw-pointer NetOp.
    ///
    /// # Safety
    /// Every referenced file descriptor must remain open and continue to identify the
    /// intended socket until terminal completion or confirmed backend teardown. Send
    /// buffers must be valid, readable, and unmodified; receive buffers must be valid,
    /// writable, and exclusively writable by the backend. All buffers and, for
    /// UdpRecvBatch, the slots array and each pointed-to buffer must stay at stable
    /// addresses until terminal completion or teardown. A cancellation request alone
    /// does not end these obligations. They remain the caller's responsibility even
    /// if submission returns an error or this wrapper is dropped. Do not submit a
    /// cloned NetOp concurrently unless all aliasing obligations are independently met.
    pub unsafe fn new(op: NetOp) -> Self {
        Self { op }
    }

    /// Consumes the validated wrapper into the backend's raw operation description.
    pub fn into_inner(self) -> NetOp {
        self.op
    }
}


#[derive(Clone, Copy, Debug)]
pub enum NetSubscription {
    AcceptMulti { listener_fd: RawFd, flags: i32 },
}

#[derive(Clone, Copy, Debug)]
pub enum NetSubscriptionEvent {
    Accepted { fd: RawFd },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum NetOpKind {
    Connect,
    Send,
    SendAll,
    Recv,
    UdpSend,
    UdpRecv,
    UdpRecvBatch,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ReactorCompletion {
    pub token: OpToken,
    pub kind: NetOpKind,
    pub result: i32,
    pub flags: u32,
}

#[derive(Clone, Copy, Debug)]
pub enum NetEvent {
    OpComplete {
        token: OpToken,
        kind: NetOpKind,
        result: i32,
        flags: u32,
    },
}

impl From<NetEvent> for ReactorCompletion {
    #[inline(always)]
    fn from(event: NetEvent) -> Self {
        match event {
            NetEvent::OpComplete {
                token,
                kind,
                result,
                flags,
            } => Self {
                token,
                kind,
                result,
                flags,
            },
        }
    }
}

pub mod net {
    use super::{Operation, ReactorClass, SubscriptionOp};
    use std::net::SocketAddr;
    use std::os::fd::RawFd;
    use std::sync::Arc;

    #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
    pub struct ConnHandle(pub u64);

    #[derive(Clone, Copy, Debug)]
    pub struct AcceptMulti {
        pub listener_fd: RawFd,
        pub flags: i32,
    }

    #[derive(Clone, Copy, Debug)]
    pub struct AcceptedConn {
        pub fd: RawFd,
    }

    impl SubscriptionOp for AcceptMulti {
        type Event = AcceptedConn;
        const CLASS: ReactorClass = ReactorClass::LowLatency;
    }

    #[derive(Clone, Debug)]
    pub struct Connect {
        pub addr: SocketAddr,
    }

    impl Operation for Connect {
        type Output = ConnHandle;
        const CLASS: ReactorClass = ReactorClass::LowLatency;
    }

    #[derive(Clone, Debug)]
    pub struct Send {
        pub conn: ConnHandle,
        pub payload: Arc<Vec<u8>>,
        pub offset: usize,
    }

    impl Operation for Send {
        type Output = usize;
        const CLASS: ReactorClass = ReactorClass::Throughput;
    }

    #[derive(Clone, Debug)]
    pub struct Recv {
        pub conn: ConnHandle,
        pub len: usize,
    }

    impl Operation for Recv {
        type Output = usize;
        const CLASS: ReactorClass = ReactorClass::LowLatency;
    }

    #[derive(Clone, Copy, Debug)]
    pub struct Close {
        pub conn: ConnHandle,
    }

    impl Operation for Close {
        type Output = ();
        const CLASS: ReactorClass = ReactorClass::LowLatency;
    }
}
