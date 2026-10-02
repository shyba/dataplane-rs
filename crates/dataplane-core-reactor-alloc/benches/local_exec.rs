//! Portable enqueue/drain workloads, including allocation and teardown.
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use dataplane_core_reactor_alloc::{local_exec::LocalExec, local_exec_counts::LocalExecCounts};

fn benches(c: &mut Criterion) {
    const OPS: usize = 4096;
    let mut group = c.benchmark_group("local_exec");
    group.throughput(Throughput::Elements(OPS as u64));
    group.bench_function("items_16_slots", |b| {
        b.iter(|| {
            let mut exec = LocalExec::new(16);
            for i in 0..OPS {
                exec.push(i % 16, i).unwrap();
            }
            assert_eq!(
                exec.drain(OPS, 4, false, |v| {
                    black_box(v);
                }),
                OPS
            );
        })
    });
    group.bench_function("counts_16_slots", |b| {
        b.iter(|| {
            let mut exec = LocalExecCounts::new(16);
            for i in 0..OPS {
                exec.push_count(i % 16, 1).unwrap();
            }
            assert_eq!(
                exec.drain(OPS, 4, false, || {
                    black_box(());
                }),
                OPS
            );
        })
    });
    group.finish();
}
criterion_group!(local, benches);
criterion_main!(local);
