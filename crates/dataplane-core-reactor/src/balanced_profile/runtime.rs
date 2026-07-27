use super::controller::EmbeddedResourceError;
use super::host_adapter::{
    BalancedCompletionTick, BalancedHostLoopAdapter, BalancedHostLoopTick, RuntimeStats,
};
use super::park::{
    BalancedParkLease, BalancedParkSlots, BalancedWakeToken, EmbeddedParkStore, ParkStoreOps,
};
use super::timer::{BalancedTimerOwner, EmbeddedTimerStore, TimerStoreOps};
use super::{BalancedHostPolicy, BalancedProfileLayout, BalancedRecordingHostPolicy};
use crate::host_loop::{HostLoop, SubmissionFacade};
use crate::native_task::NativeTask;
use crate::reactor_driver::{ReactorDriver, ReactorDriverWait};
use crate::reactor_model::{OpToken, ReactorCompletion};
use crate::submission_handle::SubmissionHandle;
use crate::wake_handle::WakeHandle;
use alloc::vec::Vec;

pub struct BalancedRuntime<
    D,
    T,
    P = BalancedRecordingHostPolicy,
    TimerStore = BalancedTimerOwner,
    ParkStore = BalancedParkSlots,
> where
    D: ReactorDriver,
    T: NativeTask,
    P: BalancedHostPolicy,
    TimerStore: TimerStoreOps,
    ParkStore: ParkStoreOps,
{
    layout: BalancedProfileLayout,
    host: HostLoop<D, T>,
    adapter: BalancedHostLoopAdapter<TimerStore, ParkStore>,
    policy: P,
    stats: RuntimeStats,
}

impl<D, T, P, TimerStore, ParkStore> BalancedRuntime<D, T, P, TimerStore, ParkStore>
where
    D: ReactorDriver,
    T: NativeTask,
    P: BalancedHostPolicy,
    TimerStore: TimerStoreOps,
    ParkStore: ParkStoreOps,
{
    #[inline]
    pub fn new(
        layout: BalancedProfileLayout,
        host: HostLoop<D, T>,
        adapter: BalancedHostLoopAdapter<TimerStore, ParkStore>,
        policy: P,
    ) -> Self {
        Self {
            adapter,
            layout,
            host,
            policy,
            stats: RuntimeStats::default(),
        }
    }

    #[inline]
    pub fn layout(&self) -> &BalancedProfileLayout {
        &self.layout
    }

    #[inline]
    pub fn runtime(&self) -> &crate::reactor_runtime::ReactorRuntime<D> {
        self.host.runtime()
    }

    #[inline]
    pub fn runtime_mut(&mut self) -> &mut crate::reactor_runtime::ReactorRuntime<D> {
        self.host.runtime_mut()
    }

    #[inline]
    pub fn host(&self) -> &HostLoop<D, T> {
        &self.host
    }

    pub fn host_mut(&mut self) -> &mut HostLoop<D, T> {
        &mut self.host
    }

    #[inline]
    pub fn has_work(&self) -> bool {
        self.host.has_work()
    }

    #[inline]
    pub fn has_runtime_work(&self) -> bool {
        self.host.has_runtime_work()
    }

    #[inline]
    pub fn has_task_work(&self) -> bool {
        self.host.has_task_work()
    }

    #[inline]
    pub fn submission(&mut self) -> SubmissionFacade<'_, D, T> {
        self.host.submission()
    }

    #[inline]
    pub fn adapter(&self) -> &BalancedHostLoopAdapter<TimerStore, ParkStore> {
        &self.adapter
    }

    #[inline]
    pub fn adapter_mut(&mut self) -> &mut BalancedHostLoopAdapter<TimerStore, ParkStore> {
        &mut self.adapter
    }

    #[inline]
    pub fn policy(&self) -> &P {
        &self.policy
    }

    #[inline]
    pub fn policy_mut(&mut self) -> &mut P {
        &mut self.policy
    }

    #[inline]
    pub fn tick<F>(
        &mut self,
        now_ns: u64,
        max_events: usize,
        task_budget: usize,
        on_event: F,
    ) -> Result<BalancedHostLoopTick, <D as ReactorDriver>::Error>
    where
        F: FnMut(D::Event),
    {
        let tick = self.adapter.tick_with_policy(
            &mut self.host,
            now_ns,
            max_events,
            task_budget,
            on_event,
            &mut self.policy,
        )?;
        self.stats
            .fold_tick(&tick.controller_step, tick.events, tick.tasks);
        Ok(tick)
    }

    /// Cumulative telemetry for this runtime's controller-driven ticks, with live
    /// gauges (active tasks, clock regressions, dropped tasks) captured now. Cheap
    /// struct copy; aggregate across shard runtimes by summing snapshots.
    #[inline]
    pub fn stats(&self) -> RuntimeStats {
        let mut snap = self.stats;
        snap.active_tasks = self.host.active_tasks() as u64;
        snap.now_regressions = self.adapter.controller().now_regressions();
        snap.tasks_dropped = self.host.tasks().children_dropped();
        snap
    }
}

