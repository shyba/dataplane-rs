use std::io;

use super::syscall::SyscallReactor;
use super::uring::UringReactor;
use super::{
    DriverBackendKind as ReactorBackendKind, DriverCapabilities as ReactorCapabilities, Handler,
    NetEvent, NetOp, NetSubscription, NetSubscriptionEvent, OpToken, RawNetOp, Reactor,
    ReactorDriver, ReactorDriverWait, SubscriptionToken,
};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ReactorBackend {
    Auto,
    IoUring,
    Syscall,
}

#[allow(clippy::large_enum_variant)]
pub enum UnifiedReactor {
    IoUring(UringReactor),
    Syscall(SyscallReactor),
}

impl ReactorDriver for UnifiedReactor {
    type Error = io::Error;
    type Token = OpToken;
    type Submit = RawNetOp;
    type Event = NetEvent;

    fn submit(&mut self, op: Self::Submit, token: Self::Token) -> Result<(), Self::Error> {
        match self {
            Self::IoUring(r) => ReactorDriver::submit(r, op, token),
            Self::Syscall(r) => ReactorDriver::submit(r, op, token),
        }
    }

    fn flush(&mut self) -> Result<usize, Self::Error> {
        match self {
            Self::IoUring(r) => ReactorDriver::flush(r),
            Self::Syscall(r) => ReactorDriver::flush(r),
        }
    }

    fn drain<F>(&mut self, max_events: usize, on_event: F) -> Result<usize, Self::Error>
    where
        F: FnMut(Self::Event),
    {
        match self {
            Self::IoUring(r) => ReactorDriver::drain(r, max_events, on_event),
            Self::Syscall(r) => ReactorDriver::drain(r, max_events, on_event),
        }
    }

    fn outstanding(&self) -> usize {
        match self {
            Self::IoUring(r) => ReactorDriver::outstanding(r),
            Self::Syscall(r) => ReactorDriver::outstanding(r),
        }
    }

    fn capabilities(&self) -> ReactorCapabilities {
        match self {
            Self::IoUring(r) => ReactorDriver::capabilities(r),
            Self::Syscall(r) => ReactorDriver::capabilities(r),
        }
    }
}

impl ReactorDriverWait for UnifiedReactor {
    type Error = io::Error;
    type Readiness = ();

    fn readiness(&self) -> Option<Self::Readiness> {
        None
    }

    fn wait(&mut self, min_events: usize) -> Result<usize, Self::Error> {
        match self {
            Self::IoUring(r) => ReactorDriverWait::wait(r, min_events),
            Self::Syscall(r) => ReactorDriverWait::wait(r, min_events),
        }
    }
    fn wait_deadline(
        &mut self,
        min_events: usize,
        timeout_ns: Option<u64>,
    ) -> Result<usize, Self::Error> {
        match self {
            Self::IoUring(r) => r.wait_deadline(min_events, timeout_ns),
            Self::Syscall(r) => r.wait_deadline(min_events, timeout_ns),
        }
    }
}

impl UnifiedReactor {
    pub fn from_env(entries: u32) -> io::Result<Self> {
        let backend = std::env::var("DATAPLANE_REACTOR_BACKEND")
            .ok()
            .map(|v| v.to_ascii_lowercase())
            .map(|v| match v.as_str() {
                "uring" | "io_uring" => ReactorBackend::IoUring,
                "syscall" | "poll" | "epoll" => ReactorBackend::Syscall,
                _ => ReactorBackend::Auto,
            })
            .unwrap_or(ReactorBackend::Auto);
        Self::new(entries, backend)
    }

    pub fn new(entries: u32, backend: ReactorBackend) -> io::Result<Self> {
        match backend {
            ReactorBackend::IoUring => Ok(Self::IoUring(UringReactor::new(entries)?)),
            ReactorBackend::Syscall => Ok(Self::Syscall(SyscallReactor::new()?)),
            ReactorBackend::Auto => match UringReactor::new(entries) {
                Ok(r) => Ok(Self::IoUring(r)),
                Err(_e) => Ok(Self::Syscall(SyscallReactor::new()?)),
            },
        }
    }

    pub fn backend_kind(&self) -> ReactorBackendKind {
        match self {
            Self::IoUring(_) => ReactorBackendKind::IoUring,
            Self::Syscall(_) => ReactorBackendKind::Syscall,
        }
    }

    pub fn capabilities(&self) -> ReactorCapabilities {
        ReactorDriver::capabilities(self)
    }
}

impl Reactor for UnifiedReactor {
    type Error = io::Error;

    unsafe fn submit(&mut self, op: NetOp) -> Result<OpToken, Self::Error> {
        match self {
            Self::IoUring(r) => unsafe { Reactor::submit(r, op) },
            Self::Syscall(r) => unsafe { Reactor::submit(r, op) },
        }
    }

    fn subscribe<H>(
        &mut self,
        op: NetSubscription,
        handler: H,
    ) -> Result<SubscriptionToken, Self::Error>
    where
        H: Handler<NetSubscriptionEvent> + 'static,
    {
        match self {
            Self::IoUring(r) => r.subscribe(op, handler),
            Self::Syscall(r) => r.subscribe(op, handler),
        }
    }

    fn cancel(&mut self, token: SubscriptionToken) -> Result<(), Self::Error> {
        match self {
            Self::IoUring(r) => r.cancel(token),
            Self::Syscall(r) => r.cancel(token),
        }
    }

    fn submit_pending(&mut self) -> Result<usize, Self::Error> {
        match self {
            Self::IoUring(r) => r.submit_pending(),
            Self::Syscall(r) => r.submit_pending(),
        }
    }

    fn poll(&mut self, wait: bool) -> Result<Vec<NetEvent>, Self::Error> {
        match self {
            Self::IoUring(r) => r.poll(wait),
            Self::Syscall(r) => r.poll(wait),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn explicit_syscall_backend_reports_capabilities() {
        let reactor = UnifiedReactor::new(8, ReactorBackend::Syscall).expect("new syscall");
        let caps = reactor.capabilities();
        assert_eq!(caps.backend, ReactorBackendKind::Syscall);
        assert!(caps.supports_accept_multi);
        assert!(!caps.supports_multishot);
        assert!(!caps.supports_fixed_buffers);
        assert!(!caps.supports_sqpoll);
    }
}
