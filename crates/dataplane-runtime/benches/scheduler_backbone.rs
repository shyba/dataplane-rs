//! Scheduler backbone benchmark.
//!
//! Measures scheduler setup, admission, mesh offload, draining, and teardown.
//! Schedulers are driven sequentially; this is not a multicore scaling test.

use std::sync::Arc;

use criterion::{black_box, criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
use dataplane_core_reactor::local_scheduler::{
    build_shard_mesh, LocalMeshScheduler, SchedulerPlacement, ShardSchedulerConfig, TaskCell,
    TaskMeta, TaskPriority, WorkDisposition,
};

const STACK_BYTES: usize = 256;

type Op = u64;

fn make_placements(shard_count: usize) -> Vec<SchedulerPlacement> {
    let split = shard_count.max(2).div_ceil(2);
    (0..shard_count)
        .map(|shard| SchedulerPlacement {
            shard,
            core_id: shard,
            domain: usize::from(shard >= split),
        })
        .collect()
}

fn run_backbone_distributed(total_ops: usize, shard_count: usize) -> usize {
    let placements = make_placements(shard_count);
    let mesh: Vec<_> = build_shard_mesh::<TaskCell<Op, STACK_BYTES>>(shard_count, 1024)
        .into_iter()
        .enumerate()
        .map(|(idx, ep)| {
            // Create external bus channel pair for each scheduler
            let (tx, rx) = kanal::bounded(1024);
            (idx, ep, tx, rx)
        })
        .collect();

    let cfg = ShardSchedulerConfig {
        local_queue_capacity: total_ops.max(2048),
        overload_soft_limit: 1024,
        ingress_drain_budget: 512,
        ingress_scan_budget: 8,
        ingress_source_budget: 64,
        bus_drain_budget: 512,
        run_budget: 512,
        ..ShardSchedulerConfig::default()
    };

    let mut schedulers: Vec<LocalMeshScheduler<Op, STACK_BYTES>> = mesh
        .into_iter()
        .map(|(idx, ep, tx, rx)| {
            LocalMeshScheduler::new(placements[idx], &placements, ep, tx, Arc::new(rx), cfg)
        })
        .collect();

    // Concentrate ingress on one shard so overload can exercise mesh offload.
    for op in 0..total_ops {
        assert!(
            schedulers[0]
                .submit_work(TaskMeta::global(TaskPriority::Normal), op as Op)
                .is_ok(),
            "benchmark admission failed"
        );
    }

    let mut completed = 0usize;
    let mut rounds = 0usize;
    let max_rounds = total_ops.saturating_mul(4).max(1024);

    while completed < total_ops && rounds < max_rounds {
        let mut progressed = 0usize;
        for scheduler in &mut schedulers {
            scheduler.tick(|op| {
                black_box(*op);
                progressed += 1;
                WorkDisposition::Complete
            });
        }
        completed += progressed;
        if progressed == 0 {
            break;
        }
        rounds += 1;
    }

    assert_eq!(
        completed, total_ops,
        "not all work completed: {completed}/{total_ops}"
    );
    rounds
}

fn benchmark_scheduler_backbone(c: &mut Criterion) {
    let mut group = c.benchmark_group("scheduler_backbone");
    for shards in [1, 2, 4] {
        group.throughput(Throughput::Elements(4096));
        group.bench_with_input(
            BenchmarkId::from_parameter(shards),
            &shards,
            |b, &shards| b.iter(|| run_backbone_distributed(black_box(4096), black_box(shards))),
        );
    }
    group.finish();
}

criterion_group! {
    name = benches;
    config = Criterion::default().sample_size(10);
    targets = benchmark_scheduler_backbone
}
criterion_main!(benches);
