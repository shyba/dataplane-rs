//! Integration tests for embedded_host_loop public API surface.
//!
//! These tests validate the host-neutral embedded runtime boundary using only
//! public APIs and safe Rust. No ESP32, xtensa, or target-specific code.
//!
//! Tests are organized by the integration scenarios from DP-OVN-0136 through
//! DP-OVN-0160: try_spawn capacity, drive_steps_with_config_defaults, idle
//! adapter callback, and monotonic host timestamps.

#![cfg(test)]

use dataplane_core_reactor::native_task::{NativeTask, NativeTaskCx, StepResult};
use dataplane_core_reactor::reactor_driver::{
    DriverBackendKind, DriverCapabilities, ReactorDriver, ReactorDriverWait,
};
use dataplane_core_reactor::reactor_model::{NetEvent, OpToken};
use dataplane_runtime::embedded_host_loop::{
    EmbeddedDriveConfig, EmbeddedHostAdapter, EmbeddedHostLoop, EmbeddedHostLoopConfig,
    EmbeddedPolicySummary,
};

// ---------------------------------------------------------------------------
// Test-only driver and host adapters
// ---------------------------------------------------------------------------

#[derive(Default)]
struct DummyDriver;

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

struct CountTask {
    remaining: usize,
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

/// Host adapter that records idle notifications but does not block.
#[derive(Default)]
struct IdleRecordingHost {
    now_ns: u64,
    now_calls: usize,
    idle_notifications: usize,
}

impl EmbeddedHostAdapter for IdleRecordingHost {
    fn now_ns(&mut self) -> u64 {
        self.now_calls += 1;
        self.now_ns
    }

    fn on_idle(&mut self) {
        self.idle_notifications += 1;
    }
}

/// Host adapter that advances monotonic timestamps by a fixed step.
struct AdvancingHost {
    next_now_ns: u64,
    step_ns: u64,
    seen_now_ns: Vec<u64>,
}

impl AdvancingHost {
    fn new(start: u64, step: u64) -> Self {
        Self {
            next_now_ns: start,
            step_ns: step,
            seen_now_ns: Vec::new(),
        }
    }
}

impl EmbeddedHostAdapter for AdvancingHost {
    fn now_ns(&mut self) -> u64 {
        let now = self.next_now_ns;
        self.seen_now_ns.push(now);
        self.next_now_ns = self.next_now_ns.saturating_add(self.step_ns);
        now
    }
}

/// Host adapter that only provides monotonic time, no idle callback.
struct NowOnlyHost {
    now_ns: u64,
    now_calls: usize,
}

impl NowOnlyHost {
    fn new(now_ns: u64) -> Self {
        Self {
            now_ns,
            now_calls: 0,
        }
    }
}

impl EmbeddedHostAdapter for NowOnlyHost {
    fn now_ns(&mut self) -> u64 {
        self.now_calls += 1;
        self.now_ns
    }
}

// --------------------------------------------------------------------------
// Integration tests: boundary edge cases (DP-NB-0161, DP-NB-0162, DP-NB-0163)
// --------------------------------------------------------------------------

mod boundary_edge_cases {
    use super::*;

    #[test]
    fn integration_zero_step_limit_returns_no_progress_without_panic() {
        // DP-NB-0161: prove zero step_limit returns no progress without panic
        let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
            DummyDriver,
            EmbeddedHostLoopConfig {
                task_capacity: 4,
                ..Default::default()
            },
        );

        host_loop
            .try_spawn(CountTask { remaining: 3 })
            .expect("spawn task");

        let mut host = NowOnlyHost::new(100);
        // step_limit = 0: while loop never executes, no progress
        let result = host_loop
            .drive_steps_with_host(&mut host, 0, 1, 0, 1)
            .expect("drive with zero step_limit should not panic");

        assert_eq!(result.steps_attempted, 0);
        assert_eq!(result.tasks_run, 0);
        assert!(
            result.work_remains,
            "work should remain after zero-step drive"
        );
        assert!(
            host_loop.has_work(),
            "has_work should be true after zero-step drive"
        );
    }

    #[test]
    fn integration_max_events_zero_is_handled_gracefully() {
        // DP-NB-0162: prove max_events zero is handled by existing behavior
        let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
            DummyDriver,
            EmbeddedHostLoopConfig {
                task_capacity: 4,
                ..Default::default()
            },
        );

        host_loop
            .try_spawn(CountTask { remaining: 2 })
            .expect("spawn task");

