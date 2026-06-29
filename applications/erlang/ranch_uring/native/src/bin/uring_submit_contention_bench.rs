use std::sync::mpsc;
use std::thread;
use std::time::Instant;

use io_uring::{opcode, IoUring};
#[path = "common/step_stats.rs"]
mod step_stats;
use step_stats::StepStats;

const DEFAULT_PRODUCERS: usize = 4;
const DEFAULT_SHARDS: usize = 1;
const DEFAULT_TOTAL_OPS: usize = 8_000_000;
const DEFAULT_PUBLISH_BATCH: usize = 32;
const DEFAULT_SUBMIT_BATCH: usize = 8;
const DEFAULT_RING_ENTRIES: u32 = 256;
const DEFAULT_DRAIN_WEIGHT: usize = 64;
const DEFAULT_SQPOLL_IDLE_MS: u32 = 2000;
const NOP_PUSH_CHUNK: usize = 64;

#[derive(Clone, Copy, Debug)]
enum SqpollMode {
    Off,
    Try,
    Require,
}

fn main() {
    let producers = parse_env_usize("URING_SUBMIT_PRODUCERS", DEFAULT_PRODUCERS);
    let shards = parse_env_usize("URING_SUBMIT_SHARDS", DEFAULT_SHARDS);
    let total_ops = parse_env_usize("URING_SUBMIT_TOTAL_OPS", DEFAULT_TOTAL_OPS);
    let publish_batch = parse_env_usize("URING_SUBMIT_PUBLISH_BATCH", DEFAULT_PUBLISH_BATCH);
    let submit_batch = parse_env_usize("URING_SUBMIT_BATCH", DEFAULT_SUBMIT_BATCH);
    let ring_entries =
        parse_env_usize("URING_SUBMIT_RING_ENTRIES", DEFAULT_RING_ENTRIES as usize) as u32;
    let drain_weight = parse_env_usize("URING_SUBMIT_DRAIN_WEIGHT", DEFAULT_DRAIN_WEIGHT);
    let sqpoll_mode = parse_sqpoll_mode();
    let sqpoll_idle_ms = parse_env_usize(
        "URING_SUBMIT_SQPOLL_IDLE_MS",
        DEFAULT_SQPOLL_IDLE_MS as usize,
    ) as u32;
    let sqpoll_cpu = std::env::var("URING_SUBMIT_SQPOLL_CPU")
        .ok()
        .as_deref()
        .map(|s| s == "1" || s.eq_ignore_ascii_case("shard"))
        .unwrap_or(false);

    println!(
        "uring_submit_contention_bench producers={} shards={} total_ops={} publish_batch={} submit_batch={} ring_entries={} drain_weight={} sqpoll_mode={:?} sqpoll_idle_ms={} sqpoll_cpu={}",
        producers, shards, total_ops, publish_batch, submit_batch, ring_entries, drain_weight, sqpoll_mode, sqpoll_idle_ms, sqpoll_cpu
    );

    let ops_per_producer = total_ops / producers.max(1);
    let remainder = total_ops % producers.max(1);

    let mut shard_senders = Vec::with_capacity(shards);
    let mut shard_joins = Vec::with_capacity(shards);
    for shard in 0..shards {
        let (tx, rx) = mpsc::channel::<usize>();
        shard_senders.push(tx);
        shard_joins.push(
            thread::Builder::new()
                .name(format!("uring-submit-shard-{shard}"))
                .spawn(move || {
                    run_shard(
                        shard,
                        rx,
                        ring_entries,
                        submit_batch,
                        sqpoll_mode,
                        sqpoll_idle_ms,
                        sqpoll_cpu,
                    )
                })
                .expect("spawn shard worker"),
        );
    }

    let start = Instant::now();
    let mut producer_joins = Vec::with_capacity(producers);
    for producer_id in 0..producers {
        let senders = shard_senders.clone();
        let producer_ops = ops_per_producer + usize::from(producer_id < remainder);
        producer_joins.push(
            thread::Builder::new()
                .name(format!("uring-submit-producer-{producer_id}"))
                .spawn(move || {
                    pin_current_thread(producer_id);
                    run_producer(producer_id, producer_ops, senders, publish_batch)
                })
                .expect("spawn producer"),
        );
    }

    let mut published = 0usize;
    for join in producer_joins {
        published += join.join().expect("join producer");
    }
    drop(shard_senders);

    let mut completed = 0usize;
    for join in shard_joins {
        completed += join.join().expect("join shard");
    }

    let elapsed = start.elapsed();
    let ops_per_sec = completed as f64 / elapsed.as_secs_f64();
    println!(
        "published={} completed={} elapsed_ms={:.2} ops_per_sec={:.0}",
        published,
        completed,
        elapsed.as_secs_f64() * 1000.0,
        ops_per_sec
    );
}

