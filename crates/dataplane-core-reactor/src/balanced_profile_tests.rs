use crate::balanced_profile::{
    BalancedController, BalancedControllerPhase, BalancedHostAction, BalancedParkSlotId,
    BalancedParkSlotState, BalancedParkSlots, BalancedParkSlotsConfig, BalancedProfileBudgets,
    BalancedProfileError, BalancedProfileLayout, BalancedRecordingHostPolicy, BalancedShardRole,
    BalancedTimerOwner, BalancedTimerOwnerConfig, BalancedWakeToken, EmbeddedParkStore,
    EmbeddedProfileLayout, EmbeddedResourceError, EmbeddedTimerStore, ParkStoreOps,
    PerformanceParkStore, PerformanceProfileLayout, PerformanceTimerStore, TimerStoreOps,
};
use crate::host_loop::HostLoop;
use crate::native_task::{NativeTask, NativeTaskCx, NativeTaskEngine, StepResult};
use crate::reactor_driver::{DriverBackendKind, DriverCapabilities, ReactorDriverWait};
use crate::reactor_model::OpToken;
use crate::reactor_runtime::ReactorRuntime;

#[derive(Default)]
struct DummyDriver {
    outstanding: usize,
    wait_calls: Vec<usize>,
    deadline_calls: Vec<Option<u64>>,
}

struct CountTask {
    remaining: usize,
}

impl crate::reactor_driver::ReactorDriver for DummyDriver {
    type Error = std::io::Error;
    type Token = OpToken;
    type Submit = ();
    type Event = ();

    fn submit(&mut self, _op: Self::Submit, _token: Self::Token) -> Result<(), Self::Error> {
        self.outstanding += 1;
        Ok(())
    }

    fn flush(&mut self) -> Result<usize, Self::Error> {
        Ok(0)
    }

    fn drain<F>(&mut self, _max_events: usize, _on_event: F) -> Result<usize, Self::Error>
    where
        F: FnMut(Self::Event),
    {
        Ok(0)
    }

    fn outstanding(&self) -> usize {
        self.outstanding
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
}

impl ReactorDriverWait for DummyDriver {
    type Error = std::io::Error;
    type Readiness = ();

    fn readiness(&self) -> Option<Self::Readiness> {
        None
    }

    fn wait(&mut self, min_events: usize) -> Result<usize, Self::Error> {
        self.wait_calls.push(min_events);
        self.outstanding = 0;
        Ok(1)
    }