        let mut host = NowOnlyHost::new(100);
        // Drive with max_events = 0 - should not panic
        let result = host_loop
            .drive_steps_with_host(&mut host, 2, 0, 0, 1)
            .expect("drive with max_events=0 should not panic");

        // With max_events=0, no events can be processed but tasks may still run
        assert_eq!(result.steps_attempted, 2, "steps should be attempted");
        assert_eq!(result.tasks_run, 2, "tasks run despite max_events=0");
    }

    #[test]
    fn integration_task_budget_zero_idle_fires_without_panic() {
        // DP-NB-0163: prove task_budget zero triggers idle without panic
        let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
            DummyDriver,
            EmbeddedHostLoopConfig {
                task_capacity: 4,
                ..Default::default()
            },
        );

        host_loop
            .try_spawn(CountTask { remaining: 2 })
            .expect("spawn task");

        let mut host = IdleRecordingHost::default();

        // task_budget = 0 means no tasks can run, idle should fire
        let result = host_loop
            .drive_steps_with_host(&mut host, 2, 1, 0, 0)
            .expect("drive with task_budget=0 should not panic");

        assert_eq!(result.steps_attempted, 2, "steps should still be attempted");
        assert_eq!(result.tasks_run, 0, "no tasks should run with zero budget");
        assert_eq!(
            host.idle_notifications, 2,
            "idle should fire on each no-progress step"
        );
    }
}

// --------------------------------------------------------------------------
// Integration tests: try_spawn capacity (DP-OVN-0138)
// --------------------------------------------------------------------------

mod try_spawn_capacity {
    use super::*;

    #[test]
    fn integration_try_spawn_respects_capacity_limit() {
        let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
            DummyDriver,
            EmbeddedHostLoopConfig {
                task_capacity: 2,
                ..Default::default()
            },
        );

        // First two spawns succeed
        host_loop
            .try_spawn(CountTask { remaining: 1 })
            .expect("first spawn should succeed");
        host_loop
            .try_spawn(CountTask { remaining: 1 })
            .expect("second spawn should succeed");

        // Third spawn fails with capacity error
        let err = host_loop
            .try_spawn(CountTask { remaining: 1 })
            .expect_err("capacity exhausted should return error");
        assert_eq!(err.active(), 2);
        assert_eq!(err.max_slots(), 2);
    }

    #[test]
    fn integration_try_spawn_capacity_error_preserves_original_task() {
        let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
            DummyDriver,
            EmbeddedHostLoopConfig {
                task_capacity: 1,
                ..Default::default()
            },
        );

        host_loop
            .try_spawn(CountTask { remaining: 1 })
            .expect("first spawn should succeed");

        let err = host_loop
            .try_spawn(CountTask { remaining: 42 })
            .expect_err("capacity exhaustion should return error");

        // Original task is preserved in error
        let original = err.into_task();
        assert_eq!(original.remaining, 42);
    }

    #[test]
    fn integration_try_spawn_capacity_exhaustion_does_not_panic() {
        let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
            DummyDriver,
            EmbeddedHostLoopConfig {
                task_capacity: 1,
                ..Default::default()
            },
        );

        host_loop
            .try_spawn(CountTask { remaining: 1 })
            .expect("first spawn should succeed");

        // Panic would be caught, but try_spawn is fallible so no panic should occur
        let result = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
            host_loop.try_spawn(CountTask { remaining: 77 })
        }));

        assert!(
            result.is_ok(),
            "try_spawn should not panic on capacity exhaustion"
        );
        let err = result
            .expect("should have result")
            .expect_err("should be error");
        assert_eq!(err.into_task().remaining, 77);
    }
}

// ---------------------------------------------------------------------------
// Integration tests: drive_steps_with_config_defaults (DP-OVN-0139)
// ---------------------------------------------------------------------------

mod drive_steps_with_config_defaults {
    use super::*;

    #[test]
    fn integration_drive_steps_with_config_defaults_stops_at_step_limit() {
        let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
            DummyDriver,
            EmbeddedHostLoopConfig {
                task_capacity: 4,
                drive: EmbeddedDriveConfig {
                    step_limit: 2, // Only 2 steps even though task needs 3
                    max_events: 1,
                    min_events: 0,
                    task_budget: 1,
                },
            },
        );

        host_loop
            .try_spawn(CountTask { remaining: 3 })
            .expect("spawn multi-step task");

        let mut host = NowOnlyHost::new(100);
        let result = host_loop
            .drive_steps_with_config_defaults(&mut host)
            .expect("drive with config defaults");

