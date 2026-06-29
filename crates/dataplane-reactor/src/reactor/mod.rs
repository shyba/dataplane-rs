pub mod adaptive;
pub mod combinators;
pub mod syscall;
pub mod uring;

pub use dataplane_core_reactor::reactor_driver::{
    DriverBackendKind, DriverCapabilities, ReactorDriver, ReactorDriverWait,
};
pub use dataplane_core_reactor::reactor_model::{
    net, EventContext, FollowupAction, Handler, NetEvent, NetOp, NetOpKind, NetSubscription,
    NetSubscriptionEvent, OpToken, Operation, ReactorClass, SubscriptionOp, SubscriptionToken,
    UdpRecvSlot,
};

pub trait Reactor {
    type Error;

    fn submit(&mut self, op: NetOp) -> Result<OpToken, Self::Error>;
    fn subscribe<H>(
        &mut self,
        op: NetSubscription,
        handler: H,
    ) -> Result<SubscriptionToken, Self::Error>
    where
        H: Handler<NetSubscriptionEvent> + 'static;
    fn cancel(&mut self, token: SubscriptionToken) -> Result<(), Self::Error>;
    fn submit_pending(&mut self) -> Result<usize, Self::Error>;
    fn poll(&mut self, wait: bool) -> Result<Vec<NetEvent>, Self::Error>;
}
