use super::controller::{BalancedController, BalancedControllerStep, BalancedHostAction};
use super::BalancedHostPolicy;
use super::{BalancedParkSlots, BalancedTimerOwner};
use super::{ParkStoreOps, TimerStoreOps};
use crate::host_loop::HostLoop;
use crate::native_task::NativeTask;
use crate::reactor_driver::{ReactorDriver, ReactorDriverWait};
use crate::reactor_model::{OpToken, ReactorCompletion};
use alloc::vec::Vec;

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BalancedHostLoopAdapter<TimerStore = BalancedTimerOwner, ParkStore = BalancedParkSlots> {
    controller: BalancedController<TimerStore, ParkStore>,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BalancedHostLoopTick {
    pub controller_step: BalancedControllerStep,
    pub events: usize,
    pub tasks: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BalancedCompletionTick {
    pub controller_step: BalancedControllerStep,
    pub completions: Vec<ReactorCompletion>,
    pub tasks: usize,
}

impl<TimerStore, ParkStore> BalancedHostLoopAdapter<TimerStore, ParkStore>
where
    TimerStore: TimerStoreOps,
    ParkStore: ParkStoreOps,
{
    #[inline]
    pub fn new(controller: BalancedController<TimerStore, ParkStore>) -> Self {
        Self { controller }
    }

    #[inline]
    pub fn controller(&self) -> &BalancedController<TimerStore, ParkStore> {
        &self.controller
    }

    #[inline]
    pub fn controller_mut(&mut self) -> &mut BalancedController<TimerStore, ParkStore> {
        &mut self.controller
    }

    #[inline]
    pub fn tick<D, T, F, W>(
        &mut self,
        host: &mut HostLoop<D, T>,
        now_ns: u64,
        max_events: usize,
        task_budget: usize,
        on_event: F,
        mut wait_until: W,
    ) -> Result<BalancedHostLoopTick, <D as ReactorDriver>::Error>
    where
        D: ReactorDriver,
        T: NativeTask,
        F: FnMut(D::Event),
        W: FnMut(u64),
    {
        let controller_step =
            self.controller
                .step(now_ns, host.has_runtime_work(), host.has_task_work());

        let (events, tasks) = match controller_step.host_action {
            BalancedHostAction::Continue => host.tick(max_events, task_budget, on_event)?,
            BalancedHostAction::WaitUntil { deadline_ns } => {
                wait_until(deadline_ns);
                (0, 0)
            }
            BalancedHostAction::Idle => (0, 0),
        };

        Ok(BalancedHostLoopTick {
            controller_step,
            events,
            tasks,
        })
    }

    #[inline]
    pub fn tick_with_policy<D, T, F, P>(
        &mut self,
        host: &mut HostLoop<D, T>,
        now_ns: u64,
        max_events: usize,
        task_budget: usize,
        on_event: F,
        policy: &mut P,
    ) -> Result<BalancedHostLoopTick, <D as ReactorDriver>::Error>
    where
        D: ReactorDriver,
        T: NativeTask,
        F: FnMut(D::Event),
        P: BalancedHostPolicy,
    {
        self.tick(
            host,
            now_ns,
            max_events,
            task_budget,
            on_event,
            |deadline_ns| {
                policy.before_wait_until(deadline_ns);
            },
        )
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn tick_or_wait<D, T, F, W>(
        &mut self,
        host: &mut HostLoop<D, T>,
        now_ns: u64,
        max_events: usize,
        min_events: usize,
        task_budget: usize,
        on_event: F,
        mut before_wait_until: W,
    ) -> Result<BalancedHostLoopTick, <D as ReactorDriver>::Error>
    where
        D: ReactorDriver + ReactorDriverWait<Error = <D as ReactorDriver>::Error>,
        T: NativeTask,
        F: FnMut(D::Event),
        W: FnMut(u64),
    {
        let controller_step =
            self.controller
                .step(now_ns, host.has_runtime_work(), host.has_task_work());

        let (events, tasks) = match controller_step.host_action {
            BalancedHostAction::Continue => host.tick(max_events, task_budget, on_event)?,
            BalancedHostAction::WaitUntil { deadline_ns } => {
                before_wait_until(deadline_ns);
                host.tick_or_wait(max_events, min_events, task_budget, on_event)?
            }
            BalancedHostAction::Idle => (0, 0),
        };

        Ok(BalancedHostLoopTick {
            controller_step,
            events,
            tasks,
        })
    }

    #[inline]
    #[allow(clippy::too_many_arguments)]
    pub fn tick_or_wait_with_policy<D, T, F, P>(
        &mut self,
        host: &mut HostLoop<D, T>,
        now_ns: u64,
        max_events: usize,
        min_events: usize,
        task_budget: usize,
        on_event: F,
        policy: &mut P,
    ) -> Result<BalancedHostLoopTick, <D as ReactorDriver>::Error>
    where
        D: ReactorDriver + ReactorDriverWait<Error = <D as ReactorDriver>::Error>,
        T: NativeTask,
        F: FnMut(D::Event),
        P: BalancedHostPolicy,
    {
        self.tick_or_wait(
            host,
            now_ns,
            max_events,
            min_events,
            task_budget,
            on_event,
            |deadline_ns| policy.before_wait_until(deadline_ns),
        )
    }
}

impl<TimerStore, ParkStore> BalancedHostLoopAdapter<TimerStore, ParkStore>
where
    TimerStore: TimerStoreOps,
    ParkStore: ParkStoreOps,
{
    #[inline]
    pub fn tick_completions_or_wait_with_policy<D, T, P>(
        &mut self,
        host: &mut HostLoop<D, T>,
        now_ns: u64,
        max_events: usize,
        min_events: usize,
        task_budget: usize,
        policy: &mut P,
    ) -> Result<BalancedCompletionTick, <D as ReactorDriver>::Error>
    where
        D: ReactorDriver<Event = crate::reactor_model::NetEvent, Token = OpToken>
            + ReactorDriverWait<Error = <D as ReactorDriver>::Error>,
        T: NativeTask,
        P: BalancedHostPolicy,
    {
        let has_runtime_work = host.has_runtime_work();
        let has_task_work = host.has_task_work();
        let controller_step = self
            .controller
            .step(now_ns, has_runtime_work, has_task_work);

        let (completions, tasks) = match controller_step.host_action {
            BalancedHostAction::Continue if has_task_work => {
                host.tick_completions(max_events, task_budget)?
            }
            BalancedHostAction::Continue => {
                host.tick_completions_or_wait(max_events, min_events, task_budget)?
            }
            BalancedHostAction::WaitUntil { deadline_ns } => {
                policy.before_wait_until(deadline_ns);
                host.tick_completions_or_wait(max_events, min_events, task_budget)?
            }
            BalancedHostAction::Idle => (Vec::new(), 0),
        };

        Ok(BalancedCompletionTick {
            controller_step,
            completions,
            tasks,
        })
    }
}
