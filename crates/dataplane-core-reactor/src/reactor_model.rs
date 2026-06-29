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

#[repr(C)]
#[derive(Clone, Copy, Debug)]
pub struct UdpRecvSlot {
    pub buf_ptr: *mut u8,
    pub buf_len: usize,
    pub recv_len: usize,
    pub addr: libc::sockaddr_storage,
    pub addr_len: libc::socklen_t,
}

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
