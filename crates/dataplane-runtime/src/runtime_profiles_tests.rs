use super::{
    balanced_profile_layout, build_balanced_runtime, build_embedded_runtime,
    build_embedded_runtime_with_policy, build_performance_runtime, build_profiled_runtime,
    build_profiled_runtime_from_profile, build_profiled_runtime_from_profile_with_policy,
    build_profiled_runtime_with_policy, dispatch_profile_layout_from_profile,
    dispatch_profiled_runtime_from_profile_with_policy, embedded_profile_layout,
    layout_for_profile, performance_profile_layout, BalancedRecordingHostPolicy,
    EmbeddedResourceError, ProfileKind, ProfiledRuntime, RuntimeLoopHandle, TopologyProfile,
};
use dataplane_core_reactor::balanced_profile::{ParkStoreOps, TimerStoreOps};
use dataplane_core_reactor::native_task::{NativeTask, NativeTaskCx, StepResult};
use dataplane_core_reactor::reactor_driver::{
    DriverBackendKind, DriverCapabilities, ReactorDriver, ReactorDriverWait,
};
use dataplane_core_reactor::reactor_model::OpToken;

#[derive(Default)]
struct DummyDriver {
    outstanding: usize,
    wait_calls: Vec<usize>,
}

struct CountTask {
    remaining: usize,
}

impl ReactorDriver for DummyDriver {
    type Error = std::io::Error;
    type Token = OpToken;
    type Submit = ();
    type Event = ();

    fn submit(&mut self, _op: Self::Submit, _token: Self::Token) -> Result<(), Self::Error> {
        self.outstanding = self.outstanding.saturating_add(1);
        Ok(())
    }

    fn flush(&mut self) -> Result<usize, Self::Error> {
        Ok(self.outstanding)
    }

    fn drain<F>(&mut self, max: usize, mut on_event: F) -> Result<usize, Self::Error>
    where
        F: FnMut(Self::Event),
    {
        let drained = self.outstanding.min(max);
        for _ in 0..drained {
            on_event(());
        }
        self.outstanding -= drained;
        Ok(drained)
    }

    fn capabilities(&self) -> DriverCapabilities {
        DriverCapabilities {
            backend: DriverBackendKind::Syscall,
            supports_accept_multi: false,
            supports_multishot: false,
            supports_fixed_buffers: false,
            supports_sqpoll: false,
        }
    }

    fn outstanding(&self) -> usize {
        self.outstanding
    }
}

impl ReactorDriverWait for DummyDriver {
    type Error = std::io::Error;
    type Readiness = ();

    fn readiness(&self) -> Option<Self::Readiness> {
        if self.outstanding > 0 {
            Some(())
        } else {
            None
        }
    }

    fn wait(&mut self, min_events: usize) -> Result<usize, Self::Error> {
        self.wait_calls.push(min_events);
        self.outstanding = 0;
        Ok(1)
    }
}

impl NativeTask for CountTask {
    fn step(&mut self, _cx: &mut NativeTaskCx<Self>) -> StepResult {
        if self.remaining == 0 {
            StepResult::Complete
        } else {
            self.remaining -= 1;
            if self.remaining == 0 {
                StepResult::Complete
            } else {
                StepResult::Ready
            }
        }
    }
}

#[test]
fn family_profile_layout_helpers_expose_expected_profile_kinds() {
    assert_eq!(
        balanced_profile_layout().profile().profile_kind,
        dataplane_topology::ProfileKind::Balanced
    );
    assert_eq!(
        embedded_profile_layout().profile().profile_kind,
        dataplane_topology::ProfileKind::Embedded
    );
    assert_eq!(
        performance_profile_layout().profile().profile_kind,
        dataplane_topology::ProfileKind::Performance
    );
}

