use super::{
    EmbeddedDriveConfig, EmbeddedDriveResult, EmbeddedHostAdapter, EmbeddedHostLoop,
    EmbeddedHostLoopConfig, EmbeddedPolicySummary,
};
use crate::embedded_host_loop_test_support::{
    AdvancingHost, CountingHost, FakeHost, IdleRecordingHost, NowOnlyHost, NowOnlyTraitBoundHost,
};
use crate::runtime_profiles::{embedded_profile_layout, RuntimeLoopHandle};
use dataplane_core_reactor::balanced_profile::{BalancedWakeToken, EmbeddedResourceError};
use dataplane_core_reactor::native_task::{NativeTask, NativeTaskCx, StepResult};
use dataplane_core_reactor::reactor_driver::{
    DriverBackendKind, DriverCapabilities, ReactorDriver, ReactorDriverWait,
};
use dataplane_core_reactor::reactor_model::{NetEvent, NetOpKind, OpToken};

#[derive(Default)]
struct DummyDriver;

struct CountTask {
    remaining: usize,
}

#[derive(Default)]
struct NonNetworkDummyDriver;

#[derive(Default)]
struct FakeCompletionDriver {
    submitted_tokens: Vec<OpToken>,
    events: Vec<NetEvent>,
}

impl ReactorDriver for DummyDriver {
    type Error = std::io::Error;
    type Token = OpToken;
    type Submit = ();
    type Event = NetEvent;

    fn submit(&mut self, _op: Self::Submit, _token: Self::Token) -> Result<(), Self::Error> {
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
        0
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

    fn wait(&mut self, _min_events: usize) -> Result<usize, Self::Error> {
        Ok(0)
    }
    fn wait_deadline(&mut self, min_events: usize, _timeout_ns: Option<u64>) -> Result<usize, Self::Error> { self.wait(min_events) }

}

impl ReactorDriver for NonNetworkDummyDriver {
    type Error = std::io::Error;
    type Token = u64;
    type Submit = ();
    type Event = ();

