use super::*;
use std::sync::{
    atomic::{AtomicUsize, Ordering},
    Arc,
};

fn sample_placements() -> Vec<SchedulerPlacement> {
    vec![
        SchedulerPlacement {
            shard: 0,
            core_id: 0,
            domain: 0,
        },
        SchedulerPlacement {
            shard: 1,
            core_id: 1,
            domain: 0,
        },
        SchedulerPlacement {
            shard: 2,
            core_id: 8,
            domain: 1,
        },
        SchedulerPlacement {
            shard: 3,
            core_id: 9,
            domain: 1,
        },
    ]
}

#[test]
fn route_prefers_local_domain_before_remote() {
    let placements = sample_placements();
    let mesh = build_shard_mesh::<TaskCell<u64, 256>>(4, 8).remove(0);
    let (bus_tx, bus_rx) = kanal::unbounded::<TaskCell<u64, 256>>();
    let bus_rx = Arc::new(bus_rx);
    let scheduler = LocalMeshScheduler::<u64, 256>::new(
        placements[0],
        &placements,
        mesh,
        bus_tx,
        bus_rx,
        ShardSchedulerConfig::default(),
    );
    assert_eq!(scheduler.route(), &[1, 2, 3]);
}

#[test]
fn domain_scope_offloads_within_domain() {
    let placements = sample_placements();
    let mut mesh = build_shard_mesh::<TaskCell<u64, 256>>(4, 8);
    let (bus_tx, bus_rx) = kanal::unbounded::<TaskCell<u64, 256>>();
    let bus_rx = Arc::new(bus_rx);

    let cfg = ShardSchedulerConfig {
        local_queue_capacity: 0,
        overload_soft_limit: 0,
        ..ShardSchedulerConfig::default()
    };

    let mut s0 = LocalMeshScheduler::<u64, 256>::new(
        placements[0],
        &placements,
        mesh.remove(0),
        bus_tx.clone(),
        bus_rx.clone(),
        cfg,
    );
    let mut s1 = LocalMeshScheduler::<u64, 256>::new(
        placements[1],
        &placements,
        mesh.remove(0),
        bus_tx,
        bus_rx.clone(),
        ShardSchedulerConfig::default(),
    );

    let placement = match s0.submit_work(TaskMeta::domain(TaskPriority::Normal), 41) {
        Ok(v) => v,
        Err(_) => panic!("submit failed"),
    };
    assert_eq!(placement, SubmitPlacement::Offloaded(1));

    let mut seen = Vec::new();
    let _ = s1.tick(|value| {
        seen.push(*value);
        WorkDisposition::AllDone
    });

    assert_eq!(seen, vec![41]);
}

#[test]
fn mesh_zero_spill_reports_full_when_ring_is_full() {
    let mut mesh = build_shard_mesh::<u64>(2, 1);
    assert_eq!(mesh[0].try_push_admit(1, 10), Ok(()));

    let err = mesh[0].try_push_admit(1, 11).unwrap_err();
    assert_eq!(err.into_inner(), 11);
}

#[test]
fn mesh_spill_capacity_is_hard_limit() {
    let mut mesh = build_shard_mesh_with_spill::<u64>(2, 1, 1);
    assert_eq!(mesh[0].try_push_admit(1, 10), Ok(()));
    assert_eq!(mesh[0].try_push_admit(1, 11), Ok(()));

    let err = mesh[0].try_push_admit(1, 12).unwrap_err();
    assert_eq!(err.into_inner(), 12);
}

#[test]
fn low_priority_local_task_falls_back_to_bus_when_full() {
    let placements = sample_placements();
    let mesh = build_shard_mesh::<TaskCell<u64, 256>>(4, 8).remove(0);
    let (bus_tx, bus_rx) = kanal::unbounded::<TaskCell<u64, 256>>();
    let bus_rx = Arc::new(bus_rx);

    let mut scheduler = LocalMeshScheduler::<u64, 256>::new(
        placements[0],
        &placements,
        mesh,
        bus_tx.clone(),
        bus_rx.clone(),
        ShardSchedulerConfig {
            local_queue_capacity: 0,
            overload_soft_limit: 0,
            ..ShardSchedulerConfig::default()
        },
    );

    let placement = match scheduler.submit_work(TaskMeta::local(TaskPriority::Low), 7) {
        Ok(v) => v,
        Err(_) => panic!("submit failed"),
    };
    assert_eq!(placement, SubmitPlacement::BusDeferred);
    assert!(bus_rx.try_recv().ok().flatten().is_some());
}

#[test]
fn async_task_runs_via_boxed_future() {
    let placements = sample_placements();
    let mesh = build_shard_mesh::<TaskCell<u64, 256>>(4, 8).remove(0);
    let (bus_tx, bus_rx) = kanal::unbounded::<TaskCell<u64, 256>>();
    let bus_rx = Arc::new(bus_rx);
    let mut scheduler = LocalMeshScheduler::<u64, 256>::new(
        placements[0],
        &placements,
        mesh,
        bus_tx,
        bus_rx,
        ShardSchedulerConfig::default(),
    );

    let hits = Arc::new(AtomicUsize::new(0));
    let hits_clone = hits.clone();

    let submit = scheduler.submit_async(TaskMeta::local(TaskPriority::Normal), async move {
        hits_clone.fetch_add(1, Ordering::Relaxed);
    });
    assert!(submit.is_ok(), "submit failed");

    let _ = scheduler.tick(|_| WorkDisposition::AllDone);
    assert_eq!(hits.load(Ordering::Relaxed), 1);
}

#[cfg(feature = "trace-stamps")]
#[test]
fn trace_ring_overwrites_oldest_stamp() {
    let clock = Clock::new();
    let mut task = TaskCell::<u64, 64>::new_work(TaskMeta::global(TaskPriority::Normal), 1);

    for _ in 0..(TRACE_STAMP_DEPTH + 3) {
        task.stamp(&clock, TraceStep::Dispatch);
    }

    let stamps = task.trace_snapshot().unwrap();
    let non_zero = stamps.iter().filter(|stamp| stamp.ticks != 0).count();
    assert_eq!(non_zero, TRACE_STAMP_DEPTH);
}