#[test]
fn runtime_builders_preserve_family_store_selection() {
    let balanced = build_balanced_runtime::<DummyDriver, CountTask>(DummyDriver::default(), 4);
    let embedded = build_embedded_runtime::<DummyDriver, CountTask>(DummyDriver::default(), 4);
    let performance =
        build_performance_runtime::<DummyDriver, CountTask>(DummyDriver::default(), 4);

    assert_eq!(
        balanced.layout().profile().profile_kind,
        dataplane_topology::ProfileKind::Balanced
    );
    assert_eq!(
        embedded.layout().profile().profile_kind,
        dataplane_topology::ProfileKind::Embedded
    );
    assert_eq!(
        performance.layout().profile().profile_kind,
        dataplane_topology::ProfileKind::Performance
    );

    assert_eq!(
        embedded.adapter().controller().timer_store().wake_batch(),
        16
    );
    assert_eq!(
        embedded.adapter().controller().park_store().slot_count(),
        64
    );
    assert_eq!(
        performance
            .adapter()
            .controller()
            .timer_store()
            .wake_batch(),
        512
    );
    assert_eq!(
        performance.adapter().controller().park_store().slot_count(),
        512
    );
}

#[test]
fn embedded_runtime_with_policy_preserves_wait_policy_path() {
    let mut runtime = build_embedded_runtime_with_policy::<DummyDriver, CountTask, _>(
        DummyDriver::default(),
        4,
        BalancedRecordingHostPolicy::default(),
    );
    let lease = runtime
        .adapter_mut()
        .controller_mut()
        .park_store_mut()
        .arm_next(dataplane_core_reactor::balanced_profile::BalancedWakeToken(
            9,
        ))
        .expect("embedded lease");
    runtime
        .adapter_mut()
        .controller_mut()
        .timer_store_mut()
        .arm(777, lease)
        .expect("embedded timer");

    let tick = runtime
        .tick_or_wait(1, 1, 2, 8, |_| {})
        .expect("embedded tick_or_wait");

    assert_eq!(
        tick.controller_step.host_action,
        dataplane_core_reactor::balanced_profile::BalancedHostAction::WaitUntil {
            deadline_ns: 777
        }
    );
    assert_eq!(runtime.policy().last_deadline_ns(), Some(777));
    assert_eq!(runtime.runtime().driver().wait_calls, vec![2]);
}

#[test]
fn embedded_runtime_loop_handle_surfaces_explicit_park_exhaustion() {
    let mut runtime = RuntimeLoopHandle::new(build_embedded_runtime::<DummyDriver, CountTask>(
        DummyDriver::default(),
        1,
    ));
    let slot_count = runtime
        .inner
        .adapter()
        .controller()
        .park_store()
        .slot_count();

    let first = runtime
        .try_arm_embedded_park(dataplane_core_reactor::balanced_profile::BalancedWakeToken(
            1,
        ))
        .expect("first embedded lease");
    assert_eq!(first.slot.0, 0);
    for token in 2..=slot_count as u64 {
        runtime
            .try_arm_embedded_park(dataplane_core_reactor::balanced_profile::BalancedWakeToken(
                token,
            ))
            .expect("embedded handle lease within configured slot count");
    }
    assert_eq!(
        runtime.try_arm_embedded_park(dataplane_core_reactor::balanced_profile::BalancedWakeToken(
            slot_count as u64 + 1
        )),
        Err(EmbeddedResourceError::ParkSlotsExhausted)
    );
}

#[test]
fn embedded_runtime_loop_handle_has_work_reflects_task_admission_and_drain() {
    let mut runtime = RuntimeLoopHandle::new(build_embedded_runtime::<DummyDriver, CountTask>(
        DummyDriver::default(),
        4,
    ));

    assert!(!runtime.has_work());

    runtime
        .try_spawn(CountTask { remaining: 2 })
        .expect("spawn bounded embedded task");
    assert!(runtime.has_work());

    let first_tick = runtime
        .inner
        .tick(1, 1, 1, |_| {})
        .expect("first bounded tick");
    assert_eq!(first_tick.tasks, 1);
    assert!(runtime.has_work());

    let second_tick = runtime
        .inner
        .tick(2, 1, 1, |_| {})
        .expect("second bounded tick");
    assert_eq!(second_tick.tasks, 1);
    assert!(!runtime.has_work());
}

#[test]
fn profiled_runtime_stats_fold_across_ticks_and_capture_gauges() {
    let mut runtime = RuntimeLoopHandle::new(build_embedded_runtime::<DummyDriver, CountTask>(
        DummyDriver::default(),
        4,
    ));

    let initial = runtime.inner.stats();
    assert_eq!(initial.ticks, 0);
    assert_eq!(initial.tasks_run, 0);

    runtime
        .try_spawn(CountTask { remaining: 2 })
        .expect("spawn bounded embedded task");

    let _ = runtime.inner.tick(1, 1, 1, |_| {}).expect("tick 1");
    let _ = runtime.inner.tick(2, 1, 1, |_| {}).expect("tick 2");

    let snap = runtime.inner.stats();
    assert_eq!(snap.ticks, 2);
    assert_eq!(snap.tasks_run, 2);
    assert_eq!(snap.continue_ticks, 2);
    assert_eq!(snap.active_tasks, 0);
    assert_eq!(snap.tasks_dropped, 0);
}