        // Step limit reached with work remaining
        assert_eq!(result.steps_attempted, 2);
        assert_eq!(result.tasks_run, 2);
        assert!(result.work_remains, "task not complete, work should remain");
        assert!(host_loop.has_work());
    }

    #[test]
    fn integration_drive_steps_with_config_defaults_completes_within_limit() {
        let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
            DummyDriver,
            EmbeddedHostLoopConfig {
                task_capacity: 4,
                drive: EmbeddedDriveConfig {
                    step_limit: 5, // Enough steps to complete task
                    max_events: 1,
                    min_events: 0,
                    task_budget: 1,
                },
            },
        );

        host_loop
            .try_spawn(CountTask { remaining: 2 })
            .expect("spawn two-step task");

        let mut host = NowOnlyHost::new(200);
        let result = host_loop
            .drive_steps_with_config_defaults(&mut host)
            .expect("drive with config defaults");

        assert_eq!(result.steps_attempted, 2);
        assert_eq!(result.tasks_run, 2);
        assert!(!result.work_remains, "task complete, no work should remain");
        assert!(!host_loop.has_work());
    }

    #[test]
    fn integration_drive_steps_with_config_defaults_uses_configured_values() {
        let config = EmbeddedHostLoopConfig {
            task_capacity: 4,
            drive: EmbeddedDriveConfig {
                step_limit: 3,
                max_events: 2,
                min_events: 1,
                task_budget: 2,
            },
        };

        let host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(DummyDriver, config);

        // Verify config is stored and used
        assert_eq!(host_loop.config().drive.step_limit, 3);
        assert_eq!(host_loop.config().drive.max_events, 2);
        assert_eq!(host_loop.config().drive.min_events, 1);
        assert_eq!(host_loop.config().drive.task_budget, 2);
    }
}

// ---------------------------------------------------------------------------
// Integration tests: idle adapter callback (DP-OVN-0140)
// ---------------------------------------------------------------------------

mod idle_adapter_callback {
    use super::*;

    #[test]
    fn integration_idle_callback_fires_on_no_progress_steps() {
        let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
            DummyDriver,
            EmbeddedHostLoopConfig {
                task_capacity: 4,
                ..Default::default()
            },
        );

        // Spawn a task that needs 2 steps
        host_loop
            .try_spawn(CountTask { remaining: 2 })
            .expect("spawn task");

        let mut host = IdleRecordingHost::default();

        // First step: task runs, no idle
        let first = host_loop
            .drive_steps_with_host(&mut host, 1, 1, 0, 1)
            .expect("first drive");
        assert_eq!(first.tasks_run, 1);
        assert_eq!(host.idle_notifications, 0);

        // Now drive with task_budget=0 so no task runs - idle fires
        let second = host_loop
            .drive_steps_with_host(&mut host, 1, 1, 0, 0)
            .expect("second drive (no progress)");
        assert_eq!(second.tasks_run, 0);
        assert_eq!(
            host.idle_notifications, 1,
            "idle should fire on no-progress step"
        );

        // Another no-progress step
        let third = host_loop
            .drive_steps_with_host(&mut host, 1, 1, 0, 0)
            .expect("third drive (no progress)");
        assert_eq!(third.tasks_run, 0);
        assert_eq!(
            host.idle_notifications, 2,
            "idle should fire on each no-progress step"
        );
    }

    #[test]
    fn integration_idle_callback_not_fired_when_task_runs() {
        let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
            DummyDriver,
            EmbeddedHostLoopConfig {
                task_capacity: 4,
                ..Default::default()
            },
        );

        host_loop
            .try_spawn(CountTask { remaining: 1 })
            .expect("spawn task");

        let mut host = IdleRecordingHost::default();
        let result = host_loop
            .drive_steps_with_host(&mut host, 3, 1, 0, 1)
            .expect("drive steps with task");

        assert_eq!(result.steps_attempted, 1);
        assert_eq!(result.tasks_run, 1);
        assert_eq!(
            host.idle_notifications, 0,
            "idle should not fire when task runs"
        );
    }

    #[test]
    fn integration_idle_callback_fires_only_on_no_progress_steps_mixed_workload() {
        let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
            DummyDriver,
            EmbeddedHostLoopConfig {
                task_capacity: 4,
                ..Default::default()
            },
        );

        // Spawn a task that needs 2 steps
        host_loop
            .try_spawn(CountTask { remaining: 2 })
            .expect("spawn task");

        let mut host = IdleRecordingHost::default();

        // First step: task runs, no idle
        let first = host_loop
            .drive_steps_with_host(&mut host, 1, 1, 0, 1)
            .expect("first drive");
        assert_eq!(first.tasks_run, 1);
        assert_eq!(host.idle_notifications, 0);

        // Now task is pending but needs another step
        // Drive with task_budget=0 so no task runs - should get idle
        let second = host_loop
            .drive_steps_with_host(&mut host, 1, 1, 0, 0)
            .expect("second drive");
        assert_eq!(second.tasks_run, 0);
        assert_eq!(host.idle_notifications, 1);
    }
}

