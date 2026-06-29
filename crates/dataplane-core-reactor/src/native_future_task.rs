use crate::future_task::PinnedFuture;
use crate::native_task::{NativeTask, NativeTaskCx, StepResult};
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll, Waker};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PendingAction {
    Ready,
    Parked,
}

pub struct NativeFutureTask<F> {
    inner: PinnedFuture<F>,
    pending: PendingAction,
}

impl<F> NativeFutureTask<F>
where
    F: Future<Output = ()>,
{
    #[inline(always)]
    pub fn new(future: F, pending: PendingAction) -> Self {
        Self {
            inner: PinnedFuture::new(future),
            pending,
        }
    }

    #[inline(always)]
    pub fn ready(future: F) -> Self {
        Self::new(future, PendingAction::Ready)
    }

    #[inline(always)]
    pub fn parked(future: F) -> Self {
        Self::new(future, PendingAction::Parked)
    }

    #[inline(always)]
    pub fn future_pin(&mut self) -> Pin<&mut F> {
        self.inner.as_pin_mut()
    }

    #[inline(always)]
    pub fn poll_with_context(&mut self, cx: &mut Context<'_>) -> Poll<()> {
        self.inner.poll_with_context(cx)
    }

    #[inline(always)]
    pub fn poll_dummy(&mut self) -> Poll<()> {
        let waker: &Waker = Waker::noop();
        let mut cx = Context::from_waker(waker);
        self.poll_with_context(&mut cx)
    }
}

impl PendingAction {
    #[inline(always)]
    fn step_result(self) -> StepResult {
        match self {
            PendingAction::Ready => StepResult::Ready,
            PendingAction::Parked => StepResult::Parked,
        }
    }
}

impl<F> NativeTask for NativeFutureTask<F>
where
    F: Future<Output = ()>,
{
    #[inline(always)]
    fn step(&mut self, _cx: &mut NativeTaskCx<Self>) -> StepResult {
        match self.poll_dummy() {
            Poll::Ready(()) => StepResult::Complete,
            Poll::Pending => self.pending.step_result(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{NativeFutureTask, PendingAction};
    use crate::native_task::{NativeTask, NativeTaskCx, StepResult};
    use core::future::Future;
    use core::pin::Pin;
    use core::task::{Context, Poll};

    struct TwoPollFuture {
        ready: bool,
    }

    impl Future for TwoPollFuture {
        type Output = ();

        fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
            if self.ready {
                Poll::Ready(())
            } else {
                self.ready = true;
                Poll::Pending
            }
        }
    }

    #[test]
    fn ready_policy_requeues_pending_future() {
        let mut task = NativeFutureTask::new(TwoPollFuture { ready: false }, PendingAction::Ready);
        let mut cx = NativeTaskCx::new();
        assert_eq!(task.step(&mut cx), StepResult::Ready);
        assert_eq!(task.step(&mut cx), StepResult::Complete);
    }

    #[test]
    fn parked_policy_parks_pending_future() {
        let mut task = NativeFutureTask::new(TwoPollFuture { ready: false }, PendingAction::Parked);
        let mut cx = NativeTaskCx::new();
        assert_eq!(task.step(&mut cx), StepResult::Parked);
    }

    #[test]
    fn helper_constructors_preserve_pending_policy() {
        let mut ready = NativeFutureTask::ready(TwoPollFuture { ready: false });
        let mut parked = NativeFutureTask::parked(TwoPollFuture { ready: false });
        let mut cx = NativeTaskCx::new();

        assert_eq!(ready.step(&mut cx), StepResult::Ready);
        assert_eq!(parked.step(&mut cx), StepResult::Parked);
    }
}
