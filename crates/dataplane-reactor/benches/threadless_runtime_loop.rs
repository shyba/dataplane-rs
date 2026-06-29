use std::collections::VecDeque;
use std::hint::black_box;

use criterion::{criterion_group, criterion_main, BatchSize, BenchmarkId, Criterion, Throughput};
use dataplane_core_reactor::native_task::{NativeTask, NativeTaskCx, StepResult};
use dataplane_core_reactor::reactor_driver::{
    DriverBackendKind, DriverCapabilities, ReactorDriver, ReactorDriverWait,
};
use dataplane_core_reactor::reactor_model::{NetEvent, NetOp, NetOpKind, OpToken};
use dataplane_core_reactor::reactor_runtime::ReactorRuntime;
use dataplane_core_reactor::wake_handle::WakeHandle;
use dataplane_runtime::runtime_profiles::{
    build_balanced_runtime, build_profiled_runtime_from_profile,
    dispatch_profiled_runtime_loop_from_profile, TopologyProfile,
};

#[derive(Default)]
struct MockDriver {
    pending: VecDeque<(NetOp, OpToken)>,
    ready: VecDeque<NetEvent>,
}

impl MockDriver {
    fn with_ready_events(count: usize) -> Self {
        let mut ready = VecDeque::with_capacity(count);
        for idx in 0..count {
            ready.push_back(NetEvent::OpComplete {
                token: OpToken(idx as u64),
                kind: NetOpKind::Send,
                result: idx as i32,
                flags: 0,
            });
        }
        Self {
            pending: VecDeque::new(),
            ready,
        }
    }
}

impl ReactorDriver for MockDriver {
    type Error = std::io::Error;
    type Token = OpToken;
    type Submit = NetOp;
    type Event = NetEvent;

    fn submit(&mut self, op: Self::Submit, token: Self::Token) -> Result<(), Self::Error> {
        self.pending.push_back((op, token));
        Ok(())
    }

    fn flush(&mut self) -> Result<usize, Self::Error> {
        let mut flushed = 0usize;
        while let Some((op, token)) = self.pending.pop_front() {
            let kind = match op {
                NetOp::Connect { .. } => NetOpKind::Connect,
                NetOp::Send { .. } => NetOpKind::Send,
                NetOp::SendAll { .. } => NetOpKind::SendAll,
                NetOp::Recv { .. } => NetOpKind::Recv,
                NetOp::UdpSend { .. } => NetOpKind::UdpSend,
                NetOp::UdpRecv { .. } => NetOpKind::UdpRecv,
                NetOp::UdpRecvBatch { .. } => NetOpKind::UdpRecvBatch,
            };
            self.ready.push_back(NetEvent::OpComplete {
                token,
                kind,
                result: 1,
                flags: 0,
            });
            flushed += 1;
        }
        Ok(flushed)
    }

    fn drain<F>(&mut self, max_events: usize, mut on_event: F) -> Result<usize, Self::Error>
    where
        F: FnMut(Self::Event),
    {
        let mut drained = 0usize;
        while drained < max_events {
            match self.ready.pop_front() {
                Some(event) => {
                    on_event(event);
                    drained += 1;
                }
                None => break,
            }
        }
        Ok(drained)
    }

