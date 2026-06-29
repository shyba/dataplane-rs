use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use dataplane_runtime::runtime_scheduler::{
    FifoScheduler, ScheduledItem, SchedulerPolicy, SharesScheduler,
};

#[derive(Clone, Copy)]
enum WorkKind {
    Ready,
    WriteReady,
}

fn make_mixed_workload(len: usize, write_pct: u32) -> Vec<WorkKind> {
    let mut out = Vec::with_capacity(len);
    let mut state: u64 = 0x9E37_79B9_7F4A_7C15;
    for _ in 0..len {
        state = state
            .wrapping_mul(6364136223846793005)
            .wrapping_add(1442695040888963407);
        let p = (state >> 32) as u32 % 100;
        if p < write_pct {
            out.push(WorkKind::WriteReady);
        } else {
            out.push(WorkKind::Ready);
        }
    }
    out
}

fn run_fifo(workload: &[WorkKind]) -> usize {
    let mut scheduler = FifoScheduler::<u32, u32>::new();
    for (idx, kind) in workload.iter().enumerate() {
        match kind {
            WorkKind::Ready => scheduler.push_ready(idx as u32),
            WorkKind::WriteReady => scheduler.push_write_ready(idx as u32),
        }
    }

    let mut drained = 0usize;
    while let Some(item) = scheduler.pop_next() {
        match item {
            ScheduledItem::Ready(v) | ScheduledItem::WriteReady(v) => {
                black_box(v);
            }
        }
        drained += 1;
    }
    drained
}

fn run_shares(workload: &[WorkKind], write_burst: usize) -> usize {
    let mut scheduler = SharesScheduler::<u32, u32>::new(write_burst);
    for (idx, kind) in workload.iter().enumerate() {
        match kind {
            WorkKind::Ready => scheduler.push_ready(idx as u32),
            WorkKind::WriteReady => scheduler.push_write_ready(idx as u32),
        }
    }

    let mut drained = 0usize;
    while let Some(item) = scheduler.pop_next() {
        match item {
            ScheduledItem::Ready(v) | ScheduledItem::WriteReady(v) => {
                black_box(v);
            }
        }
        drained += 1;
    }
    drained
}

fn bench_scheduler_hot_path(c: &mut Criterion) {
    let total_ops = 16_384usize;
    let workload_balanced = make_mixed_workload(total_ops, 50);
    let workload_write_heavy = make_mixed_workload(total_ops, 80);

    let mut group = c.benchmark_group("scheduler_hot_path_mixed");
    group.throughput(Throughput::Elements(total_ops as u64));

    group.bench_function("fifo_50_50", |b| {
        b.iter(|| {
            let drained = run_fifo(&workload_balanced);
            assert_eq!(drained, workload_balanced.len());
            black_box(drained);
        });
    });

    group.bench_function("shares_50_50_burst4", |b| {
        b.iter(|| {
            let drained = run_shares(&workload_balanced, 4);
            assert_eq!(drained, workload_balanced.len());
            black_box(drained);
        });
    });

    group.bench_function("fifo_20_80", |b| {
        b.iter(|| {
            let drained = run_fifo(&workload_write_heavy);
            assert_eq!(drained, workload_write_heavy.len());
            black_box(drained);
        });
    });

    group.bench_function("shares_20_80_burst4", |b| {
        b.iter(|| {
            let drained = run_shares(&workload_write_heavy, 4);
            assert_eq!(drained, workload_write_heavy.len());
            black_box(drained);
        });
    });

    group.finish();
}

criterion_group!(benches, bench_scheduler_hot_path);
criterion_main!(benches);
