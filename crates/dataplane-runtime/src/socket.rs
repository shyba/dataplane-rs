use crossbeam_channel::Receiver;
use rustler::{LocalPid, ResourceArc};

pub use crate::runtime_protocol::NetAddr;
use std::os::fd::RawFd;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::thread;

const SESSION_API_CLOSED: usize = 1usize << (usize::BITS as usize - 1);

pub struct SocketRef {
    pub kind: SocketKind,
}

pub enum SocketKind {
    Listener(ListenerState),
    Session(SessionState),
}

pub struct ListenerState {
    pub listener_id: u64,
    pub accept_rxs: Vec<Receiver<ResourceArc<SocketRef>>>,
    pub local: NetAddr,
    pub closed: AtomicBool,
}

pub struct SessionState {
    pub session_id: u64,
    pub shard: usize,
    pub fd: RawFd,
    pub owner: Mutex<LocalPid>,
    pub opts: Mutex<SocketOpts>,
    pub mailbox_passive: AtomicBool,
    pub local: Mutex<Option<NetAddr>>,
    pub peer: Mutex<Option<NetAddr>>,
    pub api_state: AtomicUsize,
    pub closed: AtomicBool,
}

impl SessionState {
    pub fn begin_api_call(&self) -> bool {
        loop {
            let state = self.api_state.load(Ordering::Acquire);
            if (state & SESSION_API_CLOSED) != 0 {
                return false;
            }
            let Some(next) = state.checked_add(1) else {
                return false;
            };
            if self
                .api_state
                .compare_exchange(state, next, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return true;
            }
        }
    }

    pub fn end_api_call(&self) {
        let prev = self.api_state.fetch_sub(1, Ordering::AcqRel);
        debug_assert_eq!(prev & !SESSION_API_CLOSED, 1);
    }

    pub fn mark_api_closed(&self) -> bool {
        self.closed.store(true, Ordering::Release);
        loop {
            let state = self.api_state.load(Ordering::Acquire);
            if (state & SESSION_API_CLOSED) != 0 {
                return false;
            }
            let next = state | SESSION_API_CLOSED;
            if self
                .api_state
                .compare_exchange(state, next, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                while (self.api_state.load(Ordering::Acquire) & !SESSION_API_CLOSED) != 0 {
                    thread::yield_now();
                }
                return true;
            }
        }
    }

    pub fn note_runtime_closed(&self) {
        self.closed.store(true, Ordering::Release);
        self.api_state
            .fetch_or(SESSION_API_CLOSED, Ordering::AcqRel);
    }

    pub fn is_closed(&self) -> bool {
        (self.api_state.load(Ordering::Acquire) & SESSION_API_CLOSED) != 0
    }
}

pub struct SocketOpts {
    pub active: ActiveMode,
    pub packet: PacketMode,
    pub buffer: usize,
    pub nodelay: bool,
}

impl Default for SocketOpts {
    fn default() -> Self {
        SocketOpts {
            active: ActiveMode::False,
            packet: PacketMode::Raw,
            buffer: 8192,
            nodelay: false,
        }
    }
}

#[derive(Clone, Debug)]
pub enum ActiveMode {
    False,
    True,
    Once,
    N(i32),
}

#[derive(Clone, Debug)]
pub enum PacketMode {
    Raw,
}


#[rustler::resource_impl]
impl rustler::Resource for SocketRef {}
