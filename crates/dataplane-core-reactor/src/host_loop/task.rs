use crate::inflight_table::InflightTable;
use crate::native_task::{NativeTask, NativeTaskCapacityError, NativeTaskEngine, TaskRef};
use crate::reactor_driver::{ReactorDriver, ReactorDriverWait};
use crate::reactor_model::{OpToken, ReactorCompletion};
use crate::reactor_runtime::ReactorRuntime;
use crate::submission_handle::SubmissionHandle;
use crate::wake_handle::WakeHandle;

use super::completion::CompletionDrain;
use super::SubmissionFacade;

pub struct HostLoop<D, T>
where
    D: ReactorDriver,
    T: NativeTask,
{
    runtime: ReactorRuntime<D>,
    tasks: NativeTaskEngine<T>,
    pub(crate) inflight: InflightTable,
}

impl<D, T> HostLoop<D, T>
where
    D: ReactorDriver,
    T: NativeTask,
{
    #[inline(always)]
    pub fn new(runtime: ReactorRuntime<D>, tasks: NativeTaskEngine<T>) -> Self {
        Self {
            runtime,
            tasks,
            inflight: InflightTable::with_capacity(64),
        }
    }

    #[inline(always)]
    pub fn runtime(&self) -> &ReactorRuntime<D> {
        &self.runtime
    }

    #[inline(always)]
    pub fn runtime_mut(&mut self) -> &mut ReactorRuntime<D> {
        &mut self.runtime
    }

    #[inline(always)]
    pub fn tasks(&self) -> &NativeTaskEngine<T> {
        &self.tasks
    }

    #[inline(always)]
    pub fn tasks_mut(&mut self) -> &mut NativeTaskEngine<T> {
        &mut self.tasks
    }

    #[inline(always)]
    pub fn spawn(&mut self, task: T) -> TaskRef {
        self.tasks.spawn(task)
    }

    #[inline(always)]
    pub fn try_spawn(&mut self, task: T) -> Result<TaskRef, NativeTaskCapacityError<T>> {
        self.tasks.try_spawn(task)
    }

    #[inline(always)]
    pub fn into_parts(self) -> (ReactorRuntime<D>, NativeTaskEngine<T>) {
        (self.runtime, self.tasks)
    }

    #[inline(always)]
    pub fn submission(&mut self) -> SubmissionFacade<'_, D, T> {
        SubmissionFacade { host: self }
    }

    #[inline(always)]
    pub fn tick<F>(
        &mut self,
        max_events: usize,
        task_budget: usize,
        on_event: F,
    ) -> Result<(usize, usize), D::Error>
    where
        F: FnMut(D::Event),
    {
        self.runtime.flush()?;
        let events = self.runtime.drain(max_events, on_event)?;
        let tasks = self.tasks.run_budget(task_budget);
        Ok((events, tasks))
    }

    #[inline(always)]
    pub fn has_work(&self) -> bool {
        self.runtime.outstanding() != 0 || self.tasks.active_tasks() != 0
    }

    #[inline(always)]
    pub fn has_runtime_work(&self) -> bool {
        self.runtime.outstanding() != 0
    }

    #[inline(always)]
    pub fn has_task_work(&self) -> bool {
        self.tasks.active_tasks() != 0
    }
}