#[test]
fn embedded_runtime_loop_handle_try_spawn_surfaces_capacity_error() {
    let mut runtime = RuntimeLoopHandle::new(build_embedded_runtime::<DummyDriver, CountTask>(
        DummyDriver::default(),
        1,
    ));

    runtime
        .try_spawn(CountTask { remaining: 1 })
        .expect("first task should fit in bounded embedded capacity");

    let overflow = runtime
        .try_spawn(CountTask { remaining: 7 })
        .expect_err("second task should return explicit capacity error");
    let original = overflow.into_task();
    assert_eq!(original.remaining, 7);
    assert!(runtime.has_work());
}

#[test]
fn profiled_runtime_builder_selects_requested_family() {
    let balanced = build_profiled_runtime::<DummyDriver, CountTask>(
        ProfileKind::Balanced,
        DummyDriver::default(),
        4,
    )
    .expect("balanced profiled runtime");
    let embedded = build_profiled_runtime::<DummyDriver, CountTask>(
        ProfileKind::Embedded,
        DummyDriver::default(),
        4,
    )
    .expect("embedded profiled runtime");
    let performance = build_profiled_runtime::<DummyDriver, CountTask>(
        ProfileKind::Performance,
        DummyDriver::default(),
        4,
    )
    .expect("performance profiled runtime");

    assert!(matches!(balanced, ProfiledRuntime::Balanced(_)));
    assert!(matches!(embedded, ProfiledRuntime::Embedded(_)));
    assert!(matches!(performance, ProfiledRuntime::Performance(_)));
}

#[test]
fn profiled_runtime_builder_surfaces_invalid_profile_errors() {
    let profile = TopologyProfile::balanced_dual_shard().with_shard_count(0);

    let result = build_profiled_runtime_from_profile::<DummyDriver, CountTask>(
        profile,
        DummyDriver::default(),
        4,
    );

    assert!(matches!(
        result,
        Err(dataplane_core_reactor::balanced_profile::BalancedProfileError::ShardCountMismatch)
    ));
}

#[test]
fn profiled_runtime_with_policy_preserves_wait_path() {
    let mut runtime = build_profiled_runtime_with_policy::<DummyDriver, CountTask, _>(
        ProfileKind::Embedded,
        DummyDriver::default(),
        4,
        BalancedRecordingHostPolicy::default(),
    )
    .expect("embedded profiled runtime with policy");
    match &mut runtime {
        ProfiledRuntime::Embedded(runtime) => {
            let lease = runtime
                .adapter_mut()
                .controller_mut()
                .park_store_mut()
                .arm_next(dataplane_core_reactor::balanced_profile::BalancedWakeToken(
                    9,
                ))
                .expect("embedded lease");
            runtime
                .adapter_mut()
                .controller_mut()
                .timer_store_mut()
                .arm(888, lease)
                .expect("embedded timer");
        }
        _ => panic!("unexpected profiled runtime family"),
    }

    let tick = runtime
        .tick_or_wait(1, 1, 2, 8, |_| {})
        .expect("profiled runtime tick_or_wait");

    assert_eq!(
        tick.controller_step.host_action,
        dataplane_core_reactor::balanced_profile::BalancedHostAction::WaitUntil {
            deadline_ns: 888
        }
    );
    assert_eq!(runtime.policy().last_deadline_ns(), Some(888));
    assert_eq!(runtime.runtime().driver().wait_calls, vec![2]);
}

#[test]
fn layout_for_profile_matches_family_profile_kind() {
    let embedded =
        layout_for_profile(TopologyProfile::embedded_reference()).expect("embedded layout");
    let performance =
        layout_for_profile(TopologyProfile::performance_dual_shard()).expect("performance layout");

    assert_eq!(embedded.profile().profile_kind, ProfileKind::Embedded);
    assert_eq!(performance.profile().profile_kind, ProfileKind::Performance);
}

