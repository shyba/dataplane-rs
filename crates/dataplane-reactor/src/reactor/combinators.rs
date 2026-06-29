use std::os::fd::RawFd;

use crate::reactor::{
    net, EventContext, Handler, NetSubscription, NetSubscriptionEvent, Reactor, SubscriptionToken,
};

struct AcceptFnHandler<F>
where
    F: FnMut(net::AcceptedConn) + Send + 'static,
{
    f: F,
}

impl<F> Handler<NetSubscriptionEvent> for AcceptFnHandler<F>
where
    F: FnMut(net::AcceptedConn) + Send + 'static,
{
    fn on_event(&mut self, _cx: &mut EventContext<'_>, event: NetSubscriptionEvent) {
        match event {
            NetSubscriptionEvent::Accepted { fd } => (self.f)(net::AcceptedConn { fd }),
        }
    }
}

pub fn subscribe_accept_with<R, F>(
    reactor: &mut R,
    listener_fd: RawFd,
    flags: i32,
    f: F,
) -> Result<SubscriptionToken, R::Error>
where
    R: Reactor,
    F: FnMut(net::AcceptedConn) + Send + 'static,
{
    reactor.subscribe(
        NetSubscription::AcceptMulti { listener_fd, flags },
        AcceptFnHandler { f },
    )
}