    fn wait_deadline(
        &mut self,
        min_events: usize,
        timeout_ns: Option<u64>,
    ) -> Result<usize, Self::Error> {
        self.deadline_calls.push(timeout_ns);
        self.wait(min_events)
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
fn reference_layout_is_valid() {
    let layout = BalancedProfileLayout::reference();
    assert_eq!(layout.validate(), Ok(()));
    assert_eq!(layout.shard_group().shard_count(), 2);
    assert_eq!(layout.role_for_shard(0), Some(BalancedShardRole::Control));
    assert_eq!(layout.role_for_shard(1), Some(BalancedShardRole::Worker));
}

#[test]
fn missing_role_split_is_rejected() {
    let mut layout = BalancedProfileLayout::reference();
    layout = BalancedProfileLayout::new(
        layout.profile().clone(),
        layout.shard_group().clone(),
        [BalancedShardRole::Control, BalancedShardRole::Control],
        BalancedProfileBudgets::default(),
    );
    assert_eq!(
        layout.validate(),
        Err(BalancedProfileError::MissingRoleSplit)
    );
}

#[test]
fn from_profile_uses_topology_profile_resolution() {
    let layout = BalancedProfileLayout::from_profile(
        dataplane_topology::TopologyProfile::balanced_dual_shard(),
        [BalancedShardRole::Control, BalancedShardRole::Worker],
        BalancedProfileBudgets::default(),
    )
    .expect("layout from profile");

    assert_eq!(layout.shard_group().shard_count(), 2);
}

#[test]
fn embedded_reference_layout_uses_embedded_profile_and_smaller_budgets() {
    let layout = EmbeddedProfileLayout::reference();

    assert_eq!(
        layout.profile().profile_kind,
        dataplane_topology::ProfileKind::Embedded
    );
    assert_eq!(
        layout.profile().profile_kind,
        dataplane_topology::ProfileKind::Embedded
    );
    assert_eq!(layout.budgets().task_budget, 64);
    assert_eq!(layout.budgets().completion_budget, 32);
    assert_eq!(layout.budgets().ingress_budget, 32);
    assert_eq!(layout.park_slots_config().slot_count, 64);
    assert_eq!(layout.timer_owner_config().initial_capacity, 32);
    assert_eq!(layout.timer_owner_config().wake_batch, 16);
    assert_eq!(layout.role_for_shard(0), Some(BalancedShardRole::Control));
    assert_eq!(layout.role_for_shard(1), Some(BalancedShardRole::Worker));
}

#[test]
fn embedded_reference_layout_exposes_explicit_bounded_policy_summary() {
    let layout = EmbeddedProfileLayout::reference();
    let policy = layout.policy();

    assert_eq!(policy.shard_count, 2);
    assert_eq!(
        policy.queue_profile,
        dataplane_topology::ProfileKind::Embedded
    );
    assert_eq!(policy.budgets.task_budget, 64);
    assert_eq!(policy.budgets.completion_budget, 32);
    assert_eq!(policy.budgets.ingress_budget, 32);
    assert_eq!(policy.timer_owner.initial_capacity, 32);
    assert_eq!(policy.timer_owner.wake_batch, 16);
    assert_eq!(policy.park_slots.slot_count, 64);
}

#[test]
fn embedded_controller_try_arm_park_returns_explicit_exhaustion_error() {
    let mut controller = BalancedController::new(
        EmbeddedTimerStore::from_config(BalancedTimerOwnerConfig {
            initial_capacity: 1,
            wake_batch: 1,
            max_entries: None,
        }),
        EmbeddedParkStore::from_config(BalancedParkSlotsConfig { slot_count: 1 }),
    );

    let first = controller
        .try_arm_park(BalancedWakeToken(1))
        .expect("first embedded lease");
    assert_eq!(first.slot.0, 0);
    assert_eq!(
        controller.try_arm_park(BalancedWakeToken(2)),
        Err(EmbeddedResourceError::ParkSlotsExhausted)
    );
}

#[test]
fn embedded_controller_try_arm_timer_returns_explicit_capacity_error() {
    let mut controller = BalancedController::new(
        EmbeddedTimerStore::from_config(BalancedTimerOwnerConfig {
            initial_capacity: 1,
            wake_batch: 1,
            max_entries: None,
        }),
        EmbeddedParkStore::from_config(BalancedParkSlotsConfig { slot_count: 2 }),
    );

    let lease_a = controller
        .try_arm_park(BalancedWakeToken(1))
        .expect("first embedded lease");
    controller
        .try_arm_timer(10, lease_a)
        .expect("first embedded timer");

    let lease_b = controller
        .try_arm_park(BalancedWakeToken(2))
        .expect("second embedded lease");
    assert_eq!(
        controller.try_arm_timer(20, lease_b),
        Err(EmbeddedResourceError::TimerCapacityExceeded)
    );
}

#[test]
fn embedded_timer_store_arm_returns_capacity_error_through_generic_controller_path() {
    let mut controller = BalancedController::new(
        EmbeddedTimerStore::from_config(BalancedTimerOwnerConfig {
            initial_capacity: 1,
            wake_batch: 1,
            max_entries: None,
        }),
        EmbeddedParkStore::from_config(BalancedParkSlotsConfig { slot_count: 2 }),
    );

    let lease_a = controller
        .park_store_mut()
        .arm_next(BalancedWakeToken(3))
        .expect("first embedded lease");
    controller
        .timer_store_mut()
        .arm(10, lease_a)
        .expect("first embedded timer");

    let lease_b = controller
        .park_store_mut()
        .arm_next(BalancedWakeToken(4))
        .expect("second embedded lease");
    assert_eq!(
        controller.timer_store_mut().arm(20, lease_b),
        Err(EmbeddedResourceError::TimerCapacityExceeded)
    );
}

#[test]
fn performance_reference_layout_uses_performance_profile_and_larger_budgets() {
    let layout = PerformanceProfileLayout::reference();

    assert_eq!(
        layout.profile().profile_kind,
        dataplane_topology::ProfileKind::Performance
    );
    assert_eq!(
        layout.profile().profile_kind,
        dataplane_topology::ProfileKind::Performance
    );
    assert_eq!(layout.budgets().task_budget, 512);
    assert_eq!(layout.budgets().completion_budget, 512);
    assert_eq!(layout.budgets().ingress_budget, 256);
    assert_eq!(layout.park_slots_config().slot_count, 512);
    assert_eq!(layout.timer_owner_config().initial_capacity, 512);
    assert_eq!(layout.timer_owner_config().wake_batch, 512);
    assert_eq!(layout.role_for_shard(0), Some(BalancedShardRole::Control));
    assert_eq!(layout.role_for_shard(1), Some(BalancedShardRole::Worker));
}

#[test]
fn family_layouts_build_family_specific_controller_resources() {
    let embedded = EmbeddedProfileLayout::reference();
    let performance = PerformanceProfileLayout::reference();

    assert_eq!(embedded.make_timer_owner().wake_batch(), 16);
    assert_eq!(embedded.make_park_slots().slot_count(), 64);

    assert_eq!(performance.make_timer_owner().wake_batch(), 512);
    assert_eq!(performance.make_park_slots().slot_count(), 512);
}

#[test]
fn family_store_types_wrap_distinct_configured_resources() {
    let embedded = EmbeddedProfileLayout::reference();
    let performance = PerformanceProfileLayout::reference();

    let embedded_timer =
        EmbeddedTimerStore::from_config(EmbeddedTimerStore::config_from_layout(&embedded));
    let embedded_park =
        EmbeddedParkStore::from_config(EmbeddedParkStore::config_from_layout(&embedded));
    let performance_timer =
        PerformanceTimerStore::from_config(PerformanceTimerStore::config_from_layout(&performance));
    let performance_park =
        PerformanceParkStore::from_config(PerformanceParkStore::config_from_layout(&performance));

    assert_eq!(embedded_timer.wake_batch(), 16);
    assert_eq!(embedded_park.slot_count(), 64);
    assert_eq!(performance_timer.wake_batch(), 512);
    assert_eq!(performance_park.slot_count(), 512);
}

#[test]
fn family_controllers_preserve_shared_step_semantics() {
    let balanced = BalancedProfileLayout::reference();
    let embedded = EmbeddedProfileLayout::reference();
    let performance = PerformanceProfileLayout::reference();

    let mut balanced_controller = balanced.make_controller();
    let mut embedded_controller = embedded.make_controller();
    let mut performance_controller = performance.make_controller();

    let balanced_lease = balanced_controller
        .park_store_mut()
        .arm_next(BalancedWakeToken(1))
        .expect("balanced lease");
    balanced_controller
        .timer_store_mut()
        .arm(50, balanced_lease);

    let embedded_lease = embedded_controller
        .park_store_mut()
        .arm_next(BalancedWakeToken(1))
        .expect("embedded lease");
    embedded_controller
        .timer_store_mut()
        .arm(50, embedded_lease)
        .expect("embedded timer");

    let performance_lease = performance_controller
        .park_store_mut()
        .arm_next(BalancedWakeToken(1))
        .expect("performance lease");
    performance_controller
        .timer_store_mut()
        .arm(50, performance_lease)
        .expect("performance timer");

    let balanced_step = balanced_controller.step(50, false, false);
    let embedded_step = embedded_controller.step(50, false, false);
    let performance_step = performance_controller.step(50, false, false);

    assert_eq!(balanced_step, embedded_step);
    assert_eq!(balanced_step, performance_step);
    assert_eq!(balanced_step.phase, BalancedControllerPhase::ProcessTimers);
    assert_eq!(balanced_step.host_action, BalancedHostAction::Continue);
    assert_eq!(balanced_step.expired_timers, 1);
}

#[test]
fn family_runtimes_build_with_family_specific_adapter_stores() {
    let embedded = EmbeddedProfileLayout::reference();
    let performance = PerformanceProfileLayout::reference();

    let embedded_runtime = embedded
        .build_runtime_with_task_capacity::<DummyDriver, CountTask>(DummyDriver::default(), 4);
    let performance_runtime = performance
        .build_runtime_with_task_capacity::<DummyDriver, CountTask>(DummyDriver::default(), 4);

    assert_eq!(
        embedded_runtime
            .adapter()
            .controller()
            .timer_store()
            .wake_batch(),
        16
    );
    assert_eq!(
        embedded_runtime
            .adapter()
            .controller()
            .park_store()
            .slot_count(),
        64
    );
    assert_eq!(
        embedded_runtime
            .adapter()
            .controller()
            .park_store()
            .state(BalancedParkSlotId(0)),
        Some(BalancedParkSlotState::Vacant)
    );
    assert_eq!(
        performance_runtime
            .adapter()
            .controller()
            .timer_store()
            .wake_batch(),
        512
    );
    assert_eq!(
        performance_runtime
            .adapter()
            .controller()
            .park_store()
            .slot_count(),
        512
    );
    assert_eq!(
        performance_runtime
            .adapter()
            .controller()
            .park_store()
            .state(BalancedParkSlotId(0)),
        Some(BalancedParkSlotState::Vacant)
    );
}

#[test]
fn park_slots_arm_wake_release_roundtrip() {
    let mut slots = BalancedParkSlots::from_config(BalancedParkSlotsConfig { slot_count: 2 });
    let lease = slots
        .arm_next(BalancedWakeToken(11))
        .expect("lease from free slot");

    assert_eq!(
        slots.state(lease.slot),
        Some(BalancedParkSlotState::Armed {
            generation: lease.generation,
            wake_token: lease.wake_token,
        })
    );
    assert!(slots.wake(lease));
    assert_eq!(
        slots.state(lease.slot),
        Some(BalancedParkSlotState::Woken {
            generation: lease.generation,
            wake_token: lease.wake_token,
        })
    );
    assert!(slots.release(lease));
    assert_eq!(slots.state(lease.slot), Some(BalancedParkSlotState::Vacant));
}

#[test]
fn park_slots_reject_stale_generation_after_reuse() {
    let mut slots = BalancedParkSlots::from_config(BalancedParkSlotsConfig { slot_count: 1 });
    let first = slots.arm_next(BalancedWakeToken(1)).expect("first lease");
    assert!(slots.cancel(first));
    assert!(slots.release(first));

    let second = slots.arm_next(BalancedWakeToken(2)).expect("second lease");
    assert_eq!(second.slot, BalancedParkSlotId(0));
    assert_ne!(second.generation, first.generation);
    assert!(!slots.wake(first));
    assert!(slots.wake(second));
}

#[test]
fn timer_owner_keeps_earliest_deadline_visible() {
    let mut owner = BalancedTimerOwner::from_config(BalancedTimerOwnerConfig {
        initial_capacity: 4,
        wake_batch: 4,
        max_entries: None,
    });
    let mut slots = BalancedParkSlots::from_config(BalancedParkSlotsConfig { slot_count: 2 });
    let late = slots.arm_next(BalancedWakeToken(10)).expect("late lease");
    let early = slots.arm_next(BalancedWakeToken(20)).expect("early lease");

    owner.arm(50, late);
    owner.arm(10, early);

    assert_eq!(owner.next_deadline(), Some(10));
}

#[test]
fn timer_owner_drains_only_expired_entries_up_to_batch() {
    let mut owner = BalancedTimerOwner::from_config(BalancedTimerOwnerConfig {
        initial_capacity: 4,
        wake_batch: 2,
        max_entries: None,
    });
    let mut slots = BalancedParkSlots::from_config(BalancedParkSlotsConfig { slot_count: 3 });
    let a = slots.arm_next(BalancedWakeToken(1)).expect("lease a");
    let b = slots.arm_next(BalancedWakeToken(2)).expect("lease b");
    let c = slots.arm_next(BalancedWakeToken(3)).expect("lease c");
    owner.arm(5, a);
    owner.arm(7, b);
    owner.arm(100, c);

    let mut drained = Vec::new();
    let count = owner.drain_expired(10, |wake| drained.push(wake));

    assert_eq!(count, 2);
    assert_eq!(drained.len(), 2);
    assert_eq!(drained[0].lease, a);
    assert_eq!(drained[1].lease, b);
    assert_eq!(owner.next_deadline(), Some(100));
}

#[test]
fn reference_layout_builds_balanced_timer_and_park_skeletons() {
    let layout = BalancedProfileLayout::reference();
    let timer = layout.make_timer_owner();
    let slots = layout.make_park_slots();

    assert_eq!(timer.wake_batch(), layout.budgets().completion_budget);
    assert!(slots.slot_count() >= 64);
}

#[test]
fn controller_waits_on_next_deadline_when_no_other_work_exists() {
    let mut controller = BalancedController::new(
        BalancedTimerOwner::from_config(BalancedTimerOwnerConfig {
            initial_capacity: 4,
            wake_batch: 2,
            max_entries: None,
        }),
        BalancedParkSlots::from_config(BalancedParkSlotsConfig { slot_count: 2 }),
    );
    let lease = controller
        .park_slots_mut()
        .arm_next(BalancedWakeToken(7))
        .expect("lease");
    controller.timer_owner_mut().arm(123, lease);

    let step = controller.step(10, false, false);
    assert_eq!(step.phase, BalancedControllerPhase::Idle);
    assert_eq!(step.expired_timers, 0);
    assert_eq!(
        step.host_action,
        BalancedHostAction::WaitUntil { deadline_ns: 123 }
    );
}

#[test]
fn controller_processes_expired_timers_before_idling() {
    let mut controller = BalancedController::new(
        BalancedTimerOwner::from_config(BalancedTimerOwnerConfig {
            initial_capacity: 4,
            wake_batch: 4,
            max_entries: None,
        }),
        BalancedParkSlots::from_config(BalancedParkSlotsConfig { slot_count: 2 }),
    );
    let lease = controller
        .park_slots_mut()
        .arm_next(BalancedWakeToken(9))
        .expect("lease");
    controller.timer_owner_mut().arm(50, lease);

    let step = controller.step(50, false, false);
    assert_eq!(step.phase, BalancedControllerPhase::ProcessTimers);
    assert_eq!(step.expired_timers, 1);
    assert_eq!(step.host_action, BalancedHostAction::Continue);
    assert_eq!(
        controller.park_slots().state(lease.slot),
        Some(BalancedParkSlotState::Woken {
            generation: lease.generation,
            wake_token: lease.wake_token,
        })
    );
}

#[test]
fn controller_prefers_runtime_and_task_work_over_waiting() {
    let mut controller = BalancedController::new(
        BalancedTimerOwner::from_config(BalancedTimerOwnerConfig::default()),
        BalancedParkSlots::from_config(BalancedParkSlotsConfig { slot_count: 1 }),
    );

    let runtime_step = controller.step(0, true, false);
    assert_eq!(
        runtime_step.phase,
        BalancedControllerPhase::DrainCompletions
    );
    assert_eq!(runtime_step.host_action, BalancedHostAction::Continue);

    let task_step = controller.step(0, false, true);
    assert_eq!(task_step.phase, BalancedControllerPhase::RunTasks);
    assert_eq!(task_step.host_action, BalancedHostAction::Continue);
}

#[test]
fn controller_step_precedence_is_runtime_then_timers_then_tasks_then_wait() {
    let mut controller = BalancedController::new(
        BalancedTimerOwner::from_config(BalancedTimerOwnerConfig {
            initial_capacity: 4,
            wake_batch: 4,
            max_entries: None,
        }),
        BalancedParkSlots::from_config(BalancedParkSlotsConfig { slot_count: 2 }),
    );
    let lease = controller
        .park_slots_mut()
        .arm_next(BalancedWakeToken(77))
        .expect("lease");
    controller.timer_owner_mut().arm(250, lease);

    let runtime_and_task = controller.step(100, true, true);
    assert_eq!(
        runtime_and_task.phase,
        BalancedControllerPhase::DrainCompletions
    );
    assert_eq!(runtime_and_task.host_action, BalancedHostAction::Continue);

    let expired_and_task = controller.step(300, false, true);
    assert_eq!(
        expired_and_task.phase,
        BalancedControllerPhase::ProcessTimers
    );
    assert_eq!(expired_and_task.host_action, BalancedHostAction::Continue);
    assert_eq!(expired_and_task.expired_timers, 1);

    let task_only = controller.step(300, false, true);
    assert_eq!(task_only.phase, BalancedControllerPhase::RunTasks);
    assert_eq!(task_only.host_action, BalancedHostAction::Continue);

    let wait_only = controller.step(300, false, false);
    assert_eq!(wait_only.phase, BalancedControllerPhase::Idle);
    assert_eq!(wait_only.host_action, BalancedHostAction::Idle);
}

#[test]
fn reference_layout_builds_balanced_controller() {
    let layout = BalancedProfileLayout::reference();
    let controller = layout.make_controller();

    assert_eq!(controller.phase(), BalancedControllerPhase::Idle);
    assert!(controller.timer_owner().is_empty());
    assert!(controller.park_slots().slot_count() >= 64);
}

#[test]
fn host_adapter_waits_via_external_closure_when_deadline_exists() {
    let layout = BalancedProfileLayout::reference();
    let mut adapter = layout.make_host_adapter();
    let lease = adapter
        .controller_mut()
        .park_slots_mut()
        .arm_next(BalancedWakeToken(1))
        .expect("lease");
    adapter.controller_mut().timer_owner_mut().arm(99, lease);

    let mut host = HostLoop::new(
        ReactorRuntime::new(DummyDriver::default()),
        NativeTaskEngine::<CountTask>::with_task_capacity(4),
    );
    let mut waited_until = None;
    let tick = adapter
        .tick(
            &mut host,
            10,
            16,
            16,
            |_| {},
            |deadline| waited_until = Some(deadline),
        )
        .expect("adapter tick");

    assert_eq!(
        tick.controller_step.host_action,
        BalancedHostAction::WaitUntil { deadline_ns: 99 }
    );
    assert_eq!(waited_until, Some(99));
    assert_eq!(tick.events, 0);
    assert_eq!(tick.tasks, 0);
}

#[test]
fn host_adapter_continues_when_runtime_work_is_present() {
    let layout = BalancedProfileLayout::reference();
    let mut adapter = layout.make_host_adapter();
    let mut host = HostLoop::new(
        ReactorRuntime::new(DummyDriver {
            outstanding: 1,
            ..DummyDriver::default()
        }),
        NativeTaskEngine::<CountTask>::with_task_capacity(4),
    );
    let mut waited = false;
    let tick = adapter
        .tick(&mut host, 0, 16, 16, |_| {}, |_| waited = true)
        .expect("adapter tick");

    assert_eq!(
        tick.controller_step.phase,
        BalancedControllerPhase::DrainCompletions
    );
    assert_eq!(
        tick.controller_step.host_action,
        BalancedHostAction::Continue
    );
    assert!(!waited);
}

#[test]
fn host_adapter_tick_or_wait_uses_runtime_wait_after_deadline_hook() {
    let layout = BalancedProfileLayout::reference();
    let mut adapter = layout.make_host_adapter();
    let lease = adapter
        .controller_mut()
        .park_slots_mut()
        .arm_next(BalancedWakeToken(5))
        .expect("lease");
    adapter.controller_mut().timer_owner_mut().arm(77, lease);

    let mut host = HostLoop::new(
        ReactorRuntime::new(DummyDriver::default()),
        NativeTaskEngine::<CountTask>::with_task_capacity(4),
    );
    let mut waited_until = None;
    let mut seen = Vec::new();
    let tick = adapter
        .tick_or_wait(
            &mut host,
            10,
            1,
            2,
            8,
            |event| seen.push(event),
            |deadline| waited_until = Some(deadline),
        )
        .expect("adapter tick_or_wait");

    assert_eq!(
        tick.controller_step.host_action,
        BalancedHostAction::WaitUntil { deadline_ns: 77 }
    );
    assert_eq!(waited_until, Some(77));
    assert_eq!(host.runtime().driver().wait_calls, vec![2]);
    assert_eq!(tick.events, 0);
    assert_eq!(tick.tasks, 0);
    assert_eq!(seen, Vec::<()>::new());
}

#[test]
fn host_adapter_tick_with_policy_records_deadline() {
    let layout = BalancedProfileLayout::reference();
    let mut adapter = layout.make_host_adapter();
    let lease = adapter
        .controller_mut()
        .park_slots_mut()
        .arm_next(BalancedWakeToken(12))
        .expect("lease");
    adapter.controller_mut().timer_owner_mut().arm(222, lease);

    let mut host = HostLoop::new(
        ReactorRuntime::new(DummyDriver::default()),
        NativeTaskEngine::<CountTask>::with_task_capacity(4),
    );
    let mut policy = BalancedRecordingHostPolicy::default();
    let tick = adapter
        .tick_with_policy(&mut host, 10, 16, 16, |_| {}, &mut policy)
        .expect("adapter tick_with_policy");

    assert_eq!(
        tick.controller_step.host_action,
        BalancedHostAction::WaitUntil { deadline_ns: 222 }
    );
    assert_eq!(policy.last_deadline_ns(), Some(222));
    assert_eq!(policy.wait_requests(), 1);
}

#[test]
fn host_adapter_tick_or_wait_with_policy_records_deadline_and_waits() {
    let layout = BalancedProfileLayout::reference();
    let mut adapter = layout.make_host_adapter();
    let lease = adapter
        .controller_mut()
        .park_slots_mut()
        .arm_next(BalancedWakeToken(13))
        .expect("lease");
    adapter.controller_mut().timer_owner_mut().arm(333, lease);

    let mut host = HostLoop::new(
        ReactorRuntime::new(DummyDriver::default()),
        NativeTaskEngine::<CountTask>::with_task_capacity(4),
    );
    let mut policy = BalancedRecordingHostPolicy::default();
    let tick = adapter
        .tick_or_wait_with_policy(&mut host, 10, 1, 2, 8, |_| {}, &mut policy)
        .expect("adapter tick_or_wait_with_policy");

    assert_eq!(
        tick.controller_step.host_action,
        BalancedHostAction::WaitUntil { deadline_ns: 333 }
    );
    assert_eq!(policy.last_deadline_ns(), Some(333));
    assert_eq!(policy.wait_requests(), 1);
    assert_eq!(host.runtime().driver().wait_calls, vec![2]);
}

#[test]
fn balanced_runtime_tick_records_deadline_through_policy() {
    let layout = BalancedProfileLayout::reference();
    let mut runtime = layout.build_runtime_with_policy::<DummyDriver, CountTask, _>(
        DummyDriver::default(),
        4,
        BalancedRecordingHostPolicy::default(),
    );
    let lease = runtime
        .adapter_mut()
        .controller_mut()
        .park_slots_mut()
        .arm_next(BalancedWakeToken(21))
        .expect("lease");
    runtime
        .adapter_mut()
        .controller_mut()
        .timer_owner_mut()
        .arm(444, lease);

    let tick = runtime.tick(10, 16, 16, |_| {}).expect("runtime tick");

    assert_eq!(
        tick.controller_step.host_action,
        BalancedHostAction::WaitUntil { deadline_ns: 444 }
    );
    assert_eq!(runtime.policy().last_deadline_ns(), Some(444));
    assert_eq!(runtime.policy().wait_requests(), 1);
}

#[test]
fn balanced_runtime_tick_or_wait_uses_policy_and_runtime_wait() {
    let layout = BalancedProfileLayout::reference();
    let mut runtime = layout.build_runtime_with_policy::<DummyDriver, CountTask, _>(
        DummyDriver::default(),
        4,
        BalancedRecordingHostPolicy::default(),
    );
    let lease = runtime
        .adapter_mut()
        .controller_mut()
        .park_slots_mut()
        .arm_next(BalancedWakeToken(22))
        .expect("lease");
    runtime
        .adapter_mut()
        .controller_mut()
        .timer_owner_mut()
        .arm(555, lease);

    let tick = runtime
        .tick_or_wait(10, 1, 2, 8, |_| {})
        .expect("runtime tick_or_wait");

    assert_eq!(
        tick.controller_step.host_action,
        BalancedHostAction::WaitUntil { deadline_ns: 555 }
    );
    assert_eq!(runtime.policy().last_deadline_ns(), Some(555));
    assert_eq!(runtime.policy().wait_requests(), 1);
    assert_eq!(runtime.runtime().driver().wait_calls, vec![2]);
}

#[test]
fn balanced_runtime_reference_constructor_is_coherent() {
    let runtime = BalancedProfileLayout::reference()
        .build_runtime_with_task_capacity::<DummyDriver, CountTask>(DummyDriver::default(), 8);

    assert_eq!(runtime.layout().shard_group().shard_count(), 2);
    assert_eq!(
        runtime.layout().role_for_shard(0),
        Some(BalancedShardRole::Control)
    );
    assert_eq!(
        runtime.layout().role_for_shard(1),
        Some(BalancedShardRole::Worker)
    );
    assert_eq!(runtime.policy().last_deadline_ns(), None);
    assert_eq!(runtime.policy().wait_requests(), 0);
}

#[test]
fn balanced_runtime_from_layout_uses_given_layout() {
    let layout = BalancedProfileLayout::from_profile(
        dataplane_topology::TopologyProfile::balanced_dual_shard(),
        [BalancedShardRole::Worker, BalancedShardRole::Control],
        BalancedProfileBudgets {
            task_budget: 32,
            completion_budget: 48,
            ingress_budget: 16,
        },
    )
    .expect("layout");

    let runtime = layout
        .build_runtime_with_task_capacity::<DummyDriver, CountTask>(DummyDriver::default(), 3);

    assert_eq!(runtime.layout().budgets().task_budget, 32);
    assert_eq!(
        runtime.layout().role_for_shard(0),
        Some(BalancedShardRole::Worker)
    );
    assert_eq!(
        runtime.layout().role_for_shard(1),
        Some(BalancedShardRole::Control)
    );

    let runtime_via_layout = layout
        .build_runtime_with_task_capacity::<DummyDriver, CountTask>(DummyDriver::default(), 3);
    assert_eq!(runtime_via_layout.layout().budgets().task_budget, 32);
}

#[test]
fn embedded_runtime_task_capacity_is_hard_fixed_by_construction() {
    let mut runtime = EmbeddedProfileLayout::reference()
        .build_runtime_with_task_capacity::<DummyDriver, CountTask>(DummyDriver::default(), 1);

    assert!(runtime
        .host_mut()
        .try_spawn(CountTask { remaining: 1 })
        .is_ok());
    let err = runtime
        .host_mut()
        .try_spawn(CountTask { remaining: 1 })
        .expect_err("embedded task capacity exhaustion");
    assert_eq!(err.active(), 1);
    assert_eq!(err.max_slots(), 1);
}

#[test]
fn embedded_runtime_try_arm_park_surfaces_explicit_exhaustion_error() {
    let mut runtime = EmbeddedProfileLayout::reference()
        .build_runtime_with_task_capacity::<DummyDriver, CountTask>(DummyDriver::default(), 1);

    let slot_count = runtime.adapter().controller().park_store().slot_count();
    let first = runtime
        .try_arm_park(BalancedWakeToken(1))
        .expect("first embedded runtime lease");
    assert_eq!(first.slot.0, 0);
    for token in 2..=slot_count as u64 {
        runtime
            .try_arm_park(BalancedWakeToken(token))
            .expect("embedded runtime lease within configured slot count");
    }
    assert_eq!(
        runtime.try_arm_park(BalancedWakeToken(slot_count as u64 + 1)),
        Err(EmbeddedResourceError::ParkSlotsExhausted)
    );
}

#[test]
fn io_idle_wait_is_bounded_by_armed_timer_deadline() {
    let layout = BalancedProfileLayout::reference();
    let mut controller = layout.make_controller();
    let lease = controller
        .park_store_mut()
        .arm_next(BalancedWakeToken(1))
        .expect("lease");
    controller
        .timer_store_mut()
        .arm(1_000, lease);
    let mut adapter = crate::balanced_profile::BalancedHostLoopAdapter::new(controller);
    let mut host = HostLoop::new(
        ReactorRuntime::new(DummyDriver::default()),
        NativeTaskEngine::<CountTask>::with_task_capacity(4),
    );

    let tick = adapter
        .tick_or_wait(&mut host, 100, 16, 1, 16, |_event| {}, |_deadline| {})
        .expect("tick");

    assert_eq!(
        tick.controller_step.host_action,
        BalancedHostAction::WaitUntil { deadline_ns: 1_000 }
    );
    assert_eq!(
        host.runtime().driver().deadline_calls,
        vec![Some(900)],
        "driver wait must be bounded by deadline - now"
    );
}