    fn outstanding(&self) -> usize {
        self.pending.len() + self.ready.len()
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

impl ReactorDriverWait for MockDriver {
    type Error = std::io::Error;
    type Readiness = ();

    fn readiness(&self) -> Option<Self::Readiness> {
        (!self.ready.is_empty()).then_some(())
    }

    fn wait(&mut self, _min_events: usize) -> Result<usize, Self::Error> {
        Ok(self.ready.len())
    }
}

struct EmptyTask;

impl NativeTask for EmptyTask {
    fn step(&mut self, _cx: &mut NativeTaskCx<Self>) -> StepResult {
        StepResult::Complete
    }
}

fn bench_empty_tick(c: &mut Criterion) {
    let mut group = c.benchmark_group("core/threadless_runtime_loop");
    group.throughput(Throughput::Elements(1));

    group.bench_function(BenchmarkId::new("empty_tick", "idle"), |b| {
        let mut runtime = ReactorRuntime::new(MockDriver::default());
        b.iter(|| {
            let flushed = runtime.flush().expect("flush");
            let drained = runtime.drain(1, |_| {}).expect("drain");
            black_box((flushed, drained, runtime.outstanding()));
        });
    });

    group.finish();
}

fn bench_submit_flush_drain_immediate(c: &mut Criterion) {
    let mut group = c.benchmark_group("core/threadless_runtime_loop");
    group.throughput(Throughput::Elements(1));

    group.bench_function(BenchmarkId::new("submit_flush_drain_immediate", "1"), |b| {
        let mut runtime = ReactorRuntime::new(MockDriver::default());
        b.iter(|| {
            let token = OpToken(7);
            runtime
                .submit(
                    NetOp::Send {
                        fd: -1,
                        ptr: std::ptr::null(),
                        len: 0,
                    },
                    token,
                )
                .expect("submit");
            let flushed = runtime.flush().expect("flush");
            let mut seen = 0usize;
            let drained = runtime
                .drain(1, |event| {
                    black_box(event);
                    seen += 1;
                })
                .expect("drain");
            black_box((flushed, drained, seen, runtime.outstanding()));
        });
    });

    group.finish();
}

fn bench_runtime_profile_submit_poll_immediate(c: &mut Criterion) {
    let mut group = c.benchmark_group("core/threadless_runtime_loop");
    group.throughput(Throughput::Elements(1));

    group.bench_function(
        BenchmarkId::new("runtime_profile_submit_poll_immediate", "balanced"),
        |b| {
            let mut runtime = build_profiled_runtime_from_profile::<MockDriver, EmptyTask>(
                TopologyProfile::balanced_dual_shard(),
                MockDriver::default(),
                1,
            )
            .expect("build balanced profiled runtime");
            b.iter(|| {
                let handle = runtime
                    .submission()
                    .submit(
                        NetOp::Send {
                            fd: -1,
                            ptr: std::ptr::null(),
                            len: 0,
                        },
                        WakeHandle::None,
                    )
                    .expect("submit");
                let mut seen = 0usize;
                let drained = runtime
                    .submission()
                    .poll_completions_into(1, |completion, returned| {
                        black_box(completion);
                        black_box(returned);
                        seen += 1;
                    })
                    .expect("poll_completions_into");
                black_box((handle.token(), drained, seen, runtime.has_work()));
            });
        },
    );

    group.bench_function(
        BenchmarkId::new("runtime_profile_submit_poll_immediate", "embedded"),
        |b| {
            let mut runtime = build_profiled_runtime_from_profile::<MockDriver, EmptyTask>(
                TopologyProfile::embedded_dual_shard(),
                MockDriver::default(),
                1,
            )
            .expect("build embedded profiled runtime");
            b.iter(|| {
                let handle = runtime
                    .submission()
                    .submit(
                        NetOp::Send {
                            fd: -1,
                            ptr: std::ptr::null(),
                            len: 0,
                        },
                        WakeHandle::None,
                    )
                    .expect("submit");
                let mut seen = 0usize;
                let drained = runtime
                    .submission()
                    .poll_completions_into(1, |completion, returned| {
                        black_box(completion);
                        black_box(returned);
                        seen += 1;
                    })
                    .expect("poll_completions_into");
                black_box((handle.token(), drained, seen, runtime.has_work()));
            });
        },
    );

    group.bench_function(
        BenchmarkId::new("runtime_profile_submit_poll_immediate", "performance"),
        |b| {
            let mut runtime = build_profiled_runtime_from_profile::<MockDriver, EmptyTask>(
                TopologyProfile::performance_dual_shard(),
                MockDriver::default(),
                1,
            )
            .expect("build performance profiled runtime");
            b.iter(|| {
                let handle = runtime
                    .submission()
                    .submit(
                        NetOp::Send {
                            fd: -1,
                            ptr: std::ptr::null(),
                            len: 0,
                        },
                        WakeHandle::None,
                    )
                    .expect("submit");
                let mut seen = 0usize;
                let drained = runtime
                    .submission()
                    .poll_completions_into(1, |completion, returned| {
                        black_box(completion);
                        black_box(returned);
                        seen += 1;
                    })
                    .expect("poll_completions_into");
                black_box((handle.token(), drained, seen, runtime.has_work()));
            });
        },
    );

    group.finish();
}

fn bench_runtime_tick_completions_or_wait_probe(c: &mut Criterion) {
    let mut group = c.benchmark_group("core/threadless_runtime_loop");
    group.throughput(Throughput::Elements(1));

    // Keep the direct balanced-builder variants as an explicit comparison probe.
    // They measure the minimal hosted balanced construction path against the
    // canonical profiled and loop-dispatch surfaces used elsewhere in M2.
    group.bench_function(
        BenchmarkId::new("runtime_tick_completions_or_wait_idle", "balanced_direct"),
        |b| {
            b.iter_batched(
                || build_balanced_runtime::<MockDriver, EmptyTask>(MockDriver::default(), 1),
                |mut runtime| {
                    let tick = runtime
                        .tick_completions_or_wait(0, 1, 1, 1)
                        .expect("direct balanced tick");
                    black_box((tick.completions.len(), tick.tasks, runtime.has_work()));
                },
                BatchSize::SmallInput,
            );
        },
    );

    group.bench_function(
        BenchmarkId::new("runtime_tick_completions_or_wait_idle", "balanced_profiled"),
        |b| {
            b.iter_batched(
                || {
                    build_profiled_runtime_from_profile::<MockDriver, EmptyTask>(
                        TopologyProfile::balanced_dual_shard(),
                        MockDriver::default(),
                        1,
                    )
                    .expect("build balanced profiled runtime")
                },
                |mut runtime| {
                    let tick = runtime
                        .tick_completions_or_wait(0, 1, 1, 1)
                        .expect("profiled balanced tick");
                    black_box((tick.completions.len(), tick.tasks, runtime.has_work()));
                },
                BatchSize::SmallInput,
            );
        },
    );

    group.bench_function(
        BenchmarkId::new(
            "runtime_tick_completions_or_wait_idle",
            "balanced_loop_handle",
        ),
        |b| {
            b.iter_batched(
                || {
                    dispatch_profiled_runtime_loop_from_profile::<MockDriver, EmptyTask, _, _, _, _>(
                        TopologyProfile::balanced_dual_shard(),
                        MockDriver::default(),
                        1,
                        |runtime| runtime,
                        |_runtime| unreachable!("balanced profile dispatch returned embedded"),
                        |_runtime| unreachable!("balanced profile dispatch returned performance"),
                    )
                    .expect("build balanced loop-handle runtime")
                },
                |mut runtime| {
                    let tick = runtime
                        .tick_completions_or_wait(0, 1, 1, 1)
                        .expect("loop-handle balanced tick");
                    black_box(tick);
                },
                BatchSize::SmallInput,
            );
        },
    );

    group.bench_function(
        BenchmarkId::new("runtime_tick_completions_or_wait_ready", "balanced_direct"),
        |b| {
            b.iter_batched(
                || {
                    build_balanced_runtime::<MockDriver, EmptyTask>(
                        MockDriver::with_ready_events(1),
                        1,
                    )
                },
                |mut runtime| {
                    let tick = runtime
                        .tick_completions_or_wait(0, 1, 1, 1)
                        .expect("direct balanced tick with completion");
                    black_box((tick.completions.len(), tick.tasks, runtime.has_work()));
                },
                BatchSize::SmallInput,
            );
        },
    );

    group.bench_function(
        BenchmarkId::new(
            "runtime_tick_completions_or_wait_ready",
            "balanced_profiled",
        ),
        |b| {
            b.iter_batched(
                || {
                    build_profiled_runtime_from_profile::<MockDriver, EmptyTask>(
                        TopologyProfile::balanced_dual_shard(),
                        MockDriver::with_ready_events(1),
                        1,
                    )
                    .expect("build balanced profiled runtime with completion")
                },
                |mut runtime| {
                    let tick = runtime
                        .tick_completions_or_wait(0, 1, 1, 1)
                        .expect("profiled balanced tick with completion");
                    black_box((tick.completions.len(), tick.tasks, runtime.has_work()));
                },
                BatchSize::SmallInput,
            );
        },
    );

    group.bench_function(
        BenchmarkId::new(
            "runtime_tick_completions_or_wait_ready",
            "balanced_loop_handle",
        ),
        |b| {
            b.iter_batched(
                || {
                    dispatch_profiled_runtime_loop_from_profile::<MockDriver, EmptyTask, _, _, _, _>(
                        TopologyProfile::balanced_dual_shard(),
                        MockDriver::with_ready_events(1),
                        1,
                        |runtime| runtime,
                        |_runtime| unreachable!("balanced profile dispatch returned embedded"),
                        |_runtime| unreachable!("balanced profile dispatch returned performance"),
                    )
                    .expect("build balanced loop-handle runtime with completion")
                },
                |mut runtime| {
                    let tick = runtime
                        .tick_completions_or_wait(0, 1, 1, 1)
                        .expect("loop-handle balanced tick with completion");
                    black_box(tick);
                },
                BatchSize::SmallInput,
            );
        },
    );

    group.finish();
}

fn bench_bounded_drain_buffered(c: &mut Criterion) {
    let mut group = c.benchmark_group("core/threadless_runtime_loop");
    group.throughput(Throughput::Elements(128));

    group.bench_function(BenchmarkId::new("bounded_drain_buffered", "128x32"), |b| {
        b.iter_batched(
            || ReactorRuntime::new(MockDriver::with_ready_events(128)),
            |mut runtime| {
                let mut total = 0usize;
                for _ in 0..4 {
                    let drained = runtime
                        .drain(32, |event| {
                            black_box(event);
                            total += 1;
                        })
                        .expect("drain");
                    black_box(drained);
                }
                black_box((total, runtime.outstanding()));
            },
            BatchSize::SmallInput,
        );
    });

    group.finish();
}

criterion_group!(
    threadless_runtime_loop_benches,
    bench_empty_tick,
    bench_submit_flush_drain_immediate,
    bench_runtime_profile_submit_poll_immediate,
    bench_runtime_tick_completions_or_wait_probe,
    bench_bounded_drain_buffered
);
criterion_main!(threadless_runtime_loop_benches);
