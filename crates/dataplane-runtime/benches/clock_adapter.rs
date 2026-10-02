//! Host-only arithmetic benchmark; no ESP32 hardware or cross toolchain required.
use criterion::{black_box, criterion_group, criterion_main, Criterion, Throughput};
use dataplane_runtime::esp32::Esp32ClockAdapter;

fn clock(c: &mut Criterion) {
    let mut group = c.benchmark_group("clock_adapter");
    group.throughput(Throughput::Elements(4096));
    group.bench_function("samples", |b| {
        b.iter(|| {
            let mut clock = Esp32ClockAdapter::from_systimer(black_box(16_000_000), 0).unwrap();
            for tick in 1..=4096 {
                black_box(clock.observe_systimer_tick(black_box(tick)).unwrap());
            }
        });
    });
    group.finish();
}
criterion_group!(benches, clock);
criterion_main!(benches);