#[test]
fn embedded_topology_profile_preserves_reference_bounded_layout_and_runtime_resources() {
    let direct = embedded_profile_layout();
    let from_profile =
        layout_for_profile(TopologyProfile::embedded_reference()).expect("embedded layout");

    assert_eq!(from_profile.profile(), direct.profile());
    assert_eq!(from_profile.budgets(), direct.budgets());
    assert_eq!(
        from_profile.role_for_shard(0),
        direct.inner().role_for_shard(0)
    );
    assert_eq!(
        from_profile.role_for_shard(1),
        direct.inner().role_for_shard(1)
    );

    let runtime = build_profiled_runtime_from_profile::<DummyDriver, CountTask>(
        TopologyProfile::embedded_reference(),
        DummyDriver::default(),
        4,
    )
    .expect("embedded runtime from topology profile");
    let ProfiledRuntime::Embedded(runtime) = runtime else {
        panic!("unexpected runtime family");
    };
    assert_eq!(
        runtime.adapter().controller().timer_store().wake_batch(),
        direct.timer_owner_config().wake_batch
    );
    assert_eq!(
        runtime.adapter().controller().park_store().slot_count(),
        direct.park_slots_config().slot_count
    );
}

#[test]
fn embedded_profiled_runtime_preserves_controller_host_work_split() {
    let mut runtime = build_profiled_runtime_from_profile::<DummyDriver, CountTask>(
        TopologyProfile::embedded_reference(),
        DummyDriver::default(),
        4,
    )
    .expect("embedded runtime from topology profile");

    assert_eq!(runtime.profile_kind(), ProfileKind::Embedded);
    assert!(!runtime.has_work());
    assert!(!runtime.has_runtime_work());
    assert!(!runtime.has_task_work());

    match &mut runtime {
        ProfiledRuntime::Embedded(runtime) => {
            runtime
                .host_mut()
                .try_spawn(CountTask { remaining: 1 })
                .expect("spawn bounded embedded task");
        }
        _ => panic!("unexpected runtime family"),
    }

    assert!(runtime.has_work());
    assert!(!runtime.has_runtime_work());
    assert!(runtime.has_task_work());

    let tick = runtime
        .tick(1, 1, 4, |_| {})
        .expect("embedded profiled runtime tick");
    assert_eq!(tick.tasks, 1);
    assert!(!runtime.has_work());
    assert!(!runtime.has_runtime_work());
    assert!(!runtime.has_task_work());
}

#[test]
fn profiled_runtime_from_profile_selects_requested_family() {
    let balanced = build_profiled_runtime_from_profile::<DummyDriver, CountTask>(
        TopologyProfile::balanced_dual_shard(),
        DummyDriver::default(),
        4,
    )
    .expect("balanced from profile");
    let embedded = build_profiled_runtime_from_profile::<DummyDriver, CountTask>(
        TopologyProfile::embedded_reference(),
        DummyDriver::default(),
        4,
    )
    .expect("embedded from profile");

    assert!(matches!(balanced, ProfiledRuntime::Balanced(_)));
    assert!(matches!(embedded, ProfiledRuntime::Embedded(_)));
}

#[test]
fn profiled_runtime_from_profile_with_policy_preserves_wait_path() {
    let mut runtime = build_profiled_runtime_from_profile_with_policy::<DummyDriver, CountTask, _>(
        TopologyProfile::performance_dual_shard(),
        DummyDriver::default(),
        4,
        BalancedRecordingHostPolicy::default(),
    )
    .expect("performance runtime from profile");
    match &mut runtime {
        ProfiledRuntime::Performance(runtime) => {
            let lease = runtime
                .adapter_mut()
                .controller_mut()
                .park_store_mut()
                .arm_next(dataplane_core_reactor::balanced_profile::BalancedWakeToken(
                    11,
                ))
                .expect("performance lease");
            runtime
                .adapter_mut()
                .controller_mut()
                .timer_store_mut()
                .arm(999, lease)
                .expect("performance timer");
        }
        _ => panic!("unexpected profiled runtime family"),
    }

    let tick = runtime
        .tick_or_wait(1, 1, 2, 8, |_| {})
        .expect("profiled runtime from profile tick_or_wait");

    assert_eq!(
        tick.controller_step.host_action,
        dataplane_core_reactor::balanced_profile::BalancedHostAction::WaitUntil {
            deadline_ns: 999
        }
    );
    assert_eq!(runtime.policy().last_deadline_ns(), Some(999));
    assert_eq!(runtime.runtime().driver().wait_calls, vec![2]);
}

