use dataplane_core_reactor::native_future_task::NativeFutureTask;
use dataplane_core_reactor::native_task::{NativeTask, NativeTaskCx, NativeTaskEngine, StepResult};
use std::env;
use std::future::Future;
use std::hint::black_box;
use std::time::Instant;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum Mode {
    Native,
    Emulated,
}

impl Mode {
    fn from_env() -> Self {
        match env::var("MODE")
            .unwrap_or_else(|_| "native".to_string())
            .as_str()
        {
            "emulated" => Self::Emulated,
            _ => Self::Native,
        }
    }
}

fn env_usize(key: &str, default: usize) -> usize {
    env::var(key)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default)
}

#[derive(Clone, Copy)]
struct NativeYieldTask {
    remaining: usize,
    salt: u64,
}

impl NativeTask for NativeYieldTask {
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

struct YieldOnce {
    yielded: bool,
}

impl Future for YieldOnce {
    type Output = ();

    fn poll(
        mut self: std::pin::Pin<&mut Self>,
        _cx: &mut std::task::Context<'_>,
    ) -> std::task::Poll<Self::Output> {
        if self.yielded {
            std::task::Poll::Ready(())
        } else {
            self.yielded = true;
            std::task::Poll::Pending
        }
    }
}

async fn future_yield_n(remaining: usize, salt: u64) {
    black_box(salt);
    for _ in 0..remaining {
        YieldOnce { yielded: false }.await;
    }
}


fn bench_native(tasks: usize, yields: usize, rounds: usize, hot: usize) -> (usize, f64) {
    let start = Instant::now();
    let mut progressed_total = 0usize;

    for round in 0..rounds {
        let mut engine = NativeTaskEngine::with_task_capacity(tasks + 16);
        for i in 0..tasks {
            engine.spawn(NativeYieldTask {
                remaining: yields,
                salt: (round * tasks + i) as u64,
            });
        }
        progressed_total += engine.run_hot_until_idle_dynamic(hot);
        assert_eq!(engine.active_tasks(), 0);
    }

    (progressed_total, start.elapsed().as_secs_f64())
}

fn bench_emulated(tasks: usize, yields: usize, rounds: usize, hot: usize) -> (usize, f64) {
    let start = Instant::now();
    let mut progressed_total = 0usize;

    for round in 0..rounds {
        let mut engine = NativeTaskEngine::with_task_capacity(tasks + 16);
        for i in 0..tasks {
            engine.spawn(NativeFutureTask::ready(future_yield_n(
                yields,
                (round * tasks + i) as u64,
            )));
        }
        progressed_total += engine.run_hot_until_idle_dynamic(hot);
        assert_eq!(engine.active_tasks(), 0);
    }

    (progressed_total, start.elapsed().as_secs_f64())
}

fn main() {
    let mode = Mode::from_env();
    let tasks = env_usize("TASKS", 4096);
    let yields = env_usize("YIELDS", 1);
    let rounds = env_usize("ROUNDS", 1000);
    let hot = env_usize("HOT", 1);

    let (steps, elapsed) = match mode {
        Mode::Native => bench_native(tasks, yields, rounds, hot),
        Mode::Emulated => bench_emulated(tasks, yields, rounds, hot),
    };
    let ops = steps as f64 / elapsed;
    println!(
        "native_future_yield_compare mode={:?} tasks={} yields={} rounds={} hot={} steps={} elapsed_ms={:.3} ops_per_sec={:.0}",
        mode,
        tasks,
        yields,
        rounds,
        hot,
        steps,
        elapsed * 1000.0,
        ops
    );
}