fn run_producer(
    producer_id: usize,
    ops: usize,
    senders: Vec<mpsc::Sender<usize>>,
    publish_batch: usize,
) -> usize {
    let mut step_stats = StepStats::from_env("URING_SUBMIT_STEP_STATS");
    let shard_count = senders.len();
    let mut staged = vec![0usize; shard_count];
    let mut published = 0usize;
    for i in 0..ops {
        let shard = (producer_id + i) % shard_count;
        staged[shard] += 1;
        if staged[shard] >= publish_batch {
            let step = step_stats.begin();
            senders[shard]
                .send(staged[shard])
                .expect("send shard batch");
            step_stats.end("producer_send", step);
            published += staged[shard];
            staged[shard] = 0;
        }
    }
    for (shard, count) in staged.into_iter().enumerate() {
        if count != 0 {
            let step = step_stats.begin();
            senders[shard].send(count).expect("flush shard batch");
            step_stats.end("producer_send", step);
            published += count;
        }
    }
    let label = format!("uring_submit_contention_bench_producer_{producer_id}");
    step_stats.print(&label);
    published
}

fn run_shard(
    shard: usize,
    rx: mpsc::Receiver<usize>,
    ring_entries: u32,
    submit_batch: usize,
    sqpoll_mode: SqpollMode,
    sqpoll_idle_ms: u32,
    sqpoll_cpu: bool,
) -> usize {
    let mut step_stats = StepStats::from_env("URING_SUBMIT_STEP_STATS");
    let drain_weight = parse_env_usize("URING_SUBMIT_DRAIN_WEIGHT", DEFAULT_DRAIN_WEIGHT);
    let effective_submit_batch = submit_batch.min(ring_entries as usize).max(1);
    pin_current_thread(DEFAULT_PRODUCERS + shard);
    let mut ring = build_ring(ring_entries, sqpoll_mode, sqpoll_idle_ms, sqpoll_cpu, shard);
    let sqpoll_accepted = ring.params().is_setup_sqpoll();
    println!(
        "shard={} sqpoll_requested={:?} sqpoll_accepted={}",
        shard, sqpoll_mode, sqpoll_accepted
    );
    if sqpoll_accepted {
        let _ = ring.submitter().submit_and_wait(0);
    }
    let mut completed = 0usize;
    let mut in_flight = 0usize;
    let mut backlog = 0usize;
    let mut disconnected = false;

    loop {
        let step = step_stats.begin();
        while !disconnected {
            match rx.try_recv() {
                Ok(count) => backlog += count,
                Err(mpsc::TryRecvError::Empty) => break,
                Err(mpsc::TryRecvError::Disconnected) => disconnected = true,
            }
        }
        step_stats.end("queue_try_recv", step);

        if in_flight == 0 && backlog < drain_weight && !disconnected {
            let step = step_stats.begin();
            while backlog < drain_weight {
                match rx.try_recv() {
                    Ok(count) => backlog += count,
                    Err(mpsc::TryRecvError::Empty) => break,
                    Err(mpsc::TryRecvError::Disconnected) => {
                        disconnected = true;
                        break;
                    }
                }
            }
            step_stats.end("queue_try_recv", step);
        }

        let room = effective_submit_batch.saturating_sub(in_flight);
        if backlog != 0 && room != 0 {
            let take = backlog.min(room);
            submit_nops(&mut ring, take, &mut step_stats);
            in_flight += take;
            backlog -= take;
        }

        if in_flight != 0 {
            let drained =
                drain_ready_completions(&mut ring, &mut completed, &mut in_flight, &mut step_stats);
            if drained > 0 {
                continue;
            }
            if sqpoll_accepted && backlog != 0 && in_flight < effective_submit_batch {
                let step = step_stats.begin();
                std::hint::spin_loop();
                step_stats.end("spin_wait", step);
                continue;
            }
            wait_for_completions(
                &mut ring,
                &mut completed,
                &mut in_flight,
                1,
                &mut step_stats,
            );
            continue;
        }

        if disconnected {
            let label = format!("uring_submit_contention_bench_shard_{shard}");
            step_stats.print(&label);
            return completed;
        }

        let step = step_stats.begin();
        match rx.recv() {
            Ok(count) => backlog += count,
            Err(_) => disconnected = true,
        }
        step_stats.end("queue_block_recv", step);
    }
}

