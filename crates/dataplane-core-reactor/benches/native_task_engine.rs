use criterion::{
    black_box, criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput,
};
use dataplane_core_reactor::native_task::{NativeTask, NativeTaskCx, NativeTaskEngine, StepResult};

#[derive(Clone, Copy)]
struct LeafTask {
    remaining: usize,
    salt: u64,
}

#[derive(Clone, Copy)]
struct FlooderTask {
    remaining: usize,
    yields: usize,
    burst: usize,
    salt: u64,
}

#[derive(Clone, Copy)]
enum BenchTask {
    Flooder(FlooderTask),
    Leaf(LeafTask),
}

impl NativeTask for BenchTask {
    fn step(&mut self, cx: &mut NativeTaskCx<Self>) -> StepResult {
        match self {
            Self::Leaf(task) => {
                black_box(task.salt);
                if task.remaining == 0 {
                    StepResult::Complete
                } else {
                    task.remaining -= 1;
                    StepResult::Ready
                }
            }
            Self::Flooder(task) => {
                let batch = task.remaining.min(task.burst.max(1));
                let mut i = 0usize;
                while i < batch {
                    let salt = task.salt.wrapping_add(i as u64);
                    cx.spawn(Self::Leaf(LeafTask {
                        remaining: task.yields,
                        salt,
                    }));
                    i += 1;
                }
                task.remaining -= batch;
                task.salt = task.salt.wrapping_add(batch as u64);
                if task.remaining == 0 {
                    StepResult::Complete
                } else {
                    StepResult::Ready
                }
            }
        }
    }
}

fn run_flooder(total_tasks: usize, yields: usize, burst: usize) -> usize {
    let mut engine = NativeTaskEngine::with_task_capacity(total_tasks + 16);
    engine.spawn(BenchTask::Flooder(FlooderTask {
        remaining: total_tasks,
        yields,
        burst,
        salt: 0,
    }));
    let progressed = engine.run_until_idle();
    assert_eq!(engine.active_tasks(), 0);
    progressed
}

fn run_preloaded(total_tasks: usize, yields: usize) -> usize {
    let mut engine = NativeTaskEngine::with_task_capacity(total_tasks + 16);
    let mut i = 0usize;
    while i < total_tasks {
        engine.spawn(BenchTask::Leaf(LeafTask {
            remaining: yields,
            salt: i as u64,
        }));
        i += 1;
    }
    let progressed = engine.run_until_idle();
    assert_eq!(engine.active_tasks(), 0);
    progressed
}

fn run_preloaded_hot<const HOT: usize>(total_tasks: usize, yields: usize) -> usize {
    let mut engine = NativeTaskEngine::with_task_capacity(total_tasks + 16);
    let mut i = 0usize;
    while i < total_tasks {
        engine.spawn(BenchTask::Leaf(LeafTask {
            remaining: yields,
            salt: i as u64,
        }));
        i += 1;
    }
    let progressed = engine.run_hot_until_idle::<HOT>();
    assert_eq!(engine.active_tasks(), 0);
    progressed
}

fn make_preloaded_engine(total_tasks: usize, yields: usize) -> NativeTaskEngine<BenchTask> {
    let mut engine = NativeTaskEngine::with_task_capacity(total_tasks + 16);
    let mut i = 0usize;
    while i < total_tasks {
        engine.spawn(BenchTask::Leaf(LeafTask {
            remaining: yields,
            salt: i as u64,
        }));
        i += 1;
    }
    engine
}

fn bench_native_task_engine(c: &mut Criterion) {
    let mut group = c.benchmark_group("core/native_task_engine_flooder");

    for &(tasks, yields, burst) in &[
        (4_096usize, 0usize, 64usize),
        (4_096usize, 1usize, 64usize),
        (16_384usize, 0usize, 64usize),
    ] {
        let total_steps = tasks * (yields + 1) + tasks.div_ceil(burst.max(1));
        group.throughput(Throughput::Elements(total_steps as u64));
        group.bench_with_input(
            BenchmarkId::new(
                format!("tasks{}_yields{}_burst{}", tasks, yields, burst),
                total_steps,
            ),
            &total_steps,
            |b, &_ops| {
                b.iter(|| {
                    let progressed = run_flooder(tasks, yields, burst);
                    black_box(progressed);
                });
            },
        );
    }

    group.finish();
}

fn bench_native_task_engine_preloaded(c: &mut Criterion) {
    let mut group = c.benchmark_group("core/native_task_engine_preloaded");

    for &(tasks, yields) in &[
        (4_096usize, 0usize),
        (4_096usize, 1usize),
        (16_384usize, 0usize),
        (16_384usize, 1usize),
    ] {
        let total_steps = tasks * (yields + 1);
        group.throughput(Throughput::Elements(total_steps as u64));
        group.bench_with_input(
            BenchmarkId::new(format!("tasks{}_yields{}", tasks, yields), total_steps),
            &total_steps,
            |b, &_ops| {
                b.iter(|| {
                    let progressed = run_preloaded(tasks, yields);
                    black_box(progressed);
                });
            },
        );
    }

    group.finish();
}

fn bench_native_task_engine_preloaded_hot(c: &mut Criterion) {
    let mut group = c.benchmark_group("core/native_task_engine_preloaded_hot");

    for &(tasks, yields) in &[(4_096usize, 1usize), (16_384usize, 1usize)] {
        let total_steps = tasks * (yields + 1);

        macro_rules! bench_hot {
            ($hot:expr) => {{
                group.throughput(Throughput::Elements(total_steps as u64));
                group.bench_with_input(
                    BenchmarkId::new(
                        format!("tasks{}_yields{}_hot{}", tasks, yields, $hot),
                        total_steps,
                    ),
                    &total_steps,
                    |b, &_ops| {
                        b.iter(|| {
                            let progressed = run_preloaded_hot::<$hot>(tasks, yields);
                            black_box(progressed);
                        });
                    },
                );
            }};
        }

        bench_hot!(1);
        bench_hot!(2);
        bench_hot!(4);
        bench_hot!(8);
        bench_hot!(16);
        bench_hot!(32);
        bench_hot!(64);
        bench_hot!(128);
    }

    group.finish();
}

fn bench_native_task_engine_preloaded_hot_exec_only(c: &mut Criterion) {
    let mut group = c.benchmark_group("core/native_task_engine_preloaded_hot_exec_only");

    for &(tasks, yields) in &[(4_096usize, 1usize), (16_384usize, 1usize)] {
        let total_steps = tasks * (yields + 1);

        macro_rules! bench_hot_exec_only {
            ($hot:expr) => {{
                group.throughput(Throughput::Elements(total_steps as u64));
                group.bench_with_input(
                    BenchmarkId::new(
                        format!("tasks{}_yields{}_hot{}", tasks, yields, $hot),
                        total_steps,
                    ),
                    &total_steps,
                    |b, &_ops| {
                        b.iter_batched(
                            || make_preloaded_engine(tasks, yields),
                            |mut engine| {
                                let progressed = engine.run_hot_until_idle::<$hot>();
                                black_box(progressed);
                            },
                            BatchSize::SmallInput,
                        );
                    },
                );
            }};
        }

        bench_hot_exec_only!(1);
        bench_hot_exec_only!(2);
        bench_hot_exec_only!(4);
        bench_hot_exec_only!(8);
    }

    group.finish();
}

criterion_group!(
    native_task_benches,
    bench_native_task_engine,
    bench_native_task_engine_preloaded,
    bench_native_task_engine_preloaded_hot,
    bench_native_task_engine_preloaded_hot_exec_only
);
criterion_main!(native_task_benches);
