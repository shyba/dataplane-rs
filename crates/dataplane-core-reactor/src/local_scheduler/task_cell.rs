use quanta::Clock;
use std::future::Future;
use std::pin::Pin;
use std::task::{Context, Poll, RawWaker, RawWakerVTable, Waker};

#[cfg(feature = "trace-stamps")]
use super::trace::TraceRing;
use super::trace::{TraceStamp, TraceStep, TRACE_STAMP_DEPTH};
use super::types::TaskMeta;

pub(crate) type AsyncTaskFuture<const STACK_BYTES: usize> =
    Pin<Box<dyn Future<Output = ()> + Send + 'static>>;

pub enum TaskPayload<Op, const STACK_BYTES: usize> {
    Work(Op),
    Future(AsyncTaskFuture<STACK_BYTES>),
}

pub struct TaskCell<Op, const STACK_BYTES: usize> {
    pub meta: TaskMeta,
    pub payload: TaskPayload<Op, STACK_BYTES>,
    #[cfg(feature = "trace-stamps")]
    trace: TraceRing,
}

impl<Op, const STACK_BYTES: usize> TaskCell<Op, STACK_BYTES> {
    #[inline(always)]
    pub fn new_work(meta: TaskMeta, op: Op) -> Self {
        Self {
            meta,
            payload: TaskPayload::Work(op),
            #[cfg(feature = "trace-stamps")]
            trace: TraceRing::default(),
        }
    }

    #[inline(always)]
    pub fn new_future(meta: TaskMeta, future: AsyncTaskFuture<STACK_BYTES>) -> Self {
        Self {
            meta,
            payload: TaskPayload::Future(future),
            #[cfg(feature = "trace-stamps")]
            trace: TraceRing::default(),
        }
    }

    #[inline(always)]
    pub fn from_future<F>(meta: TaskMeta, future: F) -> Self
    where
        F: Future<Output = ()> + Send + 'static,
    {
        Self::new_future(meta, Box::pin(future))
    }

    #[inline(always)]
    pub(crate) fn stamp(&mut self, clock: &Clock, step: TraceStep) {
        #[cfg(feature = "trace-stamps")]
        {
            self.trace.push(step, clock.raw());
        }
        #[cfg(not(feature = "trace-stamps"))]
        {
            let _ = (clock, step);
        }
    }

    #[inline(always)]
    pub fn trace_snapshot(&self) -> Option<[TraceStamp; TRACE_STAMP_DEPTH]> {
        #[cfg(feature = "trace-stamps")]
        {
            Some(self.trace.snapshot())
        }
        #[cfg(not(feature = "trace-stamps"))]
        {
            None
        }
    }
}

pub(crate) fn poll_async_future<const STACK_BYTES: usize>(
    future: &mut AsyncTaskFuture<STACK_BYTES>,
) -> Poll<()> {
    let waker = noop_waker();
    let mut cx = Context::from_waker(&waker);
    // SAFETY: this path only receives futures stored as `Pin<Box<...>>` in task cells,
    // so the pointee is already pinned with a stable address for poll.
    unsafe { Pin::new_unchecked(future) }.poll(&mut cx)
}

fn noop_waker() -> Waker {
    // SAFETY: the vtable functions never dereference or free the data pointer,
    // so a null data pointer is valid for this stateless noop waker.
    unsafe { Waker::from_raw(RawWaker::new(std::ptr::null(), &NOOP_WAKER_VTABLE)) }
}

unsafe fn noop_clone(_: *const ()) -> RawWaker {
    // SAFETY: this function is only ever called as part of a RawWaker vtable
    // invocation. It must preserve the invariant that the returned RawWaker uses
    // the same stateless null data pointer and the same NOOP_WAKER_VTABLE vtable.
    // No memory is accessed and no ownership is transferred.
    RawWaker::new(std::ptr::null(), &NOOP_WAKER_VTABLE)
}

unsafe fn noop_wake(_: *const ()) {
    // SAFETY: this function is only ever called as part of a RawWaker wake
    // invocation. It is a noop because the waker has no associated state or payload.
    // No memory is accessed and no ownership is transferred.
}

unsafe fn noop_drop(_: *const ()) {
    // SAFETY: this function is only ever called when a RawWaker is dropped.
    // Since the waker data pointer is null and owns no resources, no cleanup is needed.
    // No memory is accessed and no ownership is transferred.
}

static NOOP_WAKER_VTABLE: RawWakerVTable =
    RawWakerVTable::new(noop_clone, noop_wake, noop_wake, noop_drop);