// ---------------------------------------------------------------------------
// Integration tests: monotonic host timestamps (DP-OVN-0141)
// ---------------------------------------------------------------------------

mod monotonic_host_timestamps {
    use super::*;

    #[test]
    fn integration_step_with_host_consumes_distinct_monotonic_timestamps() {
        let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
            DummyDriver,
            EmbeddedHostLoopConfig {
                task_capacity: 4,
                ..Default::default()
            },
        );

        host_loop
            .try_spawn(CountTask { remaining: 3 })
            .expect("spawn task");

        let mut host = AdvancingHost::new(1_000, 25);

        let first = host_loop
            .step_with_host(&mut host, 1, 0, 1)
            .expect("first step");
        let second = host_loop
            .step_with_host(&mut host, 1, 0, 1)
            .expect("second step");
        let third = host_loop
            .step_with_host(&mut host, 1, 0, 1)
            .expect("third step");

        assert_eq!(first.tasks, 1);
        assert_eq!(second.tasks, 1);
        assert_eq!(third.tasks, 1);

        // Verify distinct timestamps were consumed
        assert_eq!(host.seen_now_ns, vec![1_000, 1_025, 1_050]);
    }

    #[test]
    fn integration_drive_with_host_uses_monotonic_timestamps() {
        let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
            DummyDriver,
            EmbeddedHostLoopConfig {
                task_capacity: 4,
                ..Default::default()
            },
        );

        host_loop
            .try_spawn(CountTask { remaining: 3 })
            .expect("spawn task");

        let mut host = AdvancingHost::new(5_000, 100);

        let result = host_loop
            .drive_steps_with_host(&mut host, 3, 1, 0, 1)
            .expect("drive steps");

        assert_eq!(result.steps_attempted, 3);
        assert_eq!(result.tasks_run, 3);
        assert_eq!(host.seen_now_ns, vec![5_000, 5_100, 5_200]);
    }

    #[test]
    fn integration_repeated_drive_calls_preserve_timestamp_advancement() {
        let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
            DummyDriver,
            EmbeddedHostLoopConfig {
                task_capacity: 4,
                ..Default::default()
            },
        );

        host_loop
            .try_spawn(CountTask { remaining: 2 })
            .expect("spawn task");

        let mut host = AdvancingHost::new(10_000, 50);

        let first_round = host_loop
            .drive_steps_with_host(&mut host, 2, 1, 0, 1)
            .expect("first drive round");
        assert_eq!(first_round.steps_attempted, 2);
        assert_eq!(host.seen_now_ns, vec![10_000, 10_050]);

        // Second round continues from where first round left off
        host_loop
            .try_spawn(CountTask { remaining: 2 })
            .expect("spawn second task");

        let second_round = host_loop
            .drive_steps_with_host(&mut host, 2, 1, 0, 1)
            .expect("second drive round");
        assert_eq!(second_round.steps_attempted, 2);
        assert_eq!(host.seen_now_ns, vec![10_000, 10_050, 10_100, 10_150]);
    }
}

// ---------------------------------------------------------------------------
// Integration tests: EmbeddedPolicySummary inspection (DP-OVN-0154)
// ---------------------------------------------------------------------------

mod embedded_policy_summary {
    use super::*;

    #[test]
    fn integration_policy_summary_reflects_configuration() {
        let config = EmbeddedHostLoopConfig {
            task_capacity: 7,
            ..Default::default()
        };

        let host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(DummyDriver, config);
        let summary = host_loop.embedded_policy_summary();

        assert_eq!(summary.task_capacity, 7);
        assert!(summary.task_budget > 0);
        assert!(summary.completion_budget > 0);
        assert!(summary.ingress_budget > 0);
        assert!(summary.park_slot_count > 0);
    }

