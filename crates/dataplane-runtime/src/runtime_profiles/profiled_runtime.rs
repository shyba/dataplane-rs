pub use dataplane_core_reactor::balanced_profile::{
    BalancedHostPolicy, BalancedProfileLayout, BalancedRecordingHostPolicy, BalancedRuntime,
};
use dataplane_core_reactor::balanced_profile::{
    EmbeddedParkStore, EmbeddedTimerStore, PerformanceParkStore, PerformanceTimerStore,
};
use dataplane_core_reactor::host_loop::SubmissionFacade;
use dataplane_core_reactor::native_task::{NativeTask, NativeTaskCapacityError, TaskRef};
use dataplane_core_reactor::reactor_driver::{ReactorDriver, ReactorDriverWait};
use dataplane_core_reactor::reactor_model::{OpToken, ReactorCompletion};
use dataplane_core_reactor::wake_handle::WakeHandle;
use dataplane_topology::ProfileKind;

use super::loop_handle::RuntimeLoop;

pub enum ProfiledRuntime<D, T, P = BalancedRecordingHostPolicy>
where
    D: ReactorDriver,
    T: NativeTask,
    P: BalancedHostPolicy,
{
    Balanced(BalancedRuntime<D, T, P>),
    Embedded(BalancedRuntime<D, T, P, EmbeddedTimerStore, EmbeddedParkStore>),
    Performance(BalancedRuntime<D, T, P, PerformanceTimerStore, PerformanceParkStore>),
}

impl<D, T, P> ProfiledRuntime<D, T, P>
where
    D: ReactorDriver,
    T: NativeTask,
    P: BalancedHostPolicy,
{
    #[inline]
    pub fn profile_kind(&self) -> ProfileKind {
        self.layout().profile().profile_kind
    }

    #[inline]
    pub fn layout(&self) -> &BalancedProfileLayout {
        match self {
            Self::Balanced(runtime) => runtime.layout(),
            Self::Embedded(runtime) => runtime.layout(),
            Self::Performance(runtime) => runtime.layout(),
        }
    }

    #[inline]
    pub fn runtime(&self) -> &dataplane_core_reactor::reactor_runtime::ReactorRuntime<D> {
        match self {
            Self::Balanced(runtime) => runtime.runtime(),
            Self::Embedded(runtime) => runtime.runtime(),
            Self::Performance(runtime) => runtime.runtime(),
        }
    }

    #[inline]
    pub fn runtime_mut(
        &mut self,
    ) -> &mut dataplane_core_reactor::reactor_runtime::ReactorRuntime<D> {
        match self {
            Self::Balanced(runtime) => runtime.runtime_mut(),
            Self::Embedded(runtime) => runtime.runtime_mut(),
            Self::Performance(runtime) => runtime.runtime_mut(),
        }
    }

    #[inline]
    pub fn has_work(&self) -> bool {
        match self {
            Self::Balanced(runtime) => runtime.has_work(),
            Self::Embedded(runtime) => runtime.has_work(),
            Self::Performance(runtime) => runtime.has_work(),
        }
    }

    #[inline]
    pub fn has_runtime_work(&self) -> bool {
        match self {
            Self::Balanced(runtime) => runtime.has_runtime_work(),
            Self::Embedded(runtime) => runtime.has_runtime_work(),
            Self::Performance(runtime) => runtime.has_runtime_work(),
        }
    }

    /// Number of alive tasks (running, ready, or parked).
    #[inline]
    pub fn active_tasks(&self) -> usize {
        match self {
            Self::Balanced(runtime) => runtime.host().active_tasks(),
            Self::Embedded(runtime) => runtime.host().active_tasks(),
            Self::Performance(runtime) => runtime.host().active_tasks(),
        }
    }

    #[inline]
    pub fn has_task_work(&self) -> bool {
        match self {
            Self::Balanced(runtime) => runtime.has_task_work(),
            Self::Embedded(runtime) => runtime.has_task_work(),
            Self::Performance(runtime) => runtime.has_task_work(),
        }
    }

    #[inline]
    pub fn try_spawn(&mut self, task: T) -> Result<TaskRef, NativeTaskCapacityError<T>> {
        match self {
            Self::Balanced(runtime) => runtime.host_mut().try_spawn(task),
            Self::Embedded(runtime) => runtime.host_mut().try_spawn(task),
            Self::Performance(runtime) => runtime.host_mut().try_spawn(task),
        }
    }

    #[inline]
    pub fn submission(&mut self) -> SubmissionFacade<'_, D, T> {
        match self {
            Self::Balanced(runtime) => runtime.submission(),
            Self::Embedded(runtime) => runtime.submission(),
            Self::Performance(runtime) => runtime.submission(),
        }
    }

    #[inline]
    pub fn policy(&self) -> &P {
        match self {
            Self::Balanced(runtime) => runtime.policy(),
            Self::Embedded(runtime) => runtime.policy(),
            Self::Performance(runtime) => runtime.policy(),
        }
    }

    #[inline]
    pub fn policy_mut(&mut self) -> &mut P {
        match self {
            Self::Balanced(runtime) => runtime.policy_mut(),
            Self::Embedded(runtime) => runtime.policy_mut(),
            Self::Performance(runtime) => runtime.policy_mut(),
        }
    }

    #[inline]
    pub fn tick<F>(
        &mut self,
        now_ns: u64,
        max_events: usize,
        task_budget: usize,
        on_event: F,
    ) -> Result<
        dataplane_core_reactor::balanced_profile::BalancedHostLoopTick,
        <D as ReactorDriver>::Error,
    >
    where
        F: FnMut(D::Event),
    {
        match self {
            Self::Balanced(runtime) => runtime.tick(now_ns, max_events, task_budget, on_event),
            Self::Embedded(runtime) => runtime.tick(now_ns, max_events, task_budget, on_event),
            Self::Performance(runtime) => runtime.tick(now_ns, max_events, task_budget, on_event),
        }
    }

    #[inline]
    pub fn dispatch<R, FB, FE, FP>(self, on_balanced: FB, on_embedded: FE, on_performance: FP) -> R
    where
        FB: FnOnce(BalancedRuntime<D, T, P>) -> R,
        FE: FnOnce(BalancedRuntime<D, T, P, EmbeddedTimerStore, EmbeddedParkStore>) -> R,
        FP: FnOnce(BalancedRuntime<D, T, P, PerformanceTimerStore, PerformanceParkStore>) -> R,
    {
        match self {
            Self::Balanced(runtime) => on_balanced(runtime),
            Self::Embedded(runtime) => on_embedded(runtime),
            Self::Performance(runtime) => on_performance(runtime),
        }
    }
}

