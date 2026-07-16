//! Embedded future adapter with an explicit `TaskHeader` state machine and
//! `WaitTag` integration, consumed by bare-metal targets (RP2040 smoke
//! firmware via `dataplane_runtime::rp2040`). Host runtimes use
//! `native_future_task::NativeFutureTask` instead — see that module for the
//! waker invariant shared by both adapters.
use crate::wait_tag::{LocalWaitKind, WaitTag};
use alloc::boxed::Box;
use core::future::Future;
use core::pin::Pin;
use core::sync::atomic::{AtomicU32, Ordering};
use core::task::{Context, Poll, Waker};

pub const TASK_STATE_RUNNABLE: u32 = 1 << 0;
pub const TASK_STATE_RUNNING: u32 = 1 << 1;
pub const TASK_STATE_COMPLETE: u32 = 1 << 2;

pub struct PinnedFuture<F>(Pin<Box<F>>);

impl<F> PinnedFuture<F>
where
    F: Future,
{
    #[inline(always)]
    pub fn new(future: F) -> Self {
        Self(Box::pin(future))
    }

    #[inline(always)]
    pub fn as_pin_mut(&mut self) -> Pin<&mut F> {
        self.0.as_mut()
    }

    #[inline(always)]
    pub fn poll_with_context(&mut self, cx: &mut Context<'_>) -> Poll<F::Output> {
        self.as_pin_mut().poll(cx)
    }
}

#[derive(Debug)]
pub struct TaskHeader {
    state: AtomicU32,
    wait: WaitTag,
}

impl TaskHeader {
    #[inline(always)]
    pub fn new(wait: WaitTag) -> Self {
        Self {
            state: AtomicU32::new(TASK_STATE_RUNNABLE),
            wait,
        }
    }

    #[inline(always)]
    pub fn wait(&self) -> WaitTag {
        self.wait
    }

    #[inline(always)]
    pub fn state(&self) -> u32 {
        self.state.load(Ordering::Acquire)
    }

    #[inline(always)]
    pub fn store_state(&self, state: u32) {
        self.state.store(state, Ordering::Release);
    }

    #[inline(always)]
    pub fn begin_run(&self) {
        self.store_state(TASK_STATE_RUNNABLE | TASK_STATE_RUNNING);
    }

    #[inline(always)]
    pub fn mark_pending(&self) {
        self.store_state(TASK_STATE_RUNNABLE);
    }

    #[inline(always)]
    pub fn mark_complete(&self) {
        self.store_state(TASK_STATE_COMPLETE);
    }
}

pub struct FutureTask<F> {
    header: TaskHeader,
    future: PinnedFuture<F>,
}

impl<F> FutureTask<F>
where
    F: Future,
{
    #[inline(always)]
    pub fn new_local(future: F) -> Self {
        Self::new_with_wait(future, WaitTag::new_local(LocalWaitKind::Runnable, 0))
    }

    #[inline(always)]
    pub fn new_with_wait(future: F, wait: WaitTag) -> Self {
        Self {
            header: TaskHeader::new(wait),
            future: PinnedFuture::new(future),
        }
    }

    #[inline(always)]
    pub fn header(&self) -> &TaskHeader {
        &self.header
    }

    #[inline(always)]
    pub fn future_pin(&mut self) -> Pin<&mut F> {
        self.future.as_pin_mut()
    }

    #[inline(always)]
    pub fn poll_dummy(&mut self) -> Poll<F::Output> {
        let waker: &Waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        self.poll_with_context(&mut cx)
    }

    #[inline(always)]
    pub fn poll_with_context(&mut self, cx: &mut Context<'_>) -> Poll<F::Output> {
        self.header.begin_run();
        let poll = self.future.poll_with_context(cx);
        match poll {
            Poll::Pending => {
                self.header.mark_pending();
                Poll::Pending
            }
            Poll::Ready(output) => {
                self.header.mark_complete();
                Poll::Ready(output)
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{FutureTask, PinnedFuture, TaskHeader, TASK_STATE_COMPLETE, TASK_STATE_RUNNABLE};
    use crate::wait_tag::{LocalWaitKind, WaitRef, WaitTag};
    use core::future::Future;
    use core::pin::Pin;
    use core::task::{Context, Poll};

    struct TwoPollFuture {
        ready: bool,
    }

    impl Future for TwoPollFuture {
        type Output = u32;

        fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
            if self.ready {
                Poll::Ready(7)
            } else {
                self.ready = true;
                Poll::Pending
            }
        }
    }

    #[test]
    fn header_keeps_wait_tag() {
        let header = TaskHeader::new(WaitTag::new_local(LocalWaitKind::Io, 9));
        assert_eq!(
            header.wait().decode(),
            Ok(WaitRef::Local {
                kind: LocalWaitKind::Io,
                payload: 9,
            })
        );
    }

    #[test]
    fn dummy_poll_drives_compiler_future() {
        let mut task = FutureTask::new_local(async { 11u32 });
        assert_eq!(task.poll_dummy(), Poll::Ready(11));
        assert_eq!(task.header().state(), TASK_STATE_COMPLETE);
    }

    #[test]
    fn dummy_poll_drives_manual_future() {
        let mut task = FutureTask::new_local(TwoPollFuture { ready: false });
        assert_eq!(task.poll_dummy(), Poll::Pending);
        assert_eq!(task.header().state(), TASK_STATE_RUNNABLE);
        assert_eq!(task.poll_dummy(), Poll::Ready(7));
        assert_eq!(task.header().state(), TASK_STATE_COMPLETE);
    }

    #[test]
    fn header_state_transition_helpers_store_expected_bits() {
        let header = TaskHeader::new(WaitTag::new_local(LocalWaitKind::Runnable, 0));

        header.begin_run();
        assert_eq!(
            header.state(),
            TASK_STATE_RUNNABLE | super::TASK_STATE_RUNNING
        );

        header.mark_pending();
        assert_eq!(header.state(), TASK_STATE_RUNNABLE);

        header.mark_complete();
        assert_eq!(header.state(), TASK_STATE_COMPLETE);
    }

    #[test]
    fn pinned_future_wrapper_preserves_manual_future_progress() {
        let mut future = PinnedFuture::new(TwoPollFuture { ready: false });
        let waker = core::task::Waker::noop();
        let mut cx = Context::from_waker(waker);

        assert_eq!(future.poll_with_context(&mut cx), Poll::Pending);
        assert_eq!(future.poll_with_context(&mut cx), Poll::Ready(7));
    }
}