    #[test]
    fn integration_policy_summary_from_policy_matches_runtime() {
        let config = EmbeddedHostLoopConfig {
            task_capacity: 5,
            ..Default::default()
        };

        let host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(DummyDriver, config);
        let policy = host_loop.embedded_policy();

        let summary = EmbeddedPolicySummary::from_policy(config.task_capacity, policy);

        assert_eq!(summary.task_capacity, config.task_capacity);
        assert_eq!(summary.task_budget, policy.budgets.task_budget);
        assert_eq!(summary.completion_budget, policy.budgets.completion_budget);
        assert_eq!(summary.ingress_budget, policy.budgets.ingress_budget);
    }
}

// ---------------------------------------------------------------------------
// Integration tests: EmbeddedDriveResult semantics (DP-OVN-0153)
// ---------------------------------------------------------------------------

mod embedded_drive_result {
    use super::*;

    #[test]
    fn integration_drive_result_reflects_step_outcomes() {
        let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
            DummyDriver,
            EmbeddedHostLoopConfig {
                task_capacity: 4,
                ..Default::default()
            },
        );

        host_loop
            .try_spawn(CountTask { remaining: 2 })
            .expect("spawn task");

        let mut host = NowOnlyHost::new(100);
        let result = host_loop
            .drive_steps_with_host(&mut host, 5, 1, 0, 1)
            .expect("drive to completion");

        assert_eq!(result.steps_attempted, 2);
        assert_eq!(result.tasks_run, 2);
        assert!(!result.work_remains);
    }

    #[test]
    fn integration_drive_result_work_remains_when_step_limit_reached() {
        let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
            DummyDriver,
            EmbeddedHostLoopConfig {
                task_capacity: 4,
                ..Default::default()
            },
        );

        host_loop
            .try_spawn(CountTask { remaining: 5 })
            .expect("spawn long task");

        let mut host = NowOnlyHost::new(100);
        let result = host_loop
            .drive_steps_with_host(&mut host, 2, 1, 0, 1)
            .expect("drive with limit");

        assert_eq!(result.steps_attempted, 2);
        assert_eq!(result.tasks_run, 2);
        assert!(result.work_remains);
    }

    #[test]
    fn integration_drive_result_zero_steps_when_no_work() {
        let mut host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(
            DummyDriver,
            EmbeddedHostLoopConfig {
                task_capacity: 4,
                ..Default::default()
            },
        );

        // No tasks spawned
        let mut host = NowOnlyHost::new(100);
        let result = host_loop
            .drive_steps_with_host(&mut host, 5, 1, 0, 1)
            .expect("drive with no work");

        assert_eq!(result.steps_attempted, 0);
        assert_eq!(result.tasks_run, 0);
        assert!(!result.work_remains);
    }
}

// ---------------------------------------------------------------------------
// Integration tests: EmbeddedDriveConfig docs (DP-OVN-0152)
// ---------------------------------------------------------------------------

mod embedded_drive_config {
    use super::*;

    #[test]
    fn integration_drive_config_default_values() {
        let config = EmbeddedDriveConfig::default();

        assert_eq!(config.step_limit, 1);
        assert_eq!(config.max_events, 1);
        assert_eq!(config.min_events, 0);
        assert_eq!(config.task_budget, 1);
    }

    #[test]
    fn integration_drive_config_custom_values() {
        let config = EmbeddedDriveConfig {
            step_limit: 10,
            max_events: 5,
            min_events: 2,
            task_budget: 3,
        };

        assert_eq!(config.step_limit, 10);
        assert_eq!(config.max_events, 5);
        assert_eq!(config.min_events, 2);
        assert_eq!(config.task_budget, 3);
    }
}

// ---------------------------------------------------------------------------
// Integration tests: EmbeddedHostLoopConfig task_capacity (DP-OVN-0137)
// ---------------------------------------------------------------------------

mod embedded_host_loop_config {
    use super::*;

    #[test]
    fn integration_host_loop_config_default_capacity() {
        let config = EmbeddedHostLoopConfig::default();
        assert_eq!(config.task_capacity(), 64);
    }

    #[test]
    fn integration_host_loop_config_custom_capacity() {
        let config = EmbeddedHostLoopConfig {
            task_capacity: 16,
            ..Default::default()
        };
        assert_eq!(config.task_capacity(), 16);
    }

    #[test]
    fn integration_host_loop_new_example_from_docs() {
        // Replicate the dummy-driver construction example from embedded_host_loop docs
        let config = EmbeddedHostLoopConfig::default();
        let host_loop = EmbeddedHostLoop::<DummyDriver, CountTask>::new(DummyDriver, config);
        assert_eq!(host_loop.task_capacity(), 64);
    }
}
