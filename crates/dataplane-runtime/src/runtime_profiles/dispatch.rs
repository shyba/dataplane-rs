pub use dataplane_core_reactor::balanced_profile::{
    BalancedHostPolicy, BalancedProfileError, BalancedRecordingHostPolicy, BalancedRuntime,
};
use dataplane_core_reactor::balanced_profile::{
    EmbeddedParkStore, EmbeddedTimerStore, PerformanceParkStore, PerformanceTimerStore,
};
use dataplane_core_reactor::native_task::NativeTask;
use dataplane_core_reactor::reactor_driver::ReactorDriver;
use dataplane_topology::TopologyProfile;

use super::builders::build_profiled_runtime_from_profile_with_policy;
use super::loop_handle::RuntimeLoopHandle;

pub fn dispatch_profiled_runtime_from_profile_with_policy<D, T, P, R, FB, FE, FP>(
    profile: TopologyProfile,
    driver: D,
    task_capacity: usize,
    policy: P,
    on_balanced: FB,
    on_embedded: FE,
    on_performance: FP,
) -> Result<R, BalancedProfileError>
where
    D: ReactorDriver,
    T: NativeTask,
    P: BalancedHostPolicy,
    FB: FnOnce(BalancedRuntime<D, T, P>) -> R,
    FE: FnOnce(BalancedRuntime<D, T, P, EmbeddedTimerStore, EmbeddedParkStore>) -> R,
    FP: FnOnce(BalancedRuntime<D, T, P, PerformanceTimerStore, PerformanceParkStore>) -> R,
{
    build_profiled_runtime_from_profile_with_policy(profile, driver, task_capacity, policy)
        .map(|runtime| runtime.dispatch(on_balanced, on_embedded, on_performance))
}

#[inline]
pub fn dispatch_profiled_runtime_loop_from_profile<D, T, R, FB, FE, FP>(
    profile: TopologyProfile,
    driver: D,
    task_capacity: usize,
    on_balanced: FB,
    on_embedded: FE,
    on_performance: FP,
) -> Result<R, BalancedProfileError>
where
    D: ReactorDriver,
    T: NativeTask,
    FB: FnOnce(RuntimeLoopHandle<BalancedRuntime<D, T, BalancedRecordingHostPolicy>>) -> R,
    FE: FnOnce(
        RuntimeLoopHandle<
            BalancedRuntime<
                D,
                T,
                BalancedRecordingHostPolicy,
                EmbeddedTimerStore,
                EmbeddedParkStore,
            >,
        >,
    ) -> R,
    FP: FnOnce(
        RuntimeLoopHandle<
            BalancedRuntime<
                D,
                T,
                BalancedRecordingHostPolicy,
                PerformanceTimerStore,
                PerformanceParkStore,
            >,
        >,
    ) -> R,
{
    dispatch_profiled_runtime_loop_from_profile_with_policy(
        profile,
        driver,
        task_capacity,
        BalancedRecordingHostPolicy::default(),
        on_balanced,
        on_embedded,
        on_performance,
    )
}

#[inline]
pub fn dispatch_profiled_runtime_loop_from_profile_with_policy<D, T, P, R, FB, FE, FP>(
    profile: TopologyProfile,
    driver: D,
    task_capacity: usize,
    policy: P,
    on_balanced: FB,
    on_embedded: FE,
    on_performance: FP,
) -> Result<R, BalancedProfileError>
where
    D: ReactorDriver,
    T: NativeTask,
    P: BalancedHostPolicy,
    FB: FnOnce(RuntimeLoopHandle<BalancedRuntime<D, T, P>>) -> R,
    FE: FnOnce(
        RuntimeLoopHandle<BalancedRuntime<D, T, P, EmbeddedTimerStore, EmbeddedParkStore>>,
    ) -> R,
    FP: FnOnce(
        RuntimeLoopHandle<BalancedRuntime<D, T, P, PerformanceTimerStore, PerformanceParkStore>>,
    ) -> R,
{
    dispatch_profiled_runtime_from_profile_with_policy(
        profile,
        driver,
        task_capacity,
        policy,
        |runtime| on_balanced(RuntimeLoopHandle::new(runtime)),
        |runtime| on_embedded(RuntimeLoopHandle::new(runtime)),
        |runtime| on_performance(RuntimeLoopHandle::new(runtime)),
    )
}