    fn submit(&mut self, _op: Self::Submit, _token: Self::Token) -> Result<(), Self::Error> {
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
        0
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

impl ReactorDriver for FakeCompletionDriver {
    type Error = std::io::Error;
    type Token = OpToken;
    type Submit = ();
    type Event = NetEvent;

    fn submit(&mut self, _op: Self::Submit, token: Self::Token) -> Result<(), Self::Error> {
        self.submitted_tokens.push(token);
        self.events.push(NetEvent::OpComplete {
            token,
            kind: NetOpKind::Recv,
            result: 0,
            flags: 0,
        });
        Ok(())
    }

    fn flush(&mut self) -> Result<usize, Self::Error> {
        Ok(0)
    }

    fn drain<F>(&mut self, max_events: usize, mut on_event: F) -> Result<usize, Self::Error>
    where
        F: FnMut(Self::Event),
    {
        let mut drained = 0usize;
        while drained < max_events {
            let Some(event) = self.events.pop() else {
                break;
            };
            on_event(event);
            drained += 1;
        }
        Ok(drained)
    }

    fn outstanding(&self) -> usize {
        self.events.len()
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

impl ReactorDriverWait for FakeCompletionDriver {
    type Error = std::io::Error;
    type Readiness = ();

    fn readiness(&self) -> Option<Self::Readiness> {
        None
    }

    fn wait(&mut self, _min_events: usize) -> Result<usize, Self::Error> {
        Ok(0)
    }
    fn wait_deadline(&mut self, min_events: usize, _timeout_ns: Option<u64>) -> Result<usize, Self::Error> { self.wait(min_events) }

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
fn embedded_host_loop_assembles_bounded_embedded_runtime() {
    let host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 4,
            ..Default::default()
        },
    );
    let policy = host_loop.embedded_policy();
    let runtime = host_loop.into_handle().into_inner();

    assert_eq!(
        runtime.layout().profile().profile_kind,
        dataplane_topology::ProfileKind::Embedded
    );
    assert_eq!(
        runtime.adapter().controller().timer_store().wake_batch(),
        policy.timer_owner.wake_batch
    );
    assert_eq!(
        runtime.adapter().controller().park_store().slot_count(),
        policy.park_slots.slot_count
    );
}

#[test]
fn embedded_host_loop_default_capacity_matches_embedded_policy_budget() {
    let config = EmbeddedHostLoopConfig::default();
    let host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(DummyDriver, config);
    let policy = host_loop.embedded_policy();

    assert_eq!(config.task_capacity(), policy.budgets.task_budget);
    assert_eq!(host_loop.task_capacity(), config.task_capacity());
    assert_eq!(config.drive, EmbeddedDriveConfig::default());
}

#[test]
fn embedded_host_loop_non_network_driver_reuses_assembly_and_admission_shape() {
    let mut host_loop = EmbeddedHostLoop::<NonNetworkDummyDriver, CountTask>::new(
        NonNetworkDummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 2,
            ..Default::default()
        },
    );
    assert!(!host_loop.has_work());

    host_loop
        .try_spawn(CountTask { remaining: 1 })
        .expect("non-network driver should support admission path");
    assert!(host_loop.has_work());
}

#[test]
fn embedded_host_loop_fake_driver_submits_and_drains_one_completion_through_step() {
    let mut driver = FakeCompletionDriver::default();
    let submitted = OpToken(77);
    driver
        .submit((), submitted)
        .expect("fake driver should accept submit");
    let mut host_loop = EmbeddedHostLoop::<FakeCompletionDriver, CountTask>::new(
        driver,
        EmbeddedHostLoopConfig {
            task_capacity: 1,
            ..Default::default()
        },
    );

    let tick = host_loop
        .step(1, 1, 0, 0)
        .expect("step should drain one fake completion");

    assert_eq!(tick.tasks, 0);
    assert_eq!(tick.completions.len(), 1);
    assert_eq!(tick.completions[0].token, submitted);
    assert!(!host_loop.has_work());
}

#[test]
fn zero_capacity_host_loop_rejects_admission() {
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig { task_capacity: 0, ..Default::default() },
    );
    let error = host_loop.try_spawn(CountTask { remaining: 3 }).unwrap_err();
    assert_eq!(error.max_slots(), 0);
    assert_eq!(error.into_task().remaining, 3);
    assert!(!host_loop.has_work());
}

#[test]
fn embedded_host_loop_config_task_capacity_accessor_reports_configured_value() {
    let config = EmbeddedHostLoopConfig {
        task_capacity: 7,
        ..Default::default()
    };
    let host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(DummyDriver, config);

    assert_eq!(config.task_capacity(), 7);
    assert_eq!(host_loop.task_capacity(), config.task_capacity());
}

#[test]
fn embedded_host_loop_custom_task_capacity_below_policy_budget_is_accepted_and_bounded() {
    let config = EmbeddedHostLoopConfig {
        task_capacity: 3,
        ..Default::default()
    };
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(DummyDriver, config);
    let policy = host_loop.embedded_policy();

    assert!(config.task_capacity() < policy.budgets.task_budget);

    host_loop
        .try_spawn(CountTask { remaining: 1 })
        .expect("spawn first task within custom capacity");
    host_loop
        .try_spawn(CountTask { remaining: 1 })
        .expect("spawn second task within custom capacity");
    host_loop
        .try_spawn(CountTask { remaining: 1 })
        .expect("spawn third task within custom capacity");

    let err = host_loop
        .try_spawn(CountTask { remaining: 9 })
        .expect_err("custom task capacity should remain bounded");
    assert_eq!(err.active(), config.task_capacity());
    assert_eq!(err.max_slots(), config.task_capacity());
    assert_eq!(err.into_task().remaining, 9);
}

#[test]
fn embedded_host_loop_try_spawn_capacity_exhaustion_returns_original_task() {
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 1,
            ..Default::default()
        },
    );
    host_loop
        .try_spawn(CountTask { remaining: 1 })
        .expect("spawn first task within capacity");

    let err = host_loop
        .try_spawn(CountTask { remaining: 42 })
        .expect_err("capacity exhaustion should return original task");

    assert_eq!(err.active(), 1);
    assert_eq!(err.max_slots(), 1);
    assert_eq!(err.into_task().remaining, 42);
}

#[test]
fn embedded_host_loop_try_spawn_capacity_exhaustion_does_not_panic() {
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 1,
            ..Default::default()
        },
    );
    host_loop
        .try_spawn(CountTask { remaining: 1 })
        .expect("spawn first task within capacity");

    let spawn_outcome = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        host_loop.try_spawn(CountTask { remaining: 77 })
    }));

    assert!(
        spawn_outcome.is_ok(),
        "try_spawn should return a capacity error instead of panicking"
    );
    let err = spawn_outcome
        .expect("catch_unwind should capture non-panicking spawn result")
        .expect_err("capacity exhaustion should still return an error");
    assert_eq!(err.active(), 1);
    assert_eq!(err.max_slots(), 1);
    assert_eq!(err.into_task().remaining, 77);
}