impl<D, T> HostLoop<D, T>
where
    D: ReactorDriver<Event = crate::reactor_model::NetEvent, Token = OpToken>,
    T: NativeTask,
{
    #[inline(always)]
    pub fn tick_completions(
        &mut self,
        max_events: usize,
        task_budget: usize,
    ) -> Result<(Vec<ReactorCompletion>, usize), D::Error> {
        self.runtime.flush()?;
        let mut completions = Vec::new();
        self.drain_completions(max_events, |completion, _wake| completions.push(completion))?;
        let tasks = self.tasks.run_budget(task_budget);
        Ok((completions, tasks))
    }

    #[inline(always)]
    pub fn submit_and_flush(
        &mut self,
        op: D::Submit,
        wake: WakeHandle,
    ) -> Result<SubmissionHandle<D::Token>, D::Error> {
        let handle = self.submit_with_generated_token(op, wake)?;
        let submitted = self.runtime.flush()?;
        assert!(submitted > 0, "submit_and_flush returned 0 for active op");
        Ok(handle)
    }

    #[inline(always)]
    pub fn submit_if_idle(
        &mut self,
        inflight: &mut Option<D::Token>,
        op: D::Submit,
        wake: WakeHandle,
    ) -> Result<bool, D::Error> {
        if inflight.is_some() {
            return Ok(false);
        }
        let handle = self.submit_and_flush(op, wake)?;
        *inflight = Some(handle.token());
        Ok(true)
    }

    #[inline(always)]
    pub fn submit_with_wake(
        &mut self,
        op: D::Submit,
        token: D::Token,
        wake: WakeHandle,
    ) -> Result<(), D::Error> {
        assert!(
            self.inflight.insert_exact(token, wake),
            "submit_with_wake requires a free inflight token"
        );
        if let Err(err) = self.runtime.submit(op, token) {
            let _ = self.inflight.remove(token);
            return Err(err);
        }
        Ok(())
    }

    #[inline(always)]
    pub fn submit_with_handle(
        &mut self,
        op: D::Submit,
        token: D::Token,
        wake: WakeHandle,
    ) -> Result<SubmissionHandle<D::Token>, D::Error> {
        self.submit_with_wake(op, token, wake)?;
        Ok(SubmissionHandle::new(token, wake))
    }

    #[inline(always)]
    pub fn submit_with_generated_token(
        &mut self,
        op: D::Submit,
        wake: WakeHandle,
    ) -> Result<SubmissionHandle<D::Token>, D::Error> {
        let token = self.inflight.alloc(wake);
        if let Err(err) = self.runtime.submit(op, token) {
            let _ = self.inflight.remove(token);
            return Err(err);
        }
        Ok(SubmissionHandle::new(token, wake))
    }

    #[inline(always)]
    pub fn drain_completions<F>(
        &mut self,
        max_events: usize,
        mut on_completion: F,
    ) -> Result<usize, D::Error>
    where
        F: FnMut(ReactorCompletion, Option<WakeHandle>),
    {
        self.runtime.drain(max_events, |event| {
            let completion = ReactorCompletion::from(event);
            let wake = self.inflight.remove(completion.token);
            on_completion(completion, wake);
        })
    }

    #[inline(always)]
    pub fn drain_completions_into<F>(
        &mut self,
        max_events: usize,
        mut on_completion: F,
    ) -> Result<usize, D::Error>
    where
        F: FnMut(ReactorCompletion, SubmissionHandle<D::Token>),
    {
        self.runtime.drain(max_events, |event| {
            let completion = ReactorCompletion::from(event);
            if let Some(wake) = self.inflight.remove(completion.token) {
                on_completion(completion, SubmissionHandle::new(completion.token, wake));
            }
        })
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
        self.runtime.flush()?;
        let events = self.drain_completions_into(max_events, on_completion)?;
        let tasks = self.tasks.run_budget(task_budget);
        Ok((events, tasks))
    }

    #[inline(always)]
    pub fn poll_now(&mut self, max_events: usize) -> Result<Vec<ReactorCompletion>, D::Error> {
        let mut out = Vec::new();
        self.drain_completions(max_events, |completion, _wake| out.push(completion))?;
        Ok(out)
    }
}

impl<D, T> HostLoop<D, T>
where
    D: ReactorDriver<Event = crate::reactor_model::NetEvent, Token = OpToken>
        + ReactorDriverWait<Error = <D as ReactorDriver>::Error>,
    T: NativeTask,
{
    #[inline(always)]
    pub fn poll(
        &mut self,
        wait: bool,
        max_events: usize,
    ) -> Result<Vec<ReactorCompletion>, <D as ReactorDriver>::Error> {
        if !wait {
            return self.poll_now(max_events);
        }
        let out = self.drain_completion_batch_or_wait(max_events, 1)?;
        if out.is_empty() {
            return Ok(Vec::new());
        }
        Ok(out)
    }

    #[inline(always)]
    pub fn tick_completions_or_wait(
        &mut self,
        max_events: usize,
        min_events: usize,
        task_budget: usize,
    ) -> Result<(Vec<ReactorCompletion>, usize), <D as ReactorDriver>::Error> {
        self.runtime.flush()?;
        let completions = self.drain_completion_batch_or_wait(max_events, min_events)?;
        let tasks = self.tasks.run_budget(task_budget);
        Ok((completions, tasks))
    }

    #[inline(always)]
    fn drain_completion_batch_or_wait(
        &mut self,
        max_events: usize,
        min_events: usize,
    ) -> Result<Vec<ReactorCompletion>, <D as ReactorDriver>::Error> {
        self.runtime
            .drain_completion_batch_or_wait(&mut self.inflight, max_events, min_events)
    }
}

impl<D, T> HostLoop<D, T>
where
    D: ReactorDriver + ReactorDriverWait<Error = <D as ReactorDriver>::Error>,
    T: NativeTask,
{
    #[inline(always)]
    pub fn tick_or_wait<F>(
        &mut self,
        max_events: usize,
        min_events: usize,
        task_budget: usize,
        on_event: F,
    ) -> Result<(usize, usize), <D as ReactorDriver>::Error>
    where
        F: FnMut(D::Event),
    {
        self.runtime.flush()?;
        let events = self
            .runtime
            .drain_or_wait(max_events, min_events, on_event)?;
        let tasks = self.tasks.run_budget(task_budget);
        Ok((events, tasks))
    }
}