#[test]
fn profiled_runtime_dispatch_returns_selected_family_result() {
    let balanced = build_profiled_runtime_from_profile::<DummyDriver, CountTask>(
        TopologyProfile::balanced_dual_shard(),
        DummyDriver::default(),
        4,
    )
    .expect("build balanced runtime");
    let embedded = build_profiled_runtime_from_profile::<DummyDriver, CountTask>(
        TopologyProfile::embedded_reference(),
        DummyDriver::default(),
        4,
    )
    .expect("build embedded runtime");
    let performance = build_profiled_runtime_from_profile::<DummyDriver, CountTask>(
        TopologyProfile::performance_dual_shard(),
        DummyDriver::default(),
        4,
    )
    .expect("build performance runtime");

    let balanced_kind = balanced.dispatch(
        |runtime| runtime.layout().profile().profile_kind,
        |runtime| runtime.layout().profile().profile_kind,
        |runtime| runtime.layout().profile().profile_kind,
    );
    let embedded_kind = embedded.dispatch(
        |runtime| runtime.layout().profile().profile_kind,
        |runtime| runtime.layout().profile().profile_kind,
        |runtime| runtime.layout().profile().profile_kind,
    );
    let performance_kind = performance.dispatch(
        |runtime| runtime.layout().profile().profile_kind,
        |runtime| runtime.layout().profile().profile_kind,
        |runtime| runtime.layout().profile().profile_kind,
    );

    assert_eq!(balanced_kind, ProfileKind::Balanced);
    assert_eq!(embedded_kind, ProfileKind::Embedded);
    assert_eq!(performance_kind, ProfileKind::Performance);
}

#[test]
fn construction_time_dispatch_returns_selected_family_result() {
    let balanced_kind = dispatch_profiled_runtime_from_profile_with_policy::<
        DummyDriver,
        CountTask,
        _,
        _,
        _,
        _,
        _,
    >(
        TopologyProfile::balanced_dual_shard(),
        DummyDriver::default(),
        4,
        BalancedRecordingHostPolicy::default(),
        |runtime| runtime.layout().profile().profile_kind,
        |runtime| runtime.layout().profile().profile_kind,
        |runtime| runtime.layout().profile().profile_kind,
    )
    .expect("balanced dispatch");
    let embedded_kind = dispatch_profiled_runtime_from_profile_with_policy::<
        DummyDriver,
        CountTask,
        _,
        _,
        _,
        _,
        _,
    >(
        TopologyProfile::embedded_dual_shard(),
        DummyDriver::default(),
        4,
        BalancedRecordingHostPolicy::default(),
        |runtime| runtime.layout().profile().profile_kind,
        |runtime| runtime.layout().profile().profile_kind,
        |runtime| runtime.layout().profile().profile_kind,
    )
    .expect("embedded dispatch");
    let performance_kind = dispatch_profiled_runtime_from_profile_with_policy::<
        DummyDriver,
        CountTask,
        _,
        _,
        _,
        _,
        _,
    >(
        TopologyProfile::performance_dual_shard(),
        DummyDriver::default(),
        4,
        BalancedRecordingHostPolicy::default(),
        |runtime| runtime.layout().profile().profile_kind,
        |runtime| runtime.layout().profile().profile_kind,
        |runtime| runtime.layout().profile().profile_kind,
    )
    .expect("performance dispatch");

    assert_eq!(balanced_kind, ProfileKind::Balanced);
    assert_eq!(embedded_kind, ProfileKind::Embedded);
    assert_eq!(performance_kind, ProfileKind::Performance);
}

#[test]
fn profile_layout_dispatch_routes_embedded_typed_closure() {
    let profile_kind = dispatch_profile_layout_from_profile(
        TopologyProfile::embedded_reference(),
        |_layout| ProfileKind::Balanced,
        |layout| layout.profile().profile_kind,
        |_layout| ProfileKind::Performance,
    )
    .expect("embedded layout dispatch");

    assert_eq!(profile_kind, ProfileKind::Embedded);
}
