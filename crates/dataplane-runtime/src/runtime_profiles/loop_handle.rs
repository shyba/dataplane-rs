pub use dataplane_core_reactor::balanced_profile::{
    BalancedCompletionTick, BalancedHostPolicy, BalancedParkLease, BalancedRuntime,
    BalancedWakeToken, EmbeddedResourceError,
};
use dataplane_core_reactor::balanced_profile::{EmbeddedParkStore, EmbeddedTimerStore};
use dataplane_core_reactor::native_task::{NativeTask, NativeTaskCapacityError, TaskRef};
use dataplane_core_reactor::reactor_driver::{ReactorDriver, ReactorDriverWait};
use dataplane_core_reactor::reactor_model::{OpToken, ReactorCompletion};
use dataplane_core_reactor::wake_handle::WakeHandle;

pub trait RuntimeLoop {
    type Error;
    type Submit;
    type Token;

    fn tick_completions_or_wait(
        &mut self,
        now_ns: u64,
        max_events: usize,
        min_events: usize,
        task_budget: usize,
    ) -> Result<BalancedCompletionTick, Self::Error>;

    fn submit_if_idle(
        &mut self,
        inflight: &mut Option<Self::Token>,
        op: Self::Submit,
        wake: WakeHandle,
    ) -> Result<bool, Self::Error>;

    fn submit_and_flush_token(
        &mut self,
        op: Self::Submit,
        wake: WakeHandle,
    ) -> Result<Self::Token, Self::Error>;

    fn poll(
        &mut self,
        wait: bool,
        max_events: usize,
    ) -> Result<Vec<ReactorCompletion>, Self::Error>;
}

/// Typed wrapper over a [`RuntimeLoop`] implementation.
///
/// This generic handle forwards only trait-level operations and `R::Error`.
/// Embedded-only bounded-resource errors are intentionally exposed only on
/// embedded-typed wrapper surfaces.
///
/// ```compile_fail
/// use dataplane_core_reactor::balanced_profile::BalancedWakeToken;
/// use dataplane_runtime::runtime_profiles::RuntimeLoopHandle;
///
/// fn generic_handle_cannot_call_embedded_helper<R>(handle: &mut RuntimeLoopHandle<R>) {
///     let _ = handle.try_arm_embedded_park(BalancedWakeToken(1));
/// }
/// ```
pub struct RuntimeLoopHandle<R> {
    pub(super) inner: R,
}

impl<R> RuntimeLoopHandle<R> {
    #[inline]
    pub fn new(inner: R) -> Self {
        Self { inner }
    }

    #[inline]
    pub fn into_inner(self) -> R {
        self.inner
    }
}

impl<R> RuntimeLoopHandle<R>
where
    R: RuntimeLoop,
{
    #[inline]
    pub fn tick_completions_or_wait(
        &mut self,
        now_ns: u64,
        max_events: usize,
        min_events: usize,
        task_budget: usize,
    ) -> Result<BalancedCompletionTick, R::Error> {
        self.inner
            .tick_completions_or_wait(now_ns, max_events, min_events, task_budget)
    }

    #[inline]
    pub fn submit_if_idle(
        &mut self,
        inflight: &mut Option<R::Token>,
        op: R::Submit,
        wake: WakeHandle,
    ) -> Result<bool, R::Error> {
        self.inner.submit_if_idle(inflight, op, wake)
    }

    #[inline]
    pub fn submit_and_flush_token(
        &mut self,
        op: R::Submit,
        wake: WakeHandle,
    ) -> Result<R::Token, R::Error> {
        self.inner.submit_and_flush_token(op, wake)
    }

    #[inline]
    pub fn poll(
        &mut self,
        wait: bool,
        max_events: usize,
    ) -> Result<Vec<ReactorCompletion>, R::Error> {
        self.inner.poll(wait, max_events)
    }
}

/// Embedded-profile-only wrapper extensions that intentionally expose
/// embedded bounded-resource errors.
///
/// Why these are typed-surface only:
/// - Embedded helpers return embedded-only bounded-resource error types
///   (`NativeTaskCapacityError<T>` and `EmbeddedResourceError`) that do not
///   belong on the generic `RuntimeLoop::Error` transport channel.
/// - Availability is tied to the embedded store shape
///   (`EmbeddedTimerStore` + `EmbeddedParkStore`), so callers get these APIs
///   only when the runtime type itself proves embedded semantics.
/// - This keeps generic runtime-callers profile-agnostic while still giving
///   embedded callers explicit, typed bounded-failure controls.
///
/// Naming contract for these embedded-only helpers:
/// - `has_work` and `try_spawn` match the existing host-loop surface naming.
/// - `try_arm_embedded_park` keeps the embedded qualifier because park-slot
///   exhaustion is an embedded bounded-resource path.
/// - additional split helpers (for example `has_task_work` or
///   `has_runtime_work`) are intentionally deferred until a concrete caller
///   requires that distinction.
impl<D, T, P> RuntimeLoopHandle<BalancedRuntime<D, T, P, EmbeddedTimerStore, EmbeddedParkStore>>
where
    D: ReactorDriver,
    T: NativeTask,
    P: BalancedHostPolicy,
{
    #[inline]
    pub fn has_work(&self) -> bool {
        self.inner.has_work()
    }

    #[inline]
    pub fn try_spawn(&mut self, task: T) -> Result<TaskRef, NativeTaskCapacityError<T>> {
        self.inner.host_mut().try_spawn(task)
    }

    #[inline]
    pub fn try_arm_embedded_park(
        &mut self,
        wake_token: BalancedWakeToken,
    ) -> Result<BalancedParkLease, EmbeddedResourceError> {
        self.inner.try_arm_park(wake_token)
    }
}

impl<D, T, P, TimerStore, ParkStore> RuntimeLoop for BalancedRuntime<D, T, P, TimerStore, ParkStore>
where
    D: ReactorDriver<Event = dataplane_core_reactor::reactor_model::NetEvent, Token = OpToken>
        + ReactorDriverWait<Error = <D as ReactorDriver>::Error>,
    T: NativeTask,
    P: BalancedHostPolicy,
    TimerStore: dataplane_core_reactor::balanced_profile::TimerStoreOps,
    ParkStore: dataplane_core_reactor::balanced_profile::ParkStoreOps,
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
    ) -> Result<BalancedCompletionTick, Self::Error> {
        BalancedRuntime::tick_completions_or_wait(self, now_ns, max_events, min_events, task_budget)
    }

    #[inline]
    fn submit_if_idle(
        &mut self,
        inflight: &mut Option<Self::Token>,
        op: Self::Submit,
        wake: WakeHandle,
    ) -> Result<bool, Self::Error> {
        BalancedRuntime::submit_if_idle(self, inflight, op, wake)
    }

    #[inline]
    fn submit_and_flush_token(
        &mut self,
        op: Self::Submit,
        wake: WakeHandle,
    ) -> Result<Self::Token, Self::Error> {
        BalancedRuntime::submit_and_flush(self, op, wake).map(|handle| handle.token())
    }

    #[inline]
    fn poll(
        &mut self,
        wait: bool,
        max_events: usize,
    ) -> Result<Vec<ReactorCompletion>, Self::Error> {
        BalancedRuntime::poll(self, wait, max_events)
    }
}
