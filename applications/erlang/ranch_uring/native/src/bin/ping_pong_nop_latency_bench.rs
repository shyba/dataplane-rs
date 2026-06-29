use std::collections::{HashMap, VecDeque};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::thread;
use std::time::{Duration, Instant};

use crossbeam_channel::{bounded, Receiver, RecvTimeoutError, Sender, TryRecvError};
use io_uring::{opcode, IoUring};

const DEFAULT_DURATION_MS: u64 = 3000;
const DEFAULT_RING_ENTRIES: usize = 256;
const DEFAULT_LATENCY_CAP: usize = 32;
const DEFAULT_THROUGHPUT_BATCH: usize = 64;
const DEFAULT_THROUGHPUT_QUEUE_MAX: usize = 8192;
const DEFAULT_THROUGHPUT_SAMPLE_EVERY: u64 = 1024;
const DEFAULT_COMPLETION_BUDGET: usize = 1024;

#[derive(Clone, Copy, Debug)]
enum Mode {
    Latency,
    Fifo,
}

#[derive(Clone, Copy)]
struct PingReq {
    id: u64,
    sent_at: Instant,
}

#[derive(Clone, Copy)]
struct PingReply {
    id: u64,
    latency_ns: u64,
}

#[derive(Clone, Copy)]
enum BenchCommand {
    Ping(PingReq),
    Throughput { enqueued_at: Instant },
}

#[derive(Clone, Copy)]
enum BenchTask {
    ReplyPing(PingReply),
}

#[derive(Clone, Copy)]
enum LatencyItem {
    Task(BenchTask),
    Command(BenchCommand, Option<u64>),
}

#[derive(Clone, Copy)]
enum InflightOp {
    Ping(PingReq),
    Throughput { enqueued_at: Instant },
}

struct LatencyQueue {
    items: [Option<LatencyItem>; DEFAULT_LATENCY_CAP],
    len: usize,
}