fn parse_sqpoll_mode() -> SqpollMode {
    match std::env::var("URING_SUBMIT_SQPOLL")
        .ok()
        .map(|s| s.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("try") => SqpollMode::Try,
        Some("require") | Some("on") | Some("1") => SqpollMode::Require,
        _ => SqpollMode::Off,
    }
}

fn build_ring(
    ring_entries: u32,
    sqpoll_mode: SqpollMode,
    sqpoll_idle_ms: u32,
    sqpoll_cpu: bool,
    shard: usize,
) -> IoUring {
    if matches!(sqpoll_mode, SqpollMode::Off) {
        return IoUring::new(ring_entries).expect("create io_uring");
    }

    let mut builder = IoUring::builder();
    builder.setup_sqpoll(sqpoll_idle_ms);
    if sqpoll_cpu {
        builder.setup_sqpoll_cpu(shard as u32);
    }

    match builder.build(ring_entries) {
        Ok(ring) => ring,
        Err(err) if matches!(sqpoll_mode, SqpollMode::Try) => {
            let errno = err.raw_os_error().unwrap_or(0);
            if matches!(
                errno,
                libc::EPERM | libc::EINVAL | libc::EOPNOTSUPP | libc::ENOSYS
            ) {
                IoUring::new(ring_entries).expect("create io_uring fallback")
            } else {
                panic!("create sqpoll io_uring failed with errno={errno}");
            }
        }
        Err(err) => panic!("create sqpoll io_uring failed: {err}"),
    }
}

fn wait_for_completions(
    ring: &mut IoUring,
    completed: &mut usize,
    in_flight: &mut usize,
    min_complete: usize,
    step_stats: &mut StepStats,
) {
    let step = step_stats.begin();
    ring.submit_and_wait(min_complete).expect("submit_and_wait");
    step_stats.end("wait_submit", step);
    let step = step_stats.begin();
    let mut cq = ring.completion();
    for cqe in &mut cq {
        std::hint::black_box(cqe.user_data());
        *completed += 1;
        *in_flight -= 1;
    }
    step_stats.end("cq_drain_wait", step);
}

fn drain_ready_completions(
    ring: &mut IoUring,
    completed: &mut usize,
    in_flight: &mut usize,
    step_stats: &mut StepStats,
) -> usize {
    let step = step_stats.begin();
    let mut cq = ring.completion();
    cq.sync();
    let mut drained = 0usize;
    for cqe in &mut cq {
        std::hint::black_box(cqe.user_data());
        *completed += 1;
        *in_flight -= 1;
        drained += 1;
    }
    step_stats.end("cq_drain_ready", step);
    drained
}

fn submit_nops(ring: &mut IoUring, count: usize, step_stats: &mut StepStats) {
    let step = step_stats.begin();
    let base = opcode::Nop::new().build().user_data(0);
    let entries: [io_uring::squeue::Entry; NOP_PUSH_CHUNK] = std::array::from_fn(|_| base.clone());
    let mut submitted = 0usize;
    while submitted < count {
        let want = (count - submitted).min(NOP_PUSH_CHUNK);
        {
            let mut sq = ring.submission();
            sq.sync();
            if unsafe { sq.push_multiple(&entries[..want]) }.is_err() {
                drop(sq);
                let submit_step = step_stats.begin();
                ring.submit().expect("submit nops for sq space");
                step_stats.end("submit_sq_space", submit_step);
                continue;
            }
        }
        let submit_step = step_stats.begin();
        ring.submit().expect("submit nops");
        step_stats.end("submit", submit_step);
        submitted += want;
    }
    step_stats.end("sq_fill", step);
}

fn parse_env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn pin_current_thread(cpu_hint: usize) {
    let cpu_count = thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    let cpu = cpu_hint % cpu_count;
    let mut set = unsafe { std::mem::zeroed::<libc::cpu_set_t>() };
    unsafe {
        libc::CPU_ZERO(&mut set);
        libc::CPU_SET(cpu, &mut set);
        let rc = libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
        assert_eq!(rc, 0, "sched_setaffinity failed");
    }
}
