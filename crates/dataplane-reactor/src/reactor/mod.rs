pub mod adaptive;
pub mod combinators;
pub mod syscall;
pub mod uring;

pub use dataplane_core_reactor::reactor_driver::{
    DriverBackendKind, DriverCapabilities, ReactorDriver, ReactorDriverWait,
};
pub use dataplane_core_reactor::reactor_model::{
    net, EventContext, FollowupAction, Handler, NetEvent, NetOp, NetOpKind, NetSubscription,
    NetSubscriptionEvent, OpToken, Operation, RawNetOp, ReactorClass, SubscriptionOp, SubscriptionToken,

    UdpRecvSlot,
};

pub trait Reactor {
    type Error;

    /// Submits an operation described by raw fds and pointers.
    ///
    /// # Safety
    /// The caller must keep every fd open and referring to the intended socket, and every
    /// referenced buffer valid at a stable address until terminal completion or confirmed
    /// backend teardown. Send buffers must remain readable and unmodified; receive buffers
    /// exclusively writable. UdpRecvBatch slots and all slot buffers must remain stable too.
    /// Cancellation alone does not discharge these obligations. See NetOp for the full
    /// lifetime and aliasing contract.
    unsafe fn submit(&mut self, op: NetOp) -> Result<OpToken, Self::Error>;
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
