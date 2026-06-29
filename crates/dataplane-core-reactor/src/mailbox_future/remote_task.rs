use crate::mailbox_future::global_context::GlobalContext;
use crate::mailbox_future::result_cell::RemoteTaskResult;
use crate::mailbox_future::signal::SignalKey;
use crate::native_future_task::NativeFutureTask;
use std::future::Future;
use std::pin::Pin;
use std::sync::Arc;
use std::task::{Context, Poll, Waker};

pub type BoxedRuntimeFuture = Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

pub(crate) const REMOTE_DRAIN_BURST: usize = 64;

pub struct QueuedRuntimeFuture {
    future: BoxedRuntimeFuture,
}

impl QueuedRuntimeFuture {
    #[inline(always)]
    pub fn new(future: BoxedRuntimeFuture) -> Self {
        Self { future }
    }

    #[inline(always)]
    pub fn into_future(self) -> BoxedRuntimeFuture {
        self.future
    }
}

pub(crate) enum ShardMessage {
    Runtime(QueuedRuntimeFuture),
}

pub(crate) struct ShardRuntimeQueues {
    pub(crate) inbound: Vec<Option<rtrb::Consumer<ShardMessage>>>,
}

pub struct RemoteTaskReceiver<T: Send + 'static> {
    pub(crate) global: Arc<GlobalContext>,
    pub(crate) signal: Option<SignalKey>,
    pub(crate) result: Arc<RemoteTaskResult<T>>,
    pub(crate) owner_shard: Option<usize>,
    pub(crate) _marker: std::marker::PhantomData<T>,
}

pub struct RemoteValueSender<T: Send + 'static> {
    global: Arc<GlobalContext>,
    signal: Option<SignalKey>,
    result: Arc<RemoteTaskResult<T>>,
    owner_shard: Option<usize>,
    active: bool,
    _marker: std::marker::PhantomData<T>,
}

impl<T: Send + 'static> RemoteValueSender<T> {
    #[inline(always)]
    pub(crate) fn new(
        global: Arc<GlobalContext>,
        signal: Option<SignalKey>,
        result: Arc<RemoteTaskResult<T>>,
        owner_shard: Option<usize>,
    ) -> Self {
        Self {
            global,
            signal,
            result,
            owner_shard,
            active: true,
            _marker: std::marker::PhantomData,
        }
    }

    #[inline(always)]
    pub fn send(mut self, value: T) -> Result<(), T> {
        match self.result.complete(value) {
            Ok(()) => {
                self.finish_delivery();
                Ok(())
            }
            Err(value) => Err(value),
        }
    }

    #[inline(always)]
    fn finish_delivery(&mut self) {
        if !std::mem::replace(&mut self.active, false) {
            return;
        }
        if let (Some(signal), Some(owner_shard)) = (self.signal, self.owner_shard) {
            self.global.fire_signal(signal, owner_shard);
        }
    }
}

impl<T: Send + 'static> Drop for RemoteValueSender<T> {
    fn drop(&mut self) {
        if !self.active {
            return;
        }
        self.result.close();
        self.finish_delivery();
    }
}

pub type RuntimeFutureTask = NativeFutureTask<BoxedRuntimeFuture>;

#[inline(always)]
pub(crate) fn poll_boxed_runtime_future_once(future: &mut BoxedRuntimeFuture) -> Poll<()> {
    let waker: &Waker = Waker::noop();
    let mut cx = Context::from_waker(waker);
    future.as_mut().poll(&mut cx)
}
