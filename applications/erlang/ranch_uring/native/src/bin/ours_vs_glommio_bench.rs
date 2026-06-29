#[cfg(not(feature = "bench-glommio"))]
fn main() {
    eprintln!(
        "ours_vs_glommio_bench requires --features bench-glommio (example: cargo run --release --features bench-glommio --bin ours_vs_glommio_bench)"
    );
}

#[cfg(feature = "bench-glommio")]
#[path = "common/executor_swap.rs"]
mod common;

#[cfg(feature = "bench-glommio")]
use glommio::{channels::local_channel, LocalExecutorBuilder, Placement};

#[cfg(feature = "bench-glommio")]
use std::time::Instant;

#[cfg(feature = "bench-glommio")]
#[derive(Clone, Copy, Debug)]
enum Backend {
    Ours,
    Glommio,
    Both,
}

#[cfg(feature = "bench-glommio")]
fn main() {
    let cfg = common::BenchConfig::from_env();
    let workload = common::generate_workload(&cfg);
    let backend = parse_backend();

    println!(
        "ours_vs_glommio_bench backend={backend:?} case={:?} io_mode={:?} uring_link_mode={:?} uring_link_batch={} shards={} sessions={} hot_sessions={} total_ops={} session_budget={} runnable_budget={} drain_session={} ratios(c/m/h)={}/{}/{}",
        cfg.case,
        cfg.io_mode,
        cfg.uring_link_mode,
        cfg.uring_link_batch,
        cfg.shards,
        cfg.sessions,
        cfg.hot_sessions,
        workload.total_ops,
        cfg.session_budget,
        cfg.runnable_budget,
        cfg.drain_session,
        cfg.cheap_ratio,
        cfg.medium_ratio,
        cfg.heavy_ratio
    );

    match backend {
        Backend::Ours => {
            run_and_print(
                "ours",
                || common::run_ours(&cfg, &workload),
                workload.total_ops,
            );
        }
        Backend::Glommio => {
            run_and_print(
                "glommio",
                || run_glommio(&cfg, &workload),
                workload.total_ops,
            );
        }
        Backend::Both => {
            run_and_print(
                "ours",
                || common::run_ours(&cfg, &workload),
                workload.total_ops,
            );
            run_and_print(
                "glommio",
                || run_glommio(&cfg, &workload),
                workload.total_ops,
            );
        }
    }
}

#[cfg(feature = "bench-glommio")]
fn run_glommio(cfg: &common::BenchConfig, workload: &common::Workload) -> usize {
    let cpu_count = std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .max(1);

    let mut handles = Vec::with_capacity(cfg.shards);
    for shard in 0..cfg.shards {
        let events = workload.shard_events[shard].clone();
        let slot_count = workload.slot_counts[shard];
        let io_mode = cfg.io_mode;
        let medium_iters = cfg.medium_iters;
        let heavy_iters = cfg.heavy_iters;
        let cpu = shard % cpu_count;
        handles.push(
            LocalExecutorBuilder::new(Placement::Fixed(cpu))
                .name(&format!("glommio-swap-{shard}"))
                .io_memory(128 * 1024)
                .spawn(move || async move {
                    if slot_count == 0 {
                        return 0usize;
                    }

                    let mut senders = Vec::with_capacity(slot_count);
                    let mut tasks = Vec::with_capacity(slot_count);
                    for _slot in 0..slot_count {
                        let (tx, rx) = local_channel::new_unbounded::<common::JobKind>();
                        senders.push(tx);
                        tasks.push(glommio::spawn_local(async move {
                            let mut completed = 0usize;
                            while let Some(job) = rx.recv().await {
                                common::execute_job(job, io_mode, medium_iters, heavy_iters);
                                completed += 1;
                            }
                            common::flush_io_mode(io_mode);
                            completed
                        }));
                    }

                    for item in events {
                        senders[item.slot].try_send(item.kind).expect("send");
                    }
                    drop(senders);

                    let mut completed = 0usize;
                    for task in tasks {
                        completed += task.await;
                    }
                    completed
                })
                .expect("spawn glommio shard"),
        );
    }

    handles
        .into_iter()
        .map(|handle| handle.join().expect("join glommio shard"))
        .sum()
}

#[cfg(feature = "bench-glommio")]
fn run_and_print<F>(label: &str, f: F, expected_ops: usize)
where
    F: FnOnce() -> usize,
{
    let start = Instant::now();
    let completed = f();
    let elapsed = start.elapsed();
    assert_eq!(
        completed, expected_ops,
        "completed ops mismatch for backend {label}"
    );
    let ops_per_sec = completed as f64 / elapsed.as_secs_f64();
    println!(
        "backend={} completed_ops={} elapsed_ms={:.3} ops_per_sec={:.0}",
        label,
        completed,
        elapsed.as_secs_f64() * 1000.0,
        ops_per_sec
    );
}

#[cfg(feature = "bench-glommio")]
fn parse_backend() -> Backend {
    match std::env::var("EXEC_SWAP_BACKEND").ok().as_deref() {
        Some("ours") => Backend::Ours,
        Some("glommio") => Backend::Glommio,
        _ => Backend::Both,
    }
}