#[test]
fn embedded_host_loop_policy_summary_constructor_matches_embedded_policy() {
    let config = EmbeddedHostLoopConfig {
        task_capacity: 7,
        ..Default::default()
    };
    let host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(DummyDriver, config);

    assert_eq!(
        host_loop.embedded_policy_summary(),
        EmbeddedPolicySummary::from_policy(config.task_capacity(), host_loop.embedded_policy())
    );
}

#[test]
fn embedded_host_loop_step_drives_bounded_task_progress() {
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 4,
            ..Default::default()
        },
    );
    host_loop
        .try_spawn(CountTask { remaining: 1 })
        .expect("spawn embedded task");

    let tick = host_loop.step(1, 1, 0, 4).expect("embedded host step");

    assert_eq!(tick.tasks, 1);
    assert!(!host_loop.has_work());
}

#[test]
fn embedded_host_loop_step_with_host_uses_external_clock_adapter() {
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 4,
            ..Default::default()
        },
    );
    let mut host = FakeHost {
        now_ns: 123,
        ticks: 0,
    };
    host_loop
        .try_spawn(CountTask { remaining: 1 })
        .expect("spawn embedded task");

    let tick = host_loop
        .step_with_host(&mut host, 1, 0, 4)
        .expect("embedded host adapter step");

    assert_eq!(tick.tasks, 1);
    assert_eq!(host.ticks, 1);
    assert!(!host_loop.has_work());
}

#[test]
fn embedded_host_loop_step_with_host_calls_idle_hook_on_no_progress() {
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 4,
            ..Default::default()
        },
    );
    let mut host = FakeHost {
        now_ns: 123,
        ticks: 0,
    };

    let tick = host_loop
        .step_with_host(&mut host, 1, 0, 4)
        .expect("idle embedded host adapter step");

    assert_eq!(tick.tasks, 0);
    assert_eq!(tick.completions.len(), 0);
    assert_eq!(host.ticks, 101);
}

#[test]
fn embedded_host_loop_drive_steps_with_host_repeats_until_work_drains_or_step_limit() {
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 4,
            ..Default::default()
        },
    );
    let mut host = FakeHost {
        now_ns: 123,
        ticks: 0,
    };
    host_loop
        .try_spawn(CountTask { remaining: 2 })
        .expect("spawn embedded task");

    let result = host_loop
        .drive_steps_with_host(&mut host, 4, 1, 0, 1)
        .expect("drive embedded host adapter steps");

    assert_eq!(
        result,
        EmbeddedDriveResult {
            steps_attempted: 2,
            tasks_run: 2,
            completions_observed: 0,
            work_remains: false,
        }
    );
    assert_eq!(host.ticks, 2);
    assert!(!host_loop.has_work());
}

#[test]
fn embedded_host_loop_drive_steps_with_host_stops_immediately_when_no_work_exists() {
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 4,
            ..Default::default()
        },
    );
    let mut host = FakeHost {
        now_ns: 123,
        ticks: 0,
    };

    let result = host_loop
        .drive_steps_with_host(&mut host, 4, 1, 0, 1)
        .expect("drive embedded host adapter steps with no queued work");

    assert_eq!(
        result,
        EmbeddedDriveResult {
            steps_attempted: 0,
            tasks_run: 0,
            completions_observed: 0,
            work_remains: false,
        }
    );
    assert_eq!(host.ticks, 0);
    assert!(!host_loop.has_work());
}

#[test]
fn embedded_host_loop_drive_steps_with_host_stops_at_step_limit_when_work_remains() {
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 4,
            ..Default::default()
        },
    );
    let mut host = FakeHost {
        now_ns: 123,
        ticks: 0,
    };
    host_loop
        .try_spawn(CountTask { remaining: 3 })
        .expect("spawn embedded task");

    let result = host_loop
        .drive_steps_with_host(&mut host, 1, 1, 0, 1)
        .expect("drive embedded host adapter steps with step limit");

    assert_eq!(
        result,
        EmbeddedDriveResult {
            steps_attempted: 1,
            tasks_run: 1,
            completions_observed: 0,
            work_remains: true,
        }
    );
    assert_eq!(host.ticks, 1);
    assert!(host_loop.has_work());
}

