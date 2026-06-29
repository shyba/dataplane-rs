use dataplane_core_reactor::balanced_profile::{BalancedCompletionTick, BalancedHostPolicy};
use dataplane_core_reactor::native_task::NativeTask;
use dataplane_core_reactor::reactor_driver::{ReactorDriver, ReactorDriverWait};

use crate::embedded_host_loop::{EmbeddedDriveResult, EmbeddedHostAdapter};

use super::loop_core::EmbeddedHostLoop;

impl<D, T, P> EmbeddedHostLoop<D, T, P>
where
    D: ReactorDriver<
            Event = dataplane_core_reactor::reactor_model::NetEvent,
            Token = dataplane_core_reactor::reactor_model::OpToken,
        > + ReactorDriverWait<Error = <D as ReactorDriver>::Error>,
    T: NativeTask,
    P: BalancedHostPolicy,
{
    /// Run one bounded runtime progress step using host-provided monotonic time.
    ///
    /// Progress semantics:
    /// - `tick.tasks > 0` means one or more native tasks advanced during this step.
    /// - `tick.completions > 0` means one or more driver completions were observed.
    /// - A step may report both task and completion progress.
    /// - A step may report no progress (`tasks == 0 && completions == 0`); callers
    ///   decide whether to idle, wait, or immediately run another step.
    ///
    /// This API is host-neutral and non-blocking by default from runtime policy:
    /// it executes exactly one bounded tick and returns the observed progress.
    /// Any multi-step retry, backoff, or sleep policy remains caller-owned.
    ///
    /// Current driver-shape constraint:
    /// this method is only available when `D` uses
    /// `ReactorDriver<Event = NetEvent, Token = OpToken> + ReactorDriverWait`.
    /// That reflects the active M9 embedded host-loop completion path.
    #[inline]
    pub fn step(
        &mut self,
        now_ns: u64,
        max_events: usize,
        min_events: usize,
        task_budget: usize,
    ) -> Result<BalancedCompletionTick, <D as ReactorDriver>::Error> {
        self.handle
            .tick_completions_or_wait(now_ns, max_events, min_events, task_budget)
    }

    #[inline]
    /// Run one host-driven bounded runtime step.
    ///
    /// This method is host-neutral: the caller provides time through
    /// [`EmbeddedHostAdapter`] and owns any target-specific idle policy.
    ///
    /// Adapter ownership contract:
    /// - The caller retains ownership of the adapter for the full host loop.
    /// - This API only borrows `&mut host` for one bounded step.
    /// - The adapter is not stored inside [`EmbeddedHostLoop`] and no adapter
    ///   references escape this call boundary.
    /// - Adapter-owned state (clock source, idle counters, backoff hints) stays
    ///   target-side and can be reused across repeated `step_with_host(...)` calls.
    ///
    /// # Example
    ///
    /// ```rust
    /// use dataplane_core_reactor::native_task::{NativeTask, NativeTaskCx, StepResult};
    /// use dataplane_core_reactor::reactor_driver::{
    ///     DriverBackendKind, DriverCapabilities, ReactorDriver, ReactorDriverWait,
    /// };
    /// use dataplane_core_reactor::reactor_model::{NetEvent, OpToken};
    /// use dataplane_runtime::embedded_host_loop::{
    ///     EmbeddedHostAdapter, EmbeddedHostLoop, EmbeddedHostLoopConfig,
    /// };
    ///
    /// #[derive(Default)]
    /// struct DummyDriver;
    ///
    /// impl ReactorDriver for DummyDriver {
    ///     type Error = std::io::Error;
    ///     type Token = OpToken;
    ///     type Submit = ();
    ///     type Event = NetEvent;
    ///
    ///     fn submit(&mut self, _op: (), _token: OpToken) -> Result<(), Self::Error> {
    ///         Ok(())
    ///     }
    ///
    ///     fn flush(&mut self) -> Result<usize, Self::Error> {
    ///         Ok(0)
    ///     }
    ///
    ///     fn drain<F>(&mut self, _max_events: usize, _on_event: F) -> Result<usize, Self::Error>
    ///     where
    ///         F: FnMut(NetEvent),
    ///     {
    ///         Ok(0)
    ///     }
    ///
    ///     fn outstanding(&self) -> usize {
    ///         0
    ///     }
    ///
    ///     fn capabilities(&self) -> DriverCapabilities {
    ///         DriverCapabilities {
    ///             backend: DriverBackendKind::Syscall,
    ///             supports_accept_multi: false,
    ///             supports_multishot: false,
    ///             supports_fixed_buffers: false,
    ///             supports_sqpoll: false,
    ///         }
    ///     }
    /// }
    ///
    /// impl ReactorDriverWait for DummyDriver {
    ///     type Error = std::io::Error;
    ///     type Readiness = ();
    ///
    ///     fn readiness(&self) -> Option<Self::Readiness> {
    ///         None
    ///     }
    ///
    ///     fn wait(&mut self, _min_events: usize) -> Result<usize, Self::Error> {
    ///         Ok(0)
    ///     }
    /// }
    ///
    /// struct OneStepTask;
    ///
    /// impl NativeTask for OneStepTask {
    ///     fn step(&mut self, _cx: &mut NativeTaskCx<Self>) -> StepResult {
    ///         StepResult::Complete
    ///     }
    /// }
    ///
    /// struct Host {
    ///     now_ns: u64,
    /// }
    ///
    /// impl EmbeddedHostAdapter for Host {
    ///     fn now_ns(&mut self) -> u64 {
    ///         self.now_ns
    ///     }
    /// }
    ///
    /// let mut host_loop = EmbeddedHostLoop::<DummyDriver, OneStepTask>::new(
    ///     DummyDriver,
    ///     EmbeddedHostLoopConfig::default(),
    /// );
    /// host_loop.try_spawn(OneStepTask).expect("spawn");
    ///
    /// let mut host = Host { now_ns: 123 };
    /// let tick = host_loop.step_with_host(&mut host, 1, 0, 1).expect("step");
    /// assert_eq!(tick.tasks, 1);
    /// assert!(!host_loop.has_work());
    /// ```
    pub fn step_with_host<H>(
        &mut self,
        host: &mut H,
        max_events: usize,
        min_events: usize,
        task_budget: usize,
    ) -> Result<BalancedCompletionTick, <D as ReactorDriver>::Error>
    where
        H: EmbeddedHostAdapter,
    {
        let tick = self.step(host.now_ns(), max_events, min_events, task_budget)?;
        if tick.completions.is_empty() && tick.tasks == 0 {
            host.on_idle();
        }
        Ok(tick)
    }

    #[inline]
    /// Drive repeated bounded runtime steps while work remains and steps are available.
    ///
    /// By default this is non-blocking from the runtime side:
    /// each iteration runs one bounded step and returns as soon as either:
    /// - no work remains, or
    /// - `step_limit` is reached.
    ///
    /// The runtime never sleeps in this helper; any wait/backoff is host-owned via
    /// [`EmbeddedHostAdapter::on_idle`].
    ///
    /// # Example
    ///
    /// Drive a bounded multi-step task to completion with a host-owned adapter.
    ///
    /// ```rust
    /// use dataplane_core_reactor::native_task::{NativeTask, NativeTaskCx, StepResult};
    /// use dataplane_core_reactor::reactor_driver::{
    ///     DriverBackendKind, DriverCapabilities, ReactorDriver, ReactorDriverWait,
    /// };
    /// use dataplane_core_reactor::reactor_model::{NetEvent, OpToken};
    /// use dataplane_runtime::embedded_host_loop::{
    ///     EmbeddedHostAdapter, EmbeddedHostLoop, EmbeddedHostLoopConfig,
    /// };
    ///
    /// #[derive(Default)]
    /// struct DummyDriver;
    ///
    /// impl ReactorDriver for DummyDriver {
    ///     type Error = std::io::Error;
    ///     type Token = OpToken;
    ///     type Submit = ();
    ///     type Event = NetEvent;
    ///
    ///     fn submit(&mut self, _op: (), _token: OpToken) -> Result<(), Self::Error> {
    ///         Ok(())
    ///     }
    ///
    ///     fn flush(&mut self) -> Result<usize, Self::Error> {
    ///         Ok(0)
    ///     }
    ///
    ///     fn drain<F>(&mut self, _max_events: usize, _on_event: F) -> Result<usize, Self::Error>
    ///     where
    ///         F: FnMut(NetEvent),
    ///     {
    ///         Ok(0)
    ///     }
    ///
    ///     fn outstanding(&self) -> usize {
    ///         0
    ///     }
    ///
    ///     fn capabilities(&self) -> DriverCapabilities {
    ///         DriverCapabilities {
    ///             backend: DriverBackendKind::Syscall,
    ///             supports_accept_multi: false,
    ///             supports_multishot: false,
    ///             supports_fixed_buffers: false,
    ///             supports_sqpoll: false,
    ///         }
    ///     }
    /// }
    ///
    /// impl ReactorDriverWait for DummyDriver {
    ///     type Error = std::io::Error;
    ///     type Readiness = ();
    ///
    ///     fn readiness(&self) -> Option<Self::Readiness> {
    ///         None
    ///     }
    ///
    ///     fn wait(&mut self, _min_events: usize) -> Result<usize, Self::Error> {
    ///         Ok(0)
    ///     }
    /// }
    ///
    /// struct TwoStepTask {
    ///     remaining: u8,
    /// }
    ///
    /// impl NativeTask for TwoStepTask {
    ///     fn step(&mut self, _cx: &mut NativeTaskCx<Self>) -> StepResult {
    ///         if self.remaining == 0 {
    ///             return StepResult::Complete;
    ///         }
    ///         self.remaining -= 1;
    ///         if self.remaining == 0 {
    ///             StepResult::Complete
    ///         } else {
    ///             StepResult::Ready
    ///         }
    ///     }
    /// }
    ///
    /// struct Host {
    ///     now_ns: u64,
    /// }
    ///
    /// impl EmbeddedHostAdapter for Host {
    ///     fn now_ns(&mut self) -> u64 {
    ///         let now = self.now_ns;
    ///         self.now_ns += 1;
    ///         now
    ///     }
    /// }
    ///
    /// let mut host_loop = EmbeddedHostLoop::<DummyDriver, TwoStepTask>::new(
    ///     DummyDriver,
    ///     EmbeddedHostLoopConfig::default(),
    /// );
    /// host_loop
    ///     .try_spawn(TwoStepTask { remaining: 2 })
    ///     .expect("spawn");
    ///
    /// let mut host = Host { now_ns: 100 };
    /// let result = host_loop
    ///     .drive_steps_with_host(&mut host, 2, 1, 0, 1)
    ///     .expect("drive");
    ///
    /// assert_eq!(result.steps_attempted, 2);
    /// assert_eq!(result.tasks_run, 2);
    /// assert!(!result.work_remains);
    /// assert!(!host_loop.has_work());
    /// ```
    pub fn drive_steps_with_host<H>(
        &mut self,
        host: &mut H,
        step_limit: usize,
        max_events: usize,
        min_events: usize,
        task_budget: usize,
    ) -> Result<EmbeddedDriveResult, <D as ReactorDriver>::Error>
    where
        H: EmbeddedHostAdapter,
    {
        let mut result = EmbeddedDriveResult::default();
        let mut steps_remaining = step_limit;
        while steps_remaining > 0 && self.has_work() {
            let tick = self.step_with_host(host, max_events, min_events, task_budget)?;
            result.steps_attempted += 1;
            result.tasks_run += tick.tasks;
            result.completions_observed += tick.completions.len();
            steps_remaining -= 1;
        }
        result.work_remains = self.has_work();
        Ok(result)
    }

    #[inline]
    /// Drive using [`EmbeddedHostLoopConfig::drive`] defaults.
    ///
    /// This preserves the same host-neutral, non-blocking semantics as
    /// [`Self::drive_steps_with_host`].
    pub fn drive_steps_with_config_defaults<H>(
        &mut self,
        host: &mut H,
    ) -> Result<EmbeddedDriveResult, <D as ReactorDriver>::Error>
    where
        H: EmbeddedHostAdapter,
    {
        let drive = self.config.drive;
        self.drive_steps_with_host(
            host,
            drive.step_limit,
            drive.max_events,
            drive.min_events,
            drive.task_budget,
        )
    }
}
