use crate::reactor_driver::ReactorDriver;
use crate::reactor_model::{OpToken, ReactorCompletion};
use crate::submission_handle::SubmissionHandle;
use crate::wake_handle::WakeHandle;

use super::HostLoop;
use crate::native_task::NativeTask;

pub struct SubmissionFacade<'a, D, T>
where
    D: ReactorDriver,
    T: NativeTask,
{
    pub(crate) host: &'a mut HostLoop<D, T>,
}

impl<'a, D, T> SubmissionFacade<'a, D, T>
where
    D: ReactorDriver<Event = crate::reactor_model::NetEvent, Token = OpToken>,
    T: NativeTask,
{
    #[inline(always)]
    pub fn submit(
        &mut self,
        op: D::Submit,
        wake: WakeHandle,
    ) -> Result<SubmissionHandle<D::Token>, D::Error> {
        self.host.submit_with_generated_token(op, wake)
    }

    #[inline(always)]
    pub fn submit_with_wake(
        &mut self,
        op: D::Submit,
        token: D::Token,
        wake: WakeHandle,
    ) -> Result<(), D::Error> {
        self.host.submit_with_wake(op, token, wake)
    }

    #[inline(always)]
    pub fn submit_with_handle(
        &mut self,
        op: D::Submit,
        token: D::Token,
        wake: WakeHandle,
    ) -> Result<SubmissionHandle<D::Token>, D::Error> {
        self.host.submit_with_handle(op, token, wake)
    }

    #[inline(always)]
    pub fn submit_with_generated_token(
        &mut self,
        op: D::Submit,
        wake: WakeHandle,
    ) -> Result<SubmissionHandle<D::Token>, D::Error> {
        self.host.submit_with_generated_token(op, wake)
    }

    #[inline(always)]
    pub fn drain_completions_into<F>(
        &mut self,
        max_events: usize,
        on_completion: F,
    ) -> Result<usize, D::Error>
    where
        F: FnMut(ReactorCompletion, SubmissionHandle<D::Token>),
    {
        self.host.drain_completions_into(max_events, on_completion)
    }

    #[inline(always)]
    pub fn poll_completions_into<F>(
        &mut self,
        max_events: usize,
        on_completion: F,
    ) -> Result<usize, D::Error>
    where
        F: FnMut(ReactorCompletion, SubmissionHandle<D::Token>),
    {
        self.host.drain_completions_into(max_events, on_completion)
    }

    #[inline(always)]
    pub fn tick_routed<F>(
        &mut self,
        max_events: usize,
        task_budget: usize,
        on_completion: F,
    ) -> Result<(usize, usize), D::Error>
    where
        F: FnMut(ReactorCompletion, SubmissionHandle<D::Token>),
    {
        self.host
            .tick_routed(max_events, task_budget, on_completion)
    }
}