#[test]
fn embedded_host_loop_drive_steps_with_host_one_step_limit_leaves_multi_step_task_pending() {
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 4,
            ..Default::default()
        },
    );
    host_loop
        .try_spawn(CountTask { remaining: 4 })
        .expect("spawn multi-step task");
    let mut host = CountingHost {
        now_ns: 222,
        ..Default::default()
    };

    let result = host_loop
        .drive_steps_with_host(&mut host, 1, 1, 0, 1)
        .expect("drive one step against multi-step task");

    assert_eq!(
        result,
        EmbeddedDriveResult {
            steps_attempted: 1,
            tasks_run: 1,
            completions_observed: 0,
            work_remains: true,
        }
    );
    assert_eq!(host.now_calls, 1);
    assert_eq!(host.idle_calls, 0);
    assert!(host_loop.has_work());
}

#[test]
fn embedded_host_loop_drive_steps_with_host_completes_multi_step_task_within_step_limit() {
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 4,
            ..Default::default()
        },
    );
    host_loop
        .try_spawn(CountTask { remaining: 3 })
        .expect("spawn multi-step task");
    let mut host = CountingHost {
        now_ns: 333,
        ..Default::default()
    };

    let result = host_loop
        .drive_steps_with_host(&mut host, 3, 1, 0, 1)
        .expect("drive enough steps to complete multi-step task");

    assert_eq!(
        result,
        EmbeddedDriveResult {
            steps_attempted: 3,
            tasks_run: 3,
            completions_observed: 0,
            work_remains: false,
        }
    );
    assert_eq!(host.now_calls, 3);
    assert_eq!(host.idle_calls, 0);
    assert!(!host_loop.has_work());
}

#[test]
fn embedded_host_loop_drive_steps_with_host_returns_without_progress_when_step_limit_is_zero() {
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 4,
            ..Default::default()
        },
    );
    host_loop
        .try_spawn(CountTask { remaining: 2 })
        .expect("spawn embedded task");
    let mut host = CountingHost {
        now_ns: 123,
        ..Default::default()
    };

    let result = host_loop
        .drive_steps_with_host(&mut host, 0, 1, 0, 1)
        .expect("drive with zero step limit");

    assert_eq!(
        result,
        EmbeddedDriveResult {
            steps_attempted: 0,
            tasks_run: 0,
            completions_observed: 0,
            work_remains: true,
        }
    );
    assert_eq!(host.now_calls, 0);
    assert_eq!(host.idle_calls, 0);
    assert!(host_loop.has_work());
}

#[test]
fn embedded_host_loop_drive_steps_with_host_calls_idle_hook_only_on_no_progress_steps() {
    let mut no_progress_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 4,
            ..Default::default()
        },
    );
    no_progress_loop
        .try_spawn(CountTask { remaining: 2 })
        .expect("spawn no-progress task");
    let mut no_progress_host = CountingHost {
        now_ns: 456,
        ..Default::default()
    };

    let no_progress_result = no_progress_loop
        .drive_steps_with_host(&mut no_progress_host, 2, 1, 0, 0)
        .expect("drive no-progress steps");

    assert_eq!(
        no_progress_result,
        EmbeddedDriveResult {
            steps_attempted: 2,
            tasks_run: 0,
            completions_observed: 0,
            work_remains: true,
        }
    );
    assert_eq!(no_progress_host.now_calls, 2);
    assert_eq!(no_progress_host.idle_calls, 2);
    assert!(no_progress_loop.has_work());

    let mut progress_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 4,
            ..Default::default()
        },
    );
    progress_loop
        .try_spawn(CountTask { remaining: 2 })
        .expect("spawn progress task");
    let mut progress_host = CountingHost {
        now_ns: 789,
        ..Default::default()
    };

    let progress_result = progress_loop
        .drive_steps_with_host(&mut progress_host, 2, 1, 0, 1)
        .expect("drive progress steps");

    assert_eq!(
        progress_result,
        EmbeddedDriveResult {
            steps_attempted: 2,
            tasks_run: 2,
            completions_observed: 0,
            work_remains: false,
        }
    );
    assert_eq!(progress_host.now_calls, 2);
    assert_eq!(progress_host.idle_calls, 0);
    assert!(!progress_loop.has_work());
}

