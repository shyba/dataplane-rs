pub use dataplane_core_reactor::balanced_profile::{
    BalancedHostPolicy, BalancedProfileBudgets, BalancedProfileError, BalancedProfileLayout,
    BalancedRecordingHostPolicy, BalancedRuntime, BalancedShardRole, EmbeddedProfileLayout,
    PerformanceProfileLayout,
};
use dataplane_core_reactor::balanced_profile::{
    EmbeddedParkStore, EmbeddedTimerStore, PerformanceParkStore, PerformanceTimerStore,
};
use dataplane_core_reactor::native_task::NativeTask;
use dataplane_core_reactor::reactor_driver::ReactorDriver;
use dataplane_topology::{ProfileKind, TopologyProfile};

use super::profiled_runtime::ProfiledRuntime;

macro_rules! define_runtime_profile_surface {
    (
        layout_fn = $layout_fn:ident,
        layout_ty = $layout_ty:ty,
        layout_ctor = $layout_ctor:expr,
        build_fn = $build_fn:ident,
        build_with_policy_fn = $build_with_policy_fn:ident,
        default_runtime_ty = $default_runtime_ty:ty,
        policy_runtime_ty = $policy_runtime_ty:ty
    ) => {
        #[inline]
        pub fn $layout_fn() -> $layout_ty {
            $layout_ctor
        }

        #[inline]
        pub fn $build_fn<D, T>(driver: D, task_capacity: usize) -> $default_runtime_ty
        where
            D: ReactorDriver,
            T: NativeTask,
        {
            $layout_fn().build_runtime_with_task_capacity(driver, task_capacity)
        }

        #[inline]
        pub fn $build_with_policy_fn<D, T, P>(
            driver: D,
            task_capacity: usize,
            policy: P,
        ) -> $policy_runtime_ty
        where
            D: ReactorDriver,
            T: NativeTask,
            P: BalancedHostPolicy,
        {
            $layout_fn().build_runtime_with_policy(driver, task_capacity, policy)
        }
    };
}

define_runtime_profile_surface!(
    layout_fn = balanced_profile_layout,
    layout_ty = BalancedProfileLayout,
    layout_ctor = BalancedProfileLayout::reference(),
    build_fn = build_balanced_runtime,
    build_with_policy_fn = build_balanced_runtime_with_policy,
    default_runtime_ty = BalancedRuntime<D, T, BalancedRecordingHostPolicy>,
    policy_runtime_ty = BalancedRuntime<D, T, P>
);

define_runtime_profile_surface!(
    layout_fn = embedded_profile_layout,
    layout_ty = EmbeddedProfileLayout,
    layout_ctor = EmbeddedProfileLayout::reference(),
    build_fn = build_embedded_runtime,
    build_with_policy_fn = build_embedded_runtime_with_policy,
    default_runtime_ty =
        BalancedRuntime<D, T, BalancedRecordingHostPolicy, EmbeddedTimerStore, EmbeddedParkStore>,
    policy_runtime_ty = BalancedRuntime<D, T, P, EmbeddedTimerStore, EmbeddedParkStore>
);

define_runtime_profile_surface!(
    layout_fn = performance_profile_layout,
    layout_ty = PerformanceProfileLayout,
    layout_ctor = PerformanceProfileLayout::reference(),
    build_fn = build_performance_runtime,
    build_with_policy_fn = build_performance_runtime_with_policy,
    default_runtime_ty = BalancedRuntime<
        D,
        T,
        BalancedRecordingHostPolicy,
        PerformanceTimerStore,
        PerformanceParkStore,
    >,
    policy_runtime_ty = BalancedRuntime<D, T, P, PerformanceTimerStore, PerformanceParkStore>
);
pub fn build_profiled_runtime<D, T>(
    profile_kind: ProfileKind,
    driver: D,
    task_capacity: usize,
) -> Result<ProfiledRuntime<D, T>, BalancedProfileError>
where
    D: ReactorDriver,
    T: NativeTask,
{
    let profile = TopologyProfile::for_kind(profile_kind);
    build_profiled_runtime_from_profile(profile, driver, task_capacity)
}