impl LatencyQueue {
    fn new() -> Self {
        Self {
            items: std::array::from_fn(|_| None),
            len: 0,
        }
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn push_command(&mut self, cmd: BenchCommand, cqe_ref: Option<u64>) -> bool {
        if self.len == DEFAULT_LATENCY_CAP {
            return false;
        }
        self.items[self.len] = Some(LatencyItem::Command(cmd, cqe_ref));
        self.len += 1;
        true
    }

    fn push_task(&mut self, task: BenchTask) -> bool {
        if self.len == DEFAULT_LATENCY_CAP {
            return false;
        }
        self.items[self.len] = Some(LatencyItem::Task(task));
        self.len += 1;
        true
    }

    fn drain(
        &mut self,
        mut on_command: impl FnMut(BenchCommand, Option<u64>),
        mut on_task: impl FnMut(BenchTask),
    ) -> usize {
        let mut drained = 0usize;
        while self.len > 0 {
            let batch_len = self.len;
            let mut batch: [Option<LatencyItem>; DEFAULT_LATENCY_CAP] =
                std::array::from_fn(|_| None);
            for (i, slot) in batch.iter_mut().enumerate().take(batch_len) {
                *slot = self.items[i].take();
            }
            self.len = 0;

            for item in batch.iter_mut().take(batch_len) {
                if matches!(item, Some(LatencyItem::Command(_, _))) {
                    if let Some(LatencyItem::Command(cmd, cqe_ref)) = item.take() {
                        on_command(cmd, cqe_ref);
                        drained += 1;
                    }
                }
            }
            for item in batch.into_iter().take(batch_len) {
                if let Some(LatencyItem::Task(task)) = item {
                    on_task(task);
                    drained += 1;
                }
            }
        }
        drained
    }
}

struct ExecutorStats {
    elapsed: Duration,
    throughput_completed: u64,
    throughput_latency_samples_ns: Vec<u64>,
    ping_completed: u64,
    ping_requests_seen: u64,
    ping_scheduled: u64,
    ping_replies_sent: u64,
}

fn main() {
    let mode = parse_mode("PINGPONG_MODE", Mode::Latency);
    let duration_ms = parse_env_u64("PINGPONG_DURATION_MS", DEFAULT_DURATION_MS);
    let ring_entries = parse_env_usize("PINGPONG_RING_ENTRIES", DEFAULT_RING_ENTRIES);
    let throughput_batch = parse_env_usize("PINGPONG_THROUGHPUT_BATCH", DEFAULT_THROUGHPUT_BATCH);
    let throughput_queue_max = parse_env_usize(
        "PINGPONG_THROUGHPUT_QUEUE_MAX",
        DEFAULT_THROUGHPUT_QUEUE_MAX,
    );
    let throughput_sample_every = parse_env_u64(
        "PINGPONG_THROUGHPUT_SAMPLE_EVERY",
        DEFAULT_THROUGHPUT_SAMPLE_EVERY,
    )
    .max(1);
    let completion_budget =
        parse_env_usize("PINGPONG_COMPLETION_BUDGET", DEFAULT_COMPLETION_BUDGET).max(1);

    println!(
        "ping_pong_nop_latency_bench mode={:?} duration_ms={} ring_entries={} throughput_batch={} throughput_queue_max={} latency_cap={}",
        mode,
        duration_ms,
        ring_entries,
        throughput_batch,
        throughput_queue_max,
        DEFAULT_LATENCY_CAP
    );

    let running = Arc::new(AtomicBool::new(true));
    let (ping_req_tx, ping_req_rx) = bounded::<PingReq>(1024);
    let (ping_reply_tx, ping_reply_rx) = bounded::<PingReply>(1024);

    let running_exec = running.clone();
    let exec_join = thread::Builder::new()
        .name("pingpong-exec".to_string())
        .spawn(move || {
            pin_current_thread(0);
            run_executor(
                mode,
                running_exec,
                ping_req_rx,
                ping_reply_tx,
                ring_entries,
                throughput_batch,
                throughput_queue_max,
                throughput_sample_every,
                completion_budget,
            )
        })
        .expect("spawn executor");

    let ping_req_tx_client = ping_req_tx.clone();
    let ping_join = thread::Builder::new()
        .name("pingpong-client".to_string())
        .spawn(move || run_ping_client(ping_req_tx_client, ping_reply_rx, duration_ms))
        .expect("spawn ping client");

    let ping_latencies_ns = ping_join.join().expect("join ping client");
    running.store(false, Ordering::Release);
    drop(ping_req_tx);

    let exec_stats = exec_join.join().expect("join executor");
    let throughput_ops_sec =
        exec_stats.throughput_completed as f64 / exec_stats.elapsed.as_secs_f64();

    println!(
        "ping_requests_seen={} ping_scheduled={} ping_completed={} ping_replies_sent={} throughput_completed={} elapsed_ms={:.2} throughput_ops_sec={:.0}",
        exec_stats.ping_requests_seen,
        exec_stats.ping_scheduled,
        exec_stats.ping_completed,
        exec_stats.ping_replies_sent,
        exec_stats.throughput_completed,
        exec_stats.elapsed.as_secs_f64() * 1000.0,
        throughput_ops_sec
    );

    if let Some((p50, p99, p999)) = summarize_ns(&ping_latencies_ns) {
        println!(
            "ping_rtt_us p50={} p99={} p99.9={}",
            ns_to_us(p50),
            ns_to_us(p99),
            ns_to_us(p999)
        );
    } else {
        println!("ping_rtt_us no_samples");
    }

    if let Some((p50, p99, p999)) = summarize_ns(&exec_stats.throughput_latency_samples_ns) {
        println!(
            "throughput_lag_us sampled={} p50={} p99={} p99.9={}",
            exec_stats.throughput_latency_samples_ns.len(),
            ns_to_us(p50),
            ns_to_us(p99),
            ns_to_us(p999)
        );
    } else {
        println!("throughput_lag_us sampled=0");
    }
}

fn run_ping_client(
    ping_req_tx: Sender<PingReq>,
    ping_reply_rx: Receiver<PingReply>,
    duration_ms: u64,
) -> Vec<u64> {
    let deadline = Instant::now() + Duration::from_millis(duration_ms);
    let mut latencies = Vec::new();
    let mut req_id = 1u64;
    while Instant::now() < deadline {
        let req = PingReq {
            id: req_id,
            sent_at: Instant::now(),
        };
        if ping_req_tx.send(req).is_err() {
            break;
        }
        match ping_reply_rx.recv_timeout(Duration::from_secs(2)) {
            Ok(reply) => {
                if reply.id == req_id {
                    latencies.push(reply.latency_ns);
                }
            }
            Err(RecvTimeoutError::Timeout) => break,
            Err(RecvTimeoutError::Disconnected) => break,
        }
        req_id = req_id.wrapping_add(1).max(1);
    }
    latencies
}

#[allow(clippy::too_many_arguments)]
fn run_executor(
    mode: Mode,
    running: Arc<AtomicBool>,
    ping_req_rx: Receiver<PingReq>,
    ping_reply_tx: Sender<PingReply>,
    ring_entries: usize,
    throughput_batch: usize,
    throughput_queue_max: usize,
    throughput_sample_every: u64,
    completion_budget: usize,
) -> ExecutorStats {
    let started = Instant::now();
    let mut ring = IoUring::new(ring_entries as u32).expect("create io_uring");
    let max_inflight = ring_entries.max(1);
    let max_inflight_normal = max_inflight.saturating_sub(1).max(1);

    let mut next_token = 1u64;
    let mut inflight = HashMap::<u64, InflightOp>::with_capacity(max_inflight * 2);
    let mut normal_queue = VecDeque::<BenchCommand>::with_capacity(throughput_queue_max);
    let mut latency_queue = LatencyQueue::new();
    let mut ping_closed = false;

    let mut ping_completed = 0u64;
    let mut ping_requests_seen = 0u64;
    let mut ping_scheduled = 0u64;
    let mut ping_replies_sent = 0u64;
    let mut throughput_completed = 0u64;
    let mut throughput_latency_samples_ns = Vec::new();

    loop {
        let mut progressed = false;

        let drained_latency = latency_queue.drain(
            |cmd, _cqe_ref| {
                let is_ping = matches!(cmd, BenchCommand::Ping(_));
                if !try_schedule_nop(&mut ring, &mut inflight, &mut next_token, cmd, max_inflight) {
                    normal_queue.push_front(cmd);
                } else if is_ping {
                    ping_scheduled = ping_scheduled.saturating_add(1);
                }
            },
            |task| match task {
                BenchTask::ReplyPing(reply) => {
                    let _ = ping_reply_tx.send(reply);
                    ping_replies_sent = ping_replies_sent.saturating_add(1);
                }
            },
        );
        if drained_latency > 0 {
            progressed = true;
        }

        loop {
            match ping_req_rx.try_recv() {
                Ok(req) => {
                    ping_requests_seen = ping_requests_seen.saturating_add(1);
                    let cmd = BenchCommand::Ping(req);
                    match mode {
                        Mode::Latency => {
                            if !latency_queue.push_command(cmd, None) {
                                normal_queue.push_back(cmd);
                            }
                        }
                        Mode::Fifo => normal_queue.push_back(cmd),
                    }
                    progressed = true;
                }
                Err(TryRecvError::Empty) => break,
                Err(TryRecvError::Disconnected) => {
                    ping_closed = true;
                    break;
                }
            }
        }

        if running.load(Ordering::Acquire) && normal_queue.len() < throughput_queue_max {
            let now = Instant::now();
            while normal_queue.len() < throughput_queue_max {
                for _ in 0..throughput_batch {
                    normal_queue.push_back(BenchCommand::Throughput { enqueued_at: now });
                }
            }
            progressed = true;
        }

        while let Some(cmd) = normal_queue.pop_front() {
            let is_ping = matches!(cmd, BenchCommand::Ping(_));
            let inflight_limit = match cmd {
                BenchCommand::Ping(_) => max_inflight,
                BenchCommand::Throughput { .. } => max_inflight_normal,
            };
            if !try_schedule_nop(
                &mut ring,
                &mut inflight,
                &mut next_token,
                cmd,
                inflight_limit,
            ) {
                normal_queue.push_front(cmd);
                break;
            }
            if is_ping {
                ping_scheduled = ping_scheduled.saturating_add(1);
            }
            progressed = true;
        }

        if !ring.submission().is_empty() {
            let _ = ring.submit().expect("submit");
            progressed = true;
        }

        let mut completed_now = 0usize;
        {
            let mut cq = ring.completion();
            while completed_now < completion_budget {
                let Some(cqe) = cq.next() else {
                    break;
                };
                completed_now += 1;
                let token = cqe.user_data();
                let Some(op) = inflight.remove(&token) else {
                    continue;
                };
                match op {
                    InflightOp::Ping(req) => {
                        let reply = PingReply {
                            id: req.id,
                            latency_ns: req.sent_at.elapsed().as_nanos() as u64,
                        };
                        if !latency_queue.push_task(BenchTask::ReplyPing(reply)) {
                            let _ = ping_reply_tx.send(reply);
                            ping_replies_sent = ping_replies_sent.saturating_add(1);
                        }
                        ping_completed = ping_completed.saturating_add(1);
                    }
                    InflightOp::Throughput { enqueued_at } => {
                        throughput_completed = throughput_completed.saturating_add(1);
                        if throughput_completed.is_multiple_of(throughput_sample_every) {
                            throughput_latency_samples_ns
                                .push(enqueued_at.elapsed().as_nanos() as u64);
                        }
                    }
                }
            }
        }
        if completed_now > 0 {
            progressed = true;
        }

        if !progressed && !inflight.is_empty() {
            ring.submit_and_wait(1).expect("submit_and_wait");
            continue;
        }

        if !running.load(Ordering::Acquire)
            && ping_closed
            && inflight.is_empty()
            && normal_queue.is_empty()
            && latency_queue.is_empty()
        {
            break;
        }
    }

    ExecutorStats {
        elapsed: started.elapsed(),
        throughput_completed,
        throughput_latency_samples_ns,
        ping_completed,
        ping_requests_seen,
        ping_scheduled,
        ping_replies_sent,
    }
}

fn try_schedule_nop(
    ring: &mut IoUring,
    inflight: &mut HashMap<u64, InflightOp>,
    next_token: &mut u64,
    cmd: BenchCommand,
    max_inflight: usize,
) -> bool {
    if inflight.len() >= max_inflight {
        return false;
    }
    let token = *next_token;
    let entry = opcode::Nop::new().build().user_data(token);

    let pushed = {
        let mut sq = ring.submission();
        sq.sync();
        unsafe { sq.push(&entry) }.is_ok()
    };
    if !pushed {
        let _ = ring.submit().expect("submit on sq full");
        let retry = {
            let mut sq = ring.submission();
            sq.sync();
            unsafe { sq.push(&entry) }.is_ok()
        };
        if !retry {
            return false;
        }
    }

    let op = match cmd {
        BenchCommand::Ping(req) => InflightOp::Ping(req),
        BenchCommand::Throughput { enqueued_at } => InflightOp::Throughput { enqueued_at },
    };
    inflight.insert(token, op);
    *next_token = (*next_token).wrapping_add(1).max(1);
    true
}

fn summarize_ns(values: &[u64]) -> Option<(u64, u64, u64)> {
    if values.is_empty() {
        return None;
    }
    let mut sorted = values.to_vec();
    sorted.sort_unstable();
    let p50 = percentile_sorted(&sorted, 50.0);
    let p99 = percentile_sorted(&sorted, 99.0);
    let p999 = percentile_sorted(&sorted, 99.9);
    Some((p50, p99, p999))
}

fn percentile_sorted(sorted: &[u64], p: f64) -> u64 {
    let n = sorted.len();
    let idx = ((p / 100.0) * (n as f64 - 1.0)).round() as usize;
    sorted[idx.min(n - 1)]
}

fn ns_to_us(ns: u64) -> u64 {
    ns / 1_000
}

fn parse_mode(name: &str, default: Mode) -> Mode {
    match std::env::var(name)
        .ok()
        .as_deref()
        .map(|v| v.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("fifo") => Mode::Fifo,
        Some("latency") => Mode::Latency,
        _ => default,
    }
}

fn parse_env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn parse_env_u64(name: &str, default: u64) -> u64 {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<u64>().ok())
        .filter(|value| *value > 0)
        .unwrap_or(default)
}

fn pin_current_thread(shard: usize) {
    let cpu_count = thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    let cpu = shard % cpu_count;
    let mut set = unsafe { std::mem::zeroed::<libc::cpu_set_t>() };
    unsafe {
        libc::CPU_ZERO(&mut set);
        libc::CPU_SET(cpu, &mut set);
        let rc = libc::sched_setaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &set);
        assert_eq!(rc, 0, "sched_setaffinity failed");
    }
}