impl<D, T, P> ProfiledRuntime<D, T, P>
where
    D: ReactorDriver<Event = dataplane_core_reactor::reactor_model::NetEvent, Token = OpToken>,
    T: NativeTask,
    P: BalancedHostPolicy,
{
    #[inline]
    pub fn submit_and_flush_token(
        &mut self,
        op: D::Submit,
        wake: WakeHandle,
    ) -> Result<D::Token, D::Error> {
        match self {
            Self::Balanced(runtime) => runtime
                .submit_and_flush(op, wake)
                .map(|handle| handle.token()),
            Self::Embedded(runtime) => runtime
                .submit_and_flush(op, wake)
                .map(|handle| handle.token()),
            Self::Performance(runtime) => runtime
                .submit_and_flush(op, wake)
                .map(|handle| handle.token()),
        }
    }

    #[inline]
    pub fn submit_if_idle(
        &mut self,
        inflight: &mut Option<D::Token>,
        op: D::Submit,
        wake: WakeHandle,
    ) -> Result<bool, D::Error> {
        match self {
            Self::Balanced(runtime) => runtime.submit_if_idle(inflight, op, wake),
            Self::Embedded(runtime) => runtime.submit_if_idle(inflight, op, wake),
            Self::Performance(runtime) => runtime.submit_if_idle(inflight, op, wake),
        }
    }
}

impl<D, T, P> ProfiledRuntime<D, T, P>
where
    D: ReactorDriver + ReactorDriverWait<Error = <D as ReactorDriver>::Error>,
    T: NativeTask,
    P: BalancedHostPolicy,
{
    #[inline]
    pub fn tick_or_wait<F>(
        &mut self,
        now_ns: u64,
        max_events: usize,
        min_events: usize,
        task_budget: usize,
        on_event: F,
    ) -> Result<
        dataplane_core_reactor::balanced_profile::BalancedHostLoopTick,
        <D as ReactorDriver>::Error,
    >
    where
        F: FnMut(D::Event),
    {
        match self {
            Self::Balanced(runtime) => {
                runtime.tick_or_wait(now_ns, max_events, min_events, task_budget, on_event)
            }
            Self::Embedded(runtime) => {
                runtime.tick_or_wait(now_ns, max_events, min_events, task_budget, on_event)
            }
            Self::Performance(runtime) => {
                runtime.tick_or_wait(now_ns, max_events, min_events, task_budget, on_event)
            }
        }
    }
}

