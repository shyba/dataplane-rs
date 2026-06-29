#[cfg(not(feature = "bench-smolscale"))]
fn main() {
    eprintln!(
        "ours_vs_smolscale_bench requires --features bench-smolscale (example: cargo run --release --features bench-smolscale --bin ours_vs_smolscale_bench)"
    );
}

#[cfg(feature = "bench-smolscale")]
#[path = "common/executor_swap.rs"]
mod common;

#[cfg(feature = "bench-smolscale")]
use std::time::Instant;

#[cfg(feature = "bench-smolscale")]
#[derive(Clone, Copy, Debug)]
enum Backend {
    Ours,
    Smolscale,
    Both,
}

#[cfg(feature = "bench-smolscale")]
fn main() {
    let cfg = common::BenchConfig::from_env();
    let workload = common::generate_workload(&cfg);
    let backend = parse_backend();

    println!(
        "ours_vs_smolscale_bench backend={backend:?} case={:?} io_mode={:?} uring_link_mode={:?} uring_link_batch={} shards={} sessions={} hot_sessions={} total_ops={} session_budget={} runnable_budget={} drain_session={} ratios(c/m/h)={}/{}/{}",
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
        Backend::Smolscale => {
            run_and_print(
                "smolscale",
                || run_smolscale(&cfg, &workload),
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
                "smolscale",
                || run_smolscale(&cfg, &workload),
                workload.total_ops,
            );
        }
    }
}

#[cfg(feature = "bench-smolscale")]
fn run_smolscale(cfg: &common::BenchConfig, workload: &common::Workload) -> usize {
    let mut senders: Vec<Vec<async_channel::Sender<common::JobKind>>> =
        (0..cfg.shards).map(|_| Vec::new()).collect();
    let mut tasks = Vec::new();

    for (shard, slot_count) in workload.slot_counts.iter().copied().enumerate() {
        let _ = shard;
        for _slot in 0..slot_count {
            let (tx, rx) = async_channel::unbounded::<common::JobKind>();
            senders[shard].push(tx);
            let medium_iters = cfg.medium_iters;
            let heavy_iters = cfg.heavy_iters;
            let io_mode = cfg.io_mode;
            tasks.push(smolscale::spawn(async move {
                let mut completed = 0usize;
                while let Ok(job) = rx.recv().await {
                    common::execute_job(job, io_mode, medium_iters, heavy_iters);
                    completed += 1;
                }
                common::flush_io_mode(io_mode);
                completed
            }));
        }
    }

    for (shard, events) in workload.shard_events.iter().enumerate() {
        for item in events {
            senders[shard][item.slot]
                .send_blocking(item.kind)
                .expect("send");
        }
    }
    drop(senders);

    smolscale::block_on(async move {
        let mut completed = 0usize;
        for task in tasks {
            completed += task.await;
        }
        completed
    })
}

#[cfg(feature = "bench-smolscale")]
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

#[cfg(feature = "bench-smolscale")]
fn parse_backend() -> Backend {
    match std::env::var("EXEC_SWAP_BACKEND").ok().as_deref() {
        Some("ours") => Backend::Ours,
        Some("smolscale") => Backend::Smolscale,
        _ => Backend::Both,
    }
}
