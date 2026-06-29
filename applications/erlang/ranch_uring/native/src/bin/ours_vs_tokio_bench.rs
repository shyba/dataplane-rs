#[cfg(not(feature = "bench-tokio"))]
fn main() {
    eprintln!(
        "ours_vs_tokio_bench requires --features bench-tokio (example: cargo run --release --features bench-tokio --bin ours_vs_tokio_bench)"
    );
}

#[cfg(feature = "bench-tokio")]
#[path = "common/executor_swap.rs"]
mod common;

#[cfg(all(feature = "bench-tokio", feature = "bench-tokio-uring"))]
use std::fs::OpenOptions;

#[cfg(feature = "bench-tokio")]
use std::time::Instant;

#[cfg(feature = "bench-tokio")]
#[derive(Clone, Copy, Debug)]
enum Backend {
    Ours,
    Tokio,
    #[cfg(feature = "bench-tokio-uring")]
    TokioUring,
    Both,
}

#[cfg(feature = "bench-tokio")]
fn main() {
    let cfg = common::BenchConfig::from_env();
    let workload = common::generate_workload(&cfg);
    let backend = parse_backend();

    println!(
        "ours_vs_tokio_bench backend={backend:?} case={:?} io_mode={:?} uring_link_mode={:?} uring_link_batch={} shards={} sessions={} hot_sessions={} total_ops={} session_budget={} runnable_budget={} drain_session={} ratios(c/m/h)={}/{}/{}",
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
        Backend::Tokio => {
            run_and_print("tokio", || run_tokio(&cfg, &workload), workload.total_ops);
        }
        #[cfg(feature = "bench-tokio-uring")]
        Backend::TokioUring => {
            run_and_print(
                "tokio_uring",
                || run_tokio_uring(&cfg, &workload),
                workload.total_ops,
            );
        }
        Backend::Both => {
            run_and_print(
                "ours",
                || common::run_ours(&cfg, &workload),
                workload.total_ops,
            );
            run_and_print("tokio", || run_tokio(&cfg, &workload), workload.total_ops);
            #[cfg(feature = "bench-tokio-uring")]
            run_and_print(
                "tokio_uring",
                || run_tokio_uring(&cfg, &workload),
                workload.total_ops,
            );
        }
    }
}

