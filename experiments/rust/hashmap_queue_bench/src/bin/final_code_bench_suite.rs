use dataplane_core_reactor::mailbox_future::{GlobalContext, ShardRuntimeHandle};
use dataplane_core_reactor::native_task::{
    NativeTask, NativeTaskCx, NativeTaskEngine, StepResult,
};
use dataplane_core_reactor::native_future_task::NativeFutureTask;
use dataplane_core_reactor::settings::DataPlaneSettings;
use std::env;
use std::future::Future;
use std::hint::black_box;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::task::{Context, Poll};
use std::thread;
use std::time::{Duration, Instant};

type RuntimeFutureTask = NativeFutureTask<dataplane_core_reactor::mailbox_future::BoxedRuntimeFuture>;

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

impl NativeTask for LeafTask {
    fn step(&mut self, _cx: &mut NativeTaskCx<Self>) -> StepResult {
        black_box(self.salt);
        if self.remaining == 0 {
            StepResult::Complete
        } else {
            self.remaining -= 1;
            StepResult::Ready
        }
    }
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

struct YieldOnce {
    yielded: bool,
}

impl Future for YieldOnce {
    type Output = ();

    fn poll(mut self: std::pin::Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.yielded {
            Poll::Ready(())
        } else {
            self.yielded = true;
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

fn yield_now() -> YieldOnce {
    YieldOnce { yielded: false }
}

fn env_usize(key: &str, default: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default)
}

fn env_optional_usize(key: &str) -> Option<usize> {
    env::var(key).ok().and_then(|v| v.parse::<usize>().ok())
}

fn pin_to(core: usize) {
    if env::var("PIN_THREADS").ok().as_deref() == Some("0") {
        return;
    }
    if let Some(ids) = core_affinity::get_core_ids() {
        let idx = core.min(ids.len().saturating_sub(1));
        let _ = core_affinity::set_for_current(ids[idx]);
    }
}

fn bench_single_shard_raw_yield(tasks: usize, yields: usize, rounds: usize, hot: usize) -> (usize, Duration) {
    let start = Instant::now();
    let mut progressed_total = 0usize;
    for _ in 0..rounds {
        let mut engine = NativeTaskEngine::with_task_capacity(tasks + 16);
        for i in 0..tasks {
            engine.spawn(LeafTask {
                remaining: yields,
                salt: i as u64,
            });
        }
        progressed_total += engine.run_hot_until_idle_dynamic(hot);
        assert_eq!(engine.active_tasks(), 0);
    }
    (progressed_total, start.elapsed())
}

#[inline(always)]
fn stable_raw_yield_workload(tasks: usize, rounds: usize) -> (usize, usize) {
    // Keep the smoke-run baseline cheap, but large enough that scheduler setup and timer
    // resolution do not dominate the reported throughput.
    (tasks.max(512), rounds.max(32))
}

fn bench_single_shard_flooder(tasks: usize, yields: usize, burst: usize, hot_tasks: usize) -> (usize, Duration) {
    let start = Instant::now();
    let mut engine = NativeTaskEngine::with_task_capacity(tasks.saturating_add(16));
    engine.spawn(BenchTask::Flooder(FlooderTask {
        remaining: tasks,
        yields,
        burst,
        salt: 0,
    }));
    let progressed = engine.run_hot_until_idle_dynamic(hot_tasks);
    (progressed, start.elapsed())
}

struct CrossFlooderShardArgs {
    shard_id: usize,
    tasks: usize,
    yields: usize,
    burst: usize,
    handle: ShardRuntimeHandle,
    completed: Arc<AtomicUsize>,
    checksum: Arc<AtomicU64>,
    barrier: Arc<Barrier>,
}

fn run_cross_flooder_shard(args: CrossFlooderShardArgs) -> (usize, Duration) {
    let CrossFlooderShardArgs {
        shard_id,
        tasks,
        yields,
        burst,
        handle,
        completed,
        checksum,
        barrier,
    } = args;
    pin_to(shard_id);
    let mut engine = NativeTaskEngine::<RuntimeFutureTask>::with_task_capacity(
        tasks.saturating_add(1024).max(4096),
    );
    barrier.wait();
    let start = Instant::now();
    let mut submitted = 0usize;
    while submitted < tasks {
        let batch = (tasks - submitted).min(burst.max(1));
        for i in 0..batch {
            let salt = ((shard_id * tasks) + submitted + i) as u64;
            let completed_task = completed.clone();
            let checksum_task = checksum.clone();
            handle.spawn_any_detached(async move {
                let mut acc = salt;
                for _ in 0..yields {
                    yield_now().await;
                    acc = acc.wrapping_add(1);
                }
                checksum_task.fetch_add(acc, Ordering::Relaxed);
                completed_task.fetch_add(1, Ordering::Relaxed);
            });
        }
        submitted += batch;
        let _ = handle.drain_native_runtime_queue(&mut engine);
        let _ = engine.run_until_idle();
    }

    let target_total = tasks.saturating_mul(2);
    loop {
        let _ = handle.drain_native_runtime_queue(&mut engine);
        let progressed = engine.run_until_idle();
        if completed.load(Ordering::Acquire) == target_total {
            let _ = handle.drain_native_runtime_queue(&mut engine);
            let _ = engine.run_until_idle();
            break;
        }
        if progressed == 0 {
            std::hint::spin_loop();
        }
    }
    (submitted, start.elapsed())
}

fn bench_dual_shard_cross_flooder(tasks_per_shard: usize, yields: usize, burst: usize) -> (usize, u64, Duration) {
    let global = Arc::new(GlobalContext::new(2));
    let completed = Arc::new(AtomicUsize::new(0));
    let checksum = Arc::new(AtomicU64::new(0));
    let barrier = Arc::new(Barrier::new(2));

    let mut handles = Vec::with_capacity(2);
    for shard_id in 0..2 {
        let handle = global.shard_handle(shard_id);
        let completed_shard = completed.clone();
        let checksum_shard = checksum.clone();
        let barrier_shard = barrier.clone();
        handles.push(thread::spawn(move || {
            run_cross_flooder_shard(CrossFlooderShardArgs {
                shard_id,
                tasks: tasks_per_shard,
                yields,
                burst,
                handle,
                completed: completed_shard,
                checksum: checksum_shard,
                barrier: barrier_shard,
            })
        }));
    }

    let mut submitted_total = 0usize;
    let mut max_elapsed = Duration::ZERO;
    for handle in handles {
        let (submitted, elapsed) = handle.join().expect("cross flooder shard");
        submitted_total += submitted;
        max_elapsed = max_elapsed.max(elapsed);
    }
    (submitted_total, checksum.load(Ordering::Acquire), max_elapsed)
}

fn main() {
    let settings = DataPlaneSettings::default();
    let yield_tasks = env_usize("TASKS", 4096);
    let yield_count = env_usize("YIELDS", 1);
    let yield_rounds = env_usize("ROUNDS", 1000);
    let yield_hot = env_usize("HOT", 1);
    let (yield_tasks, yield_rounds) = stable_raw_yield_workload(yield_tasks, yield_rounds);

    let flooder_tasks = env_usize("FLOODER_TASKS", 1_000_000);
    let flooder_yields = env_usize("FLOODER_YIELDS", 1);
    let flooder_burst = env_usize("FLOODER_BURST", 64);
    let flooder_hot = env_optional_usize("HOT_TASKS")
        .unwrap_or(settings.native_hot_task_budget());

    let dual_tasks = env_usize("DUAL_FLOODER_TASKS", flooder_tasks / 2);
    let dual_yields = env_usize("DUAL_FLOODER_YIELDS", flooder_yields);
    let dual_burst = env_usize("DUAL_FLOODER_BURST", flooder_burst);

    let (yield_steps, yield_elapsed) =
        bench_single_shard_raw_yield(yield_tasks, yield_count, yield_rounds, yield_hot);
    println!(
        "final_code_bench_suite case=single_shard_raw_yield tasks={} yields={} rounds={} hot={} steps={} elapsed_ms={:.3} ops_per_sec={:.0}",
        yield_tasks,
        yield_count,
        yield_rounds,
        yield_hot,
        yield_steps,
        yield_elapsed.as_secs_f64() * 1000.0,
        yield_steps as f64 / yield_elapsed.as_secs_f64(),
    );

    let (flooder_steps, flooder_elapsed) =
        bench_single_shard_flooder(flooder_tasks, flooder_yields, flooder_burst, flooder_hot);
    println!(
        "final_code_bench_suite case=single_shard_flooder tasks={} yields={} burst={} hot_tasks={} steps={} elapsed_ms={:.3} ops_per_sec={:.0}",
        flooder_tasks,
        flooder_yields,
        flooder_burst,
        flooder_hot,
        flooder_steps,
        flooder_elapsed.as_secs_f64() * 1000.0,
        flooder_steps as f64 / flooder_elapsed.as_secs_f64(),
    );

    let (dual_submitted, dual_checksum, dual_elapsed) =
        bench_dual_shard_cross_flooder(dual_tasks, dual_yields, dual_burst);
    let dual_ops = dual_tasks
        .saturating_mul(2)
        .saturating_mul(dual_yields.saturating_add(1));
    println!(
        "final_code_bench_suite case=dual_shard_cross_flooder shards=2 tasks_per_shard={} yields={} burst={} submitted={} elapsed_ms={:.3} ops_per_sec={:.0} checksum={}",
        dual_tasks,
        dual_yields,
        dual_burst,
        dual_submitted,
        dual_elapsed.as_secs_f64() * 1000.0,
        dual_ops as f64 / dual_elapsed.as_secs_f64(),
        dual_checksum,
    );
}