#[test]
fn embedded_host_loop_drive_steps_with_config_defaults_uses_configured_drive_values() {
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 4,
            drive: EmbeddedDriveConfig {
                step_limit: 2,
                max_events: 1,
                min_events: 0,
                task_budget: 1,
            },
        },
    );
    host_loop
        .try_spawn(CountTask { remaining: 3 })
        .expect("spawn task for configured defaults drive");
    let mut host = CountingHost {
        now_ns: 111,
        ..Default::default()
    };

    let result = host_loop
        .drive_steps_with_config_defaults(&mut host)
        .expect("drive with configured defaults");

    assert_eq!(
        result,
        EmbeddedDriveResult {
            steps_attempted: 2,
            tasks_run: 2,
            completions_observed: 0,
            work_remains: true,
        }
    );
    assert_eq!(host.now_calls, 2);
    assert_eq!(host.idle_calls, 0);
    assert!(host_loop.has_work());
}

#[test]
fn embedded_host_loop_drive_steps_with_host_uses_monotonic_advancing_timestamps() {
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 4,
            ..Default::default()
        },
    );
    host_loop
        .try_spawn(CountTask { remaining: 3 })
        .expect("spawn task for monotonic host clock test");
    let mut host = AdvancingHost {
        next_now_ns: 1_000,
        step_ns: 25,
        ..Default::default()
    };

    let result = host_loop
        .drive_steps_with_host(&mut host, 3, 1, 0, 1)
        .expect("drive with monotonic advancing host timestamps");

    assert_eq!(
        result,
        EmbeddedDriveResult {
            steps_attempted: 3,
            tasks_run: 3,
            completions_observed: 0,
            work_remains: false,
        }
    );
    assert_eq!(host.seen_now_ns, vec![1_000, 1_025, 1_050]);
    assert_eq!(host.idle_calls, 0);
    assert!(!host_loop.has_work());
}

#[test]
fn embedded_host_loop_repeated_step_with_host_uses_distinct_monotonic_timestamps() {
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 4,
            ..Default::default()
        },
    );
    host_loop
        .try_spawn(CountTask { remaining: 3 })
        .expect("spawn task for repeated step_with_host timestamp test");
    let mut host = AdvancingHost {
        next_now_ns: 2_000,
        step_ns: 10,
        ..Default::default()
    };

    let first = host_loop
        .step_with_host(&mut host, 1, 0, 1)
        .expect("first step_with_host call");
    let second = host_loop
        .step_with_host(&mut host, 1, 0, 1)
        .expect("second step_with_host call");
    let third = host_loop
        .step_with_host(&mut host, 1, 0, 1)
        .expect("third step_with_host call");

    assert_eq!(first.tasks, 1);
    assert_eq!(second.tasks, 1);
    assert_eq!(third.tasks, 1);
    assert_eq!(host.seen_now_ns, vec![2_000, 2_010, 2_020]);
    assert_eq!(host.idle_calls, 0);
    assert!(!host_loop.has_work());
}

#[test]
fn embedded_host_loop_drive_steps_with_host_idle_counting_adapter_tracks_repeated_idle_rounds() {
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 4,
            ..Default::default()
        },
    );
    host_loop
        .try_spawn(CountTask { remaining: 2 })
        .expect("spawn task for repeated idle rounds");
    let mut host = CountingHost {
        now_ns: 900,
        ..Default::default()
    };

    let first_round = host_loop
        .drive_steps_with_host(&mut host, 2, 1, 0, 0)
        .expect("drive first repeated idle round");
    let second_round = host_loop
        .drive_steps_with_host(&mut host, 2, 1, 0, 0)
        .expect("drive second repeated idle round");

    assert_eq!(
        first_round,
        EmbeddedDriveResult {
            steps_attempted: 2,
            tasks_run: 0,
            completions_observed: 0,
            work_remains: true,
        }
    );
    assert_eq!(
        second_round,
        EmbeddedDriveResult {
            steps_attempted: 2,
            tasks_run: 0,
            completions_observed: 0,
            work_remains: true,
        }
    );
    assert_eq!(host.now_calls, 4);
    assert_eq!(host.idle_calls, 4);
    assert!(host_loop.has_work());
}