impl<D, T, P> ProfiledRuntime<D, T, P>
where
    D: ReactorDriver<Event = dataplane_core_reactor::reactor_model::NetEvent, Token = OpToken>
        + ReactorDriverWait<Error = <D as ReactorDriver>::Error>,
    T: NativeTask,
    P: BalancedHostPolicy,
{
    /// Non-blocking completions tick: drain ready completions (routing any
    /// registered task wakes) and step ready tasks.
    #[inline]
    pub fn tick_completions(
        &mut self,
        max_events: usize,
        task_budget: usize,
    ) -> Result<
        (
            Vec<ReactorCompletion>,
            usize,
        ),
        <D as ReactorDriver>::Error,
    > {
        match self {
            Self::Balanced(runtime) => runtime.host_mut().tick_completions(max_events, task_budget),
            Self::Embedded(runtime) => runtime.host_mut().tick_completions(max_events, task_budget),
            Self::Performance(runtime) => {
                runtime.host_mut().tick_completions(max_events, task_budget)
            }
        }
    }

    #[inline]
    pub fn poll(
        &mut self,
        wait: bool,
        max_events: usize,
    ) -> Result<Vec<ReactorCompletion>, <D as ReactorDriver>::Error> {
        match self {
            Self::Balanced(runtime) => runtime.poll(wait, max_events),
            Self::Embedded(runtime) => runtime.poll(wait, max_events),
            Self::Performance(runtime) => runtime.poll(wait, max_events),
        }
    }

    #[inline]
    pub fn tick_completions_or_wait(
        &mut self,
        now_ns: u64,
        max_events: usize,
        min_events: usize,
        task_budget: usize,
    ) -> Result<
        dataplane_core_reactor::balanced_profile::BalancedCompletionTick,
        <D as ReactorDriver>::Error,
    > {
        match self {
            Self::Balanced(runtime) => {
                runtime.tick_completions_or_wait(now_ns, max_events, min_events, task_budget)
            }
            Self::Embedded(runtime) => {
                runtime.tick_completions_or_wait(now_ns, max_events, min_events, task_budget)
            }
            Self::Performance(runtime) => {
                runtime.tick_completions_or_wait(now_ns, max_events, min_events, task_budget)
            }
        }
    }
}

impl<D, T, P> RuntimeLoop for ProfiledRuntime<D, T, P>
where
    D: ReactorDriver<Event = dataplane_core_reactor::reactor_model::NetEvent, Token = OpToken>
        + ReactorDriverWait<Error = <D as ReactorDriver>::Error>,
    T: NativeTask,
    P: BalancedHostPolicy,
{
    type Error = <D as ReactorDriver>::Error;
    type Submit = D::Submit;
    type Token = D::Token;

    #[inline]
    fn tick_completions_or_wait(
        &mut self,
        now_ns: u64,
        max_events: usize,
        min_events: usize,
        task_budget: usize,
    ) -> Result<
        dataplane_core_reactor::balanced_profile::BalancedCompletionTick,
        Self::Error,
    > {
        ProfiledRuntime::tick_completions_or_wait(self, now_ns, max_events, min_events, task_budget)
    }

    #[inline]
    fn submit_if_idle(
        &mut self,
        inflight: &mut Option<Self::Token>,
        op: Self::Submit,
        wake: WakeHandle,
    ) -> Result<bool, Self::Error> {
        ProfiledRuntime::submit_if_idle(self, inflight, op, wake)
    }

    #[inline]
    fn submit_and_flush_token(
        &mut self,
        op: Self::Submit,
        wake: WakeHandle,
    ) -> Result<Self::Token, Self::Error> {
        ProfiledRuntime::submit_and_flush_token(self, op, wake)
    }

    #[inline]
    fn poll(
        &mut self,
        wait: bool,
        max_events: usize,
    ) -> Result<Vec<ReactorCompletion>, Self::Error> {
        ProfiledRuntime::poll(self, wait, max_events)
    }
}