#[inline]
pub fn build_profiled_runtime_with_policy<D, T, P>(
    profile_kind: ProfileKind,
    driver: D,
    task_capacity: usize,
    policy: P,
) -> Result<ProfiledRuntime<D, T, P>, BalancedProfileError>
where
    D: ReactorDriver,
    T: NativeTask,
    P: BalancedHostPolicy,
{
    let profile = TopologyProfile::for_kind(profile_kind);
    build_profiled_runtime_from_profile_with_policy(profile, driver, task_capacity, policy)
}

#[inline]
pub fn layout_for_profile(
    profile: TopologyProfile,
) -> Result<BalancedProfileLayout, BalancedProfileError> {
    dispatch_profile_layout_from_profile(
        profile,
        |layout| layout,
        |layout| layout.inner().clone(),
        |layout| layout.inner().clone(),
    )
}

#[inline]
pub fn dispatch_profile_layout_from_profile<R, FB, FE, FP>(
    profile: TopologyProfile,
    on_balanced: FB,
    on_embedded: FE,
    on_performance: FP,
) -> Result<R, BalancedProfileError>
where
    FB: FnOnce(BalancedProfileLayout) -> R,
    FE: FnOnce(EmbeddedProfileLayout) -> R,
    FP: FnOnce(PerformanceProfileLayout) -> R,
{
    match profile.profile_kind {
        ProfileKind::Balanced => BalancedProfileLayout::from_profile(
            profile,
            [BalancedShardRole::Control, BalancedShardRole::Worker],
            BalancedProfileBudgets::default(),
        )
        .map(on_balanced),
        ProfileKind::Embedded => EmbeddedProfileLayout::from_profile(profile).map(on_embedded),
        ProfileKind::Performance => {
            PerformanceProfileLayout::from_profile(profile).map(on_performance)
        }
    }
}

#[inline]
pub fn build_profiled_runtime_from_profile<D, T>(
    profile: TopologyProfile,
    driver: D,
    task_capacity: usize,
) -> Result<ProfiledRuntime<D, T>, BalancedProfileError>
where
    D: ReactorDriver,
    T: NativeTask,
{
    match profile.profile_kind {
        ProfileKind::Balanced => BalancedProfileLayout::from_profile(
            profile,
            [BalancedShardRole::Control, BalancedShardRole::Worker],
            BalancedProfileBudgets::default(),
        )
        .map(|layout| {
            ProfiledRuntime::Balanced(
                layout.build_runtime_with_task_capacity(driver, task_capacity),
            )
        }),
        ProfileKind::Embedded => EmbeddedProfileLayout::from_profile(profile).map(|layout| {
            ProfiledRuntime::Embedded(
                layout.build_runtime_with_task_capacity(driver, task_capacity),
            )
        }),
        ProfileKind::Performance => PerformanceProfileLayout::from_profile(profile).map(|layout| {
            ProfiledRuntime::Performance(
                layout.build_runtime_with_task_capacity(driver, task_capacity),
            )
        }),
    }
}

#[inline]
pub fn build_profiled_runtime_from_profile_with_policy<D, T, P>(
    profile: TopologyProfile,
    driver: D,
    task_capacity: usize,
    policy: P,
) -> Result<ProfiledRuntime<D, T, P>, BalancedProfileError>
where
    D: ReactorDriver,
    T: NativeTask,
    P: BalancedHostPolicy,
{
    match profile.profile_kind {
        ProfileKind::Balanced => BalancedProfileLayout::from_profile(
            profile,
            [BalancedShardRole::Control, BalancedShardRole::Worker],
            BalancedProfileBudgets::default(),
        )
        .map(|layout| {
            ProfiledRuntime::Balanced(layout.build_runtime_with_policy(
                driver,
                task_capacity,
                policy,
            ))
        }),
        ProfileKind::Embedded => EmbeddedProfileLayout::from_profile(profile).map(|layout| {
            ProfiledRuntime::Embedded(layout.build_runtime_with_policy(
                driver,
                task_capacity,
                policy,
            ))
        }),
        ProfileKind::Performance => PerformanceProfileLayout::from_profile(profile).map(|layout| {
            ProfiledRuntime::Performance(layout.build_runtime_with_policy(
                driver,
                task_capacity,
                policy,
            ))
        }),
    }
}
