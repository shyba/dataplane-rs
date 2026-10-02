use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use dataplane_core_reactor::local_boundary::LocalShardPublisher;

fn publisher(c: &mut Criterion) {
    let mut group = c.benchmark_group("local_publisher");
    group.throughput(Throughput::Elements(4096));
    group.bench_function("weighted_16_shards", |b| b.iter(|| {
        let mut publisher = LocalShardPublisher::new(16, 64);
        let mut published = 0;
        for i in 0..4096 {
            publisher.push_weighted(i % 16, i, black_box(3), |_, batch| {
                published += batch.len();
                black_box(batch);
            });
        }
        publisher.flush_all(|_, batch| { published += batch.len(); black_box(batch); });
        assert_eq!(published, 4096);
    }));
    group.finish();
}
criterion_group!(benches, publisher);
criterion_main!(benches);