#[test]
fn embedded_host_loop_drive_steps_with_host_idle_recording_adapter_is_non_blocking_policy_only() {
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 4,
            ..Default::default()
        },
    );
    host_loop
        .try_spawn(CountTask { remaining: 2 })
        .expect("spawn task for idle-recording adapter");
    let mut host = IdleRecordingHost {
        now_ns: 1_500,
        ..Default::default()
    };

    let first_round = host_loop
        .drive_steps_with_host(&mut host, 2, 1, 0, 0)
        .expect("drive first idle-only round");
    let second_round = host_loop
        .drive_steps_with_host(&mut host, 2, 1, 0, 0)
        .expect("drive second idle-only round");

    assert_eq!(
        first_round,
        EmbeddedDriveResult {
            steps_attempted: 2,
            tasks_run: 0,
            completions_observed: 0,
            work_remains: true,
        }
    );
    assert_eq!(
        second_round,
        EmbeddedDriveResult {
            steps_attempted: 2,
            tasks_run: 0,
            completions_observed: 0,
            work_remains: true,
        }
    );
    assert_eq!(host.now_calls, 4);
    assert_eq!(host.idle_notifications, 4);
    assert!(host_loop.has_work());
}

#[test]
fn embedded_host_loop_step_with_host_allows_now_only_safe_target_neutral_adapter() {
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 4,
            ..Default::default()
        },
    );
    host_loop
        .try_spawn(CountTask { remaining: 1 })
        .expect("spawn task for now-only host adapter");
    let mut host = NowOnlyHost {
        now_ns: 2_048,
        ..Default::default()
    };

    let tick = host_loop
        .step_with_host(&mut host, 1, 0, 4)
        .expect("step with now-only host adapter");

    assert_eq!(tick.tasks, 1);
    assert_eq!(host.now_calls, 1);
    assert!(!host_loop.has_work());
}

#[test]
fn embedded_host_adapter_compile_proof_now_only_hook_uses_default_idle() {
    fn assert_embedded_host_adapter<T: EmbeddedHostAdapter>() {}
    assert_embedded_host_adapter::<NowOnlyTraitBoundHost>();
}

#[test]
fn embedded_host_adapter_host_neutral_surface_uses_now_and_idle_contract() {
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 4,
            ..Default::default()
        },
    );
    host_loop
        .try_spawn(CountTask { remaining: 1 })
        .expect("spawn task for host-neutral adapter surface");
    let mut host = CountingHost {
        now_ns: 4_096,
        ..Default::default()
    };

    let tick = host_loop
        .step_with_host(&mut host, 1, 0, 1)
        .expect("step via host-neutral adapter surface");

    assert_eq!(tick.tasks, 1);
    assert_eq!(host.now_calls, 1);
    assert_eq!(host.idle_calls, 0);
    assert!(!host_loop.has_work());
}

#[test]
fn embedded_host_loop_try_arm_embedded_park_surfaces_explicit_exhaustion_error() {
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
        DummyDriver,
        EmbeddedHostLoopConfig {
            task_capacity: 4,
            ..Default::default()
        },
    );
    let slot_count = host_loop.embedded_policy_summary().park_slot_count;

    let first = host_loop
        .try_arm_embedded_park(BalancedWakeToken(1))
        .expect("first embedded lease via host-loop wrapper");
    assert_eq!(first.slot.0, 0);
    for token in 2..=slot_count as u64 {
        host_loop
            .try_arm_embedded_park(BalancedWakeToken(token))
            .expect("embedded host-loop lease within configured slot count");
    }
    assert_eq!(
        host_loop.try_arm_embedded_park(BalancedWakeToken(slot_count as u64 + 1)),
        Err(EmbeddedResourceError::ParkSlotsExhausted)
    );
}

#[test]
fn embedded_host_loop_try_arm_embedded_park_mirrors_runtime_loop_handle_behavior() {
    let config = EmbeddedHostLoopConfig {
        task_capacity: 4,
        ..Default::default()
    };
    let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(DummyDriver, config);
    let mut runtime_handle = RuntimeLoopHandle::new(
        embedded_profile_layout()
            .build_runtime_with_task_capacity::<DummyDriver, CountTask>(DummyDriver, 4),
    );
    let slot_count = host_loop.embedded_policy_summary().park_slot_count;

    let mut host_results = Vec::with_capacity(slot_count + 1);
    let mut runtime_results = Vec::with_capacity(slot_count + 1);

    for token in 1..=(slot_count as u64 + 1) {
        host_results.push(
            host_loop
                .try_arm_embedded_park(BalancedWakeToken(token))
                .map(|lease| lease.slot.0),
        );
        runtime_results.push(
            runtime_handle
                .try_arm_embedded_park(BalancedWakeToken(token))
                .map(|lease| lease.slot.0),
        );
    }

    assert_eq!(host_results, runtime_results);
    assert_eq!(
        host_results.last(),
        Some(&Err(EmbeddedResourceError::ParkSlotsExhausted))
    );
}