impl<D, T, P> BalancedRuntime<D, T, P, EmbeddedTimerStore, EmbeddedParkStore>
where
    D: ReactorDriver,
    T: NativeTask,
    P: BalancedHostPolicy,
{
    #[inline]
    pub fn try_arm_park(
        &mut self,
        wake_token: BalancedWakeToken,
    ) -> Result<BalancedParkLease, EmbeddedResourceError> {
        self.adapter.controller_mut().try_arm_park(wake_token)
    }
}

impl<D, T, P, TimerStore, ParkStore> BalancedRuntime<D, T, P, TimerStore, ParkStore>
where
    D: ReactorDriver<Event = crate::reactor_model::NetEvent, Token = OpToken>,
    T: NativeTask,
    P: BalancedHostPolicy,
    TimerStore: TimerStoreOps,
    ParkStore: ParkStoreOps,
{
    #[inline]
    pub fn submit_and_flush(
        &mut self,
        op: D::Submit,
        wake: WakeHandle,
    ) -> Result<SubmissionHandle<D::Token>, D::Error> {
        self.host.submit_and_flush(op, wake)
    }

    #[inline]
    pub fn submit_if_idle(
        &mut self,
        inflight: &mut Option<D::Token>,
        op: D::Submit,
        wake: WakeHandle,
    ) -> Result<bool, D::Error> {
        self.host.submit_if_idle(inflight, op, wake)
    }
}

impl<D, T, P, TimerStore, ParkStore> BalancedRuntime<D, T, P, TimerStore, ParkStore>
where
    D: ReactorDriver + ReactorDriverWait<Error = <D as ReactorDriver>::Error>,
    T: NativeTask,
    P: BalancedHostPolicy,
    TimerStore: TimerStoreOps,
    ParkStore: ParkStoreOps,
{
    #[inline]
    pub fn tick_or_wait<F>(
        &mut self,
        now_ns: u64,
        max_events: usize,
        min_events: usize,
        task_budget: usize,
        on_event: F,
    ) -> Result<BalancedHostLoopTick, <D as ReactorDriver>::Error>
    where
        F: FnMut(D::Event),
    {
        let tick = self.adapter.tick_or_wait_with_policy(
            &mut self.host,
            now_ns,
            max_events,
            min_events,
            task_budget,
            on_event,
            &mut self.policy,
        )?;
        self.stats
            .fold_tick(&tick.controller_step, tick.events, tick.tasks);
        Ok(tick)
    }
}

impl<D, T, P, TimerStore, ParkStore> BalancedRuntime<D, T, P, TimerStore, ParkStore>
where
    D: ReactorDriver<Event = crate::reactor_model::NetEvent, Token = OpToken>
        + ReactorDriverWait<Error = <D as ReactorDriver>::Error>,
    T: NativeTask,
    P: BalancedHostPolicy,
    TimerStore: TimerStoreOps,
    ParkStore: ParkStoreOps,
{
    #[inline]
    pub fn poll(
        &mut self,
        wait: bool,
        max_events: usize,
    ) -> Result<Vec<ReactorCompletion>, <D as ReactorDriver>::Error> {
        self.host.poll(wait, max_events)
    }

    #[inline]
    pub fn tick_completions_or_wait(
        &mut self,
        now_ns: u64,
        max_events: usize,
        min_events: usize,
        task_budget: usize,
    ) -> Result<BalancedCompletionTick, <D as ReactorDriver>::Error> {
        let tick = self.adapter.tick_completions_or_wait_with_policy(
            &mut self.host,
            now_ns,
            max_events,
            min_events,
            task_budget,
            &mut self.policy,
        )?;
        self.stats
            .fold_tick(&tick.controller_step, tick.completions.len(), tick.tasks);
        Ok(tick)
    }
}