#[cfg(feature = "bench-tokio")]
fn run_tokio(cfg: &common::BenchConfig, workload: &common::Workload) -> usize {
    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(cfg.shards.max(1))
        .enable_all()
        .build()
        .expect("build tokio runtime");

    runtime.block_on(async move {
        let mut senders: Vec<Vec<tokio::sync::mpsc::UnboundedSender<common::JobKind>>> =
            (0..cfg.shards).map(|_| Vec::new()).collect();
        let mut tasks = Vec::new();

        for (shard, slot_count) in workload.slot_counts.iter().copied().enumerate() {
            let _ = shard;
            for _slot in 0..slot_count {
                let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel::<common::JobKind>();
                senders[shard].push(tx);
                let medium_iters = cfg.medium_iters;
                let heavy_iters = cfg.heavy_iters;
                let io_mode = cfg.io_mode;
                tasks.push(tokio::spawn(async move {
                    let mut completed = 0usize;
                    while let Some(job) = rx.recv().await {
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
                senders[shard][item.slot].send(item.kind).expect("send");
            }
        }
        drop(senders);

        let mut completed = 0usize;
        for task in tasks {
            completed += task.await.expect("join tokio task");
        }
        completed
    })
}

#[cfg(all(feature = "bench-tokio", feature = "bench-tokio-uring"))]
fn run_tokio_uring(cfg: &common::BenchConfig, workload: &common::Workload) -> usize {
    let mut joins = Vec::with_capacity(cfg.shards);
    for shard in 0..cfg.shards {
        let events = workload.shard_events[shard].clone();
        let slot_count = workload.slot_counts[shard];
        let io_mode = cfg.io_mode;
        let medium_iters = cfg.medium_iters;
        let heavy_iters = cfg.heavy_iters;
        joins.push(
            std::thread::Builder::new()
                .name(format!("tokio-uring-shard-{shard}"))
                .spawn(move || {
                    common::pin_current_thread(shard);
                    tokio_uring::start(async move {
                        if slot_count == 0 {
                            return 0usize;
                        }
                        let mut senders =
                            Vec::<tokio::sync::mpsc::UnboundedSender<common::JobKind>>::new();
                        let mut tasks = Vec::new();
                        for _slot in 0..slot_count {
                            let (tx, mut rx) = tokio::sync::mpsc::unbounded_channel();
                            senders.push(tx);
                            tasks.push(tokio_uring::spawn(async move {
                                let mut completed = 0usize;
                                let mut read_file: Option<tokio_uring::fs::File> = None;
                                let mut write_file: Option<tokio_uring::fs::File> = None;
                                let mut read_buf = vec![0u8; 64];
                                let mut write_buf = vec![0u8; 64];
                                while let Some(job) = rx.recv().await {
                                    execute_tokio_uring_io(
                                        io_mode,
                                        &mut read_file,
                                        &mut write_file,
                                        &mut read_buf,
                                        &mut write_buf,
                                    )
                                    .await;
                                    common::apply_job_weight(job, medium_iters, heavy_iters);
                                    completed += 1;
                                }
                                if let Some(file) = read_file {
                                    let _ = file.close().await;
                                }
                                if let Some(file) = write_file {
                                    let _ = file.close().await;
                                }
                                common::flush_io_mode(io_mode);
                                completed
                            }));
                        }

                        for item in events {
                            senders[item.slot].send(item.kind).expect("send");
                        }
                        drop(senders);

                        let mut completed = 0usize;
                        for task in tasks {
                            completed += task.await.expect("join tokio-uring task");
                        }
                        completed
                    })
                })
                .expect("spawn tokio-uring shard"),
        );
    }
    joins
        .into_iter()
        .map(|j| j.join().expect("join tokio-uring shard"))
        .sum()
}

#[cfg(all(feature = "bench-tokio", feature = "bench-tokio-uring"))]
async fn execute_tokio_uring_io(
    io_mode: common::IoMode,
    read_file: &mut Option<tokio_uring::fs::File>,
    write_file: &mut Option<tokio_uring::fs::File>,
    read_buf: &mut Vec<u8>,
    write_buf: &mut Vec<u8>,
) {
    match io_mode {
        common::IoMode::UringNop => {
            tokio_uring::no_op().await.expect("tokio_uring no_op");
        }
        common::IoMode::UringDevNullRead => {
            if read_file.is_none() {
                let file = tokio_uring::fs::File::open("/dev/null")
                    .await
                    .expect("open /dev/null for tokio_uring read");
                *read_file = Some(file);
            }
            let file = read_file.as_ref().expect("read file initialized");
            let (res, buf) = file.read_at(std::mem::take(read_buf), 0).await;
            *read_buf = buf;
            let _ = res.expect("tokio_uring read_at");
        }
        common::IoMode::UringDevNullWrite => {
            if write_file.is_none() {
                let stdf = OpenOptions::new()
                    .write(true)
                    .open("/dev/null")
                    .expect("open /dev/null for tokio_uring write");
                *write_file = Some(tokio_uring::fs::File::from_std(stdf));
            }
            let file = write_file.as_ref().expect("write file initialized");
            let (res, buf) = file.write_at(std::mem::take(write_buf), 0).submit().await;
            *write_buf = buf;
            let _ = res.expect("tokio_uring write_at");
        }
        _ => common::run_io_mode(io_mode),
    }
}

#[cfg(feature = "bench-tokio")]
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

#[cfg(feature = "bench-tokio")]
fn parse_backend() -> Backend {
    match std::env::var("EXEC_SWAP_BACKEND").ok().as_deref() {
        Some("ours") => Backend::Ours,
        Some("tokio") => Backend::Tokio,
        #[cfg(feature = "bench-tokio-uring")]
        Some("tokio_uring") => Backend::TokioUring,
        #[cfg(not(feature = "bench-tokio-uring"))]
        Some("tokio_uring") => {
            panic!("tokio_uring backend requires --features bench-tokio-uring")
        }
        _ => Backend::Both,
    }
}
