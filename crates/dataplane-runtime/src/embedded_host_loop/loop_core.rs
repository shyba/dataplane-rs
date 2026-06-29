use dataplane_core_reactor::balanced_profile::{
    BalancedHostPolicy, BalancedParkLease, BalancedRecordingHostPolicy, BalancedRuntime,
    BalancedWakeToken, EmbeddedParkStore, EmbeddedProfilePolicy, EmbeddedResourceError,
    EmbeddedTimerStore,
};
use dataplane_core_reactor::native_task::{NativeTask, NativeTaskCapacityError, TaskRef};
use dataplane_core_reactor::reactor_driver::ReactorDriver;

use crate::runtime_profiles::{embedded_profile_layout, RuntimeLoopHandle};

use super::{EmbeddedHostLoopConfig, EmbeddedPolicySummary};

pub struct EmbeddedHostLoop<D, T, P = BalancedRecordingHostPolicy>
where
    D: ReactorDriver,
    T: NativeTask,
    P: BalancedHostPolicy,
{
    pub(super) handle:
        RuntimeLoopHandle<BalancedRuntime<D, T, P, EmbeddedTimerStore, EmbeddedParkStore>>,
    pub(super) config: EmbeddedHostLoopConfig,
}

impl<D, T> EmbeddedHostLoop<D, T, BalancedRecordingHostPolicy>
where
    D: ReactorDriver,
    T: NativeTask,
{
    /// Construct an embedded host loop using the default recording host policy.
    ///
    /// # Example
    ///
    /// ```rust
    /// use dataplane_core_reactor::native_task::{NativeTask, NativeTaskCx, StepResult};
    /// use dataplane_core_reactor::reactor_driver::{
    ///     DriverBackendKind, DriverCapabilities, ReactorDriver,
    /// };
    /// use dataplane_runtime::embedded_host_loop::{EmbeddedHostLoop, EmbeddedHostLoopConfig};
    ///
    /// struct DummyDriver;
    ///
    /// impl ReactorDriver for DummyDriver {
    ///     type Error = core::convert::Infallible;
    ///     type Token = u64;
    ///     type Submit = ();
    ///     type Event = ();
    ///
    ///     fn submit(&mut self, _op: Self::Submit, _token: Self::Token) -> Result<(), Self::Error> {
    ///         Ok(())
    ///     }
    ///
    ///     fn flush(&mut self) -> Result<usize, Self::Error> {
    ///         Ok(0)
    ///     }
    ///
    ///     fn drain<F>(&mut self, _max_events: usize, _on_event: F) -> Result<usize, Self::Error>
    ///     where
    ///         F: FnMut(Self::Event),
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
    /// struct DummyTask;
    ///
    /// impl NativeTask for DummyTask {
    ///     fn step(&mut self, _cx: &mut NativeTaskCx<Self>) -> StepResult {
    ///         StepResult::Complete
    ///     }
    /// }
    ///
    /// let host_loop = EmbeddedHostLoop::<DummyDriver, DummyTask>::new(
    ///     DummyDriver,
    ///     EmbeddedHostLoopConfig::default(),
    /// );
    /// assert_eq!(host_loop.task_capacity(), EmbeddedHostLoopConfig::default().task_capacity);
    /// ```
    #[inline]
    pub fn new(driver: D, config: EmbeddedHostLoopConfig) -> Self {
        Self::with_policy(driver, config, BalancedRecordingHostPolicy::default())
    }
}

impl<D, T, P> EmbeddedHostLoop<D, T, P>
where
    D: ReactorDriver,
    T: NativeTask,
    P: BalancedHostPolicy,
{
    #[inline]
    pub fn with_policy(driver: D, config: EmbeddedHostLoopConfig, policy: P) -> Self {
        let runtime = embedded_profile_layout().build_runtime_with_policy(
            driver,
            config.task_capacity,
            policy,
        );
        Self {
            handle: RuntimeLoopHandle::new(runtime),
            config,
        }
    }

    #[inline]
    pub fn embedded_policy(&self) -> EmbeddedProfilePolicy {
        embedded_profile_layout().policy()
    }

    #[inline]
    pub fn embedded_policy_summary(&self) -> EmbeddedPolicySummary {
        EmbeddedPolicySummary::from_policy(self.task_capacity(), self.embedded_policy())
    }

    #[inline]
    pub fn handle(
        &self,
    ) -> &RuntimeLoopHandle<BalancedRuntime<D, T, P, EmbeddedTimerStore, EmbeddedParkStore>> {
        &self.handle
    }

    #[inline]
    pub fn handle_mut(
        &mut self,
    ) -> &mut RuntimeLoopHandle<BalancedRuntime<D, T, P, EmbeddedTimerStore, EmbeddedParkStore>>
    {
        &mut self.handle
    }

    #[inline]
    pub fn into_handle(
        self,
    ) -> RuntimeLoopHandle<BalancedRuntime<D, T, P, EmbeddedTimerStore, EmbeddedParkStore>> {
        self.handle
    }

    #[inline]
    pub fn config(&self) -> EmbeddedHostLoopConfig {
        self.config
    }

    #[inline]
    pub const fn task_capacity(&self) -> usize {
        self.config.task_capacity()
    }

    #[inline]
    pub fn has_work(&self) -> bool {
        self.handle.has_work()
    }

    #[inline]
    pub fn try_spawn(&mut self, task: T) -> Result<TaskRef, NativeTaskCapacityError<T>> {
        self.handle.try_spawn(task)
    }

    #[inline]
    pub fn try_arm_embedded_park(
        &mut self,
        wake_token: BalancedWakeToken,
    ) -> Result<BalancedParkLease, EmbeddedResourceError> {
        // DP-EMB-0028 boundary decision: keep timer-capacity propagation narrow.
        // The active 038 plan defers any host-loop timer wrapper until a real
        // caller requires a typed timer-allocation path.
        self.handle.try_arm_embedded_park(wake_token)
    }
}
