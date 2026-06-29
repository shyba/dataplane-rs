use std::collections::HashMap;
use std::fs::File;
use std::io::{BufRead, BufReader};
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread::{self, Thread};
use std::time::{Duration, Instant};

use ranch_uring_nif::local_boundary::LocalShardPublisher;
use ranch_uring_nif::local_exec::LocalExec;
use ranch_uring_nif::local_exec_counts::LocalExecCounts;
use ranch_uring_nif::local_ingress::LocalIngress;
use ranch_uring_nif::replay_protocol::{ReplayKind, ScheduledOp, TraceEvent};

#[derive(Default)]
struct ShardStats {
    recv_ops: u64,
    recv_bytes: u64,
    send_ops: u64,
    send_bytes: u64,
    accept_ops: u64,
}

struct Worker {
    ingress: LocalIngress<ScheduledOp>,
    pending: Arc<AtomicUsize>,
    notified: Arc<AtomicBool>,
    thread: Thread,
    done: Arc<AtomicBool>,
}

#[derive(Clone, Copy)]
struct CountOp {
    slot: usize,
    count: usize,
    weight: usize,
    recv_ops: u64,
    recv_bytes: u64,
    send_ops: u64,
    send_bytes: u64,
    accept_ops: u64,
}

struct CountWorker {
    ingress: LocalIngress<CountOp>,
    pending: Arc<AtomicUsize>,
    notified: Arc<AtomicBool>,
    thread: Thread,
    done: Arc<AtomicBool>,
}

#[derive(Clone, Copy)]
enum CheapSyscall {
    None,
    GetTid,
}

const SESSION_RUN_BUDGET: usize = 16;
const RUNNABLE_DRAIN_BUDGET: usize = 256;

#[derive(Clone, Copy)]
enum ReplayMode {
    File,
    Synthetic,
}

struct LoadedTrace {
    events: Vec<TraceEvent>,
    max_shard: usize,
    reply_ok: u64,
    reply_error: u64,
    reply_timeout: u64,
}

struct ReplayWorkload {
    parsed_ops: Vec<(usize, ScheduledOp)>,
    parsed_ops_by_shard: Vec<Vec<ScheduledOp>>,
    count_ops_by_shard: Vec<Vec<CountOp>>,
    shard_slot_counts: Vec<usize>,
    reply_ok: u64,
    reply_error: u64,
    reply_timeout: u64,
}

#[derive(Clone, Copy)]
enum SyntheticCase {
    Serial,
    Parallel,
    Mixed,
}

struct SyntheticConfig {
    case: SyntheticCase,
    total_ops: usize,
    sessions: usize,
    hot_sessions: usize,
    hot_ratio: u32,
    accept_ratio: u32,
    recv_ratio: u32,
    send_ratio: u32,
    max_bytes: usize,
    seed: u64,
}

impl SyntheticConfig {
    fn from_env() -> Self {
        let case = match std::env::var("TRACE_REPLAY_SYNTHETIC_CASE").ok().as_deref() {
            Some("serial") => SyntheticCase::Serial,
            Some("parallel") => SyntheticCase::Parallel,
            _ => SyntheticCase::Mixed,
        };
        let sessions = parse_env_usize("TRACE_REPLAY_SYNTHETIC_SESSIONS", 1024).max(1);
        let hot_default = (sessions / 8).max(1);
        let hot_sessions =
            parse_env_usize("TRACE_REPLAY_SYNTHETIC_HOT_SESSIONS", hot_default).clamp(1, sessions);
        let hot_ratio = parse_env_u32("TRACE_REPLAY_SYNTHETIC_HOT_RATIO", 80).min(100);
        Self {
            case,
            total_ops: parse_env_usize("TRACE_REPLAY_SYNTHETIC_TOTAL_OPS", 2_000_000).max(1),
            sessions,
            hot_sessions,
            hot_ratio,
            accept_ratio: parse_env_u32("TRACE_REPLAY_SYNTHETIC_ACCEPT_RATIO", 5),
            recv_ratio: parse_env_u32("TRACE_REPLAY_SYNTHETIC_RECV_RATIO", 50),
            send_ratio: parse_env_u32("TRACE_REPLAY_SYNTHETIC_SEND_RATIO", 45),
            max_bytes: parse_env_usize("TRACE_REPLAY_SYNTHETIC_MAX_BYTES", 4096).max(1),
            seed: parse_env_u64("TRACE_REPLAY_SYNTHETIC_SEED", 0xC0FFEE_u64),
        }
    }
}

struct XorShift64 {
    state: u64,
}

impl XorShift64 {
    fn new(seed: u64) -> Self {
        Self {
            state: if seed == 0 {
                0x9E37_79B9_7F4A_7C15
            } else {
                seed
            },
        }
    }

    #[inline(always)]
    fn next_u64(&mut self) -> u64 {
        let mut x = self.state;
        x ^= x << 13;
        x ^= x >> 7;
        x ^= x << 17;
        self.state = x;
        x
    }

    #[inline(always)]
    fn next_usize(&mut self, upper: usize) -> usize {
        if upper <= 1 {
            0
        } else {
            (self.next_u64() as usize) % upper
        }
    }

    #[inline(always)]
    fn pct(&mut self) -> u32 {
        (self.next_u64() % 100) as u32
    }
}

fn main() {
    let mode = parse_replay_mode();
    let shard_count_env = std::env::var("TRACE_REPLAY_SHARDS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|n| *n > 0);
    let reshared = std::env::var("TRACE_REPLAY_RESHARD").ok().as_deref() == Some("1");
    let session_run_budget = std::env::var("TRACE_REPLAY_SESSION_BUDGET")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(SESSION_RUN_BUDGET);
    let drain_session = std::env::var("TRACE_REPLAY_DRAIN_SESSION").ok().as_deref() == Some("1");
    let cheap_syscall = match std::env::var("TRACE_REPLAY_CHEAP_SYSCALL").ok().as_deref() {
        Some("gettid") => CheapSyscall::GetTid,
        _ => CheapSyscall::None,
    };
    let prepartition = std::env::var("TRACE_REPLAY_PREPARTITION").ok().as_deref() == Some("1");
    let publish_batch = std::env::var("TRACE_REPLAY_PUBLISH_BATCH")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(64);
    let recv_weight = std::env::var("TRACE_REPLAY_WEIGHT_RECV")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1);
    let send_weight = std::env::var("TRACE_REPLAY_WEIGHT_SEND")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(1);
    let accept_weight = std::env::var("TRACE_REPLAY_WEIGHT_ACCEPT")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(4);
    let publish_slots = std::env::var("TRACE_REPLAY_PUBLISH_SLOTS")
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(256);
    let local_exec = std::env::var("TRACE_REPLAY_LOCAL_EXEC").ok().as_deref() == Some("1");
    let mut workload = match mode {
        ReplayMode::File => {
            let path = std::env::args()
                .nth(1)
                .expect("usage: trace_replay <trace-file> (or set TRACE_REPLAY_MODE=synthetic)");
            let loaded = load_trace_events(&path);
            let shard_count = shard_count_env.unwrap_or(loaded.max_shard.saturating_add(1).max(1));
            build_workload_from_events(
                loaded.events,
                shard_count,
                reshared,
                recv_weight,
                send_weight,
                accept_weight,
                loaded.reply_ok,
                loaded.reply_error,
                loaded.reply_timeout,
            )
        }
        ReplayMode::Synthetic => {
            let shard_count = shard_count_env.unwrap_or_else(default_shard_count);
            let synthetic = SyntheticConfig::from_env();
            build_synthetic_workload(
                shard_count,
                reshared,
                recv_weight,
                send_weight,
                accept_weight,
                &synthetic,
            )
        }
    };
    let shard_count = workload.shard_slot_counts.len();
    let reply_ok = workload.reply_ok;
    let reply_error = workload.reply_error;
    let reply_timeout = workload.reply_timeout;
    let parsed_ops = std::mem::take(&mut workload.parsed_ops);
    let mut parsed_ops_by_shard = std::mem::take(&mut workload.parsed_ops_by_shard);
    let mut count_ops_by_shard = std::mem::take(&mut workload.count_ops_by_shard);
    let shard_slot_counts = std::mem::take(&mut workload.shard_slot_counts);
    let total_count_replayed: u64 = count_ops_by_shard
        .iter()
        .flatten()
        .map(|op| op.count as u64)
        .sum();

    let start_barrier = if local_exec {
        Some(Arc::new(Barrier::new(shard_count + 1)))
    } else {
        None
    };
    let cheap_mode = !matches!(cheap_syscall, CheapSyscall::None);
    let mut workers = Vec::new();
    let mut count_workers = Vec::new();
    let mut joins = Vec::with_capacity(shard_count);
    for shard in 0..shard_count {
        let pending = Arc::new(AtomicUsize::new(0));
        let notified = Arc::new(AtomicBool::new(false));
        let done = Arc::new(AtomicBool::new(false));
        let (tx, rx) = std::sync::mpsc::sync_channel::<Thread>(1);
        let slot_count = shard_slot_counts[shard];
        let barrier = start_barrier.clone();
        if cheap_mode {
            let ingress = LocalIngress::new(publish_slots, publish_batch);
            let worker = CountWorker {
                ingress: ingress.clone(),
                pending: pending.clone(),
                notified: notified.clone(),
                thread: thread::current(),
                done: done.clone(),
            };
            let initial_ops = if local_exec {
                std::mem::take(&mut count_ops_by_shard[shard])
            } else {
                Vec::new()
            };
            let join = thread::Builder::new()
                .name(format!("trace_replay_shard_{shard}"))
                .spawn(move || {
                    worker_loop_counts(
                        ingress,
                        pending,
                        notified,
                        done,
                        tx,
                        slot_count,
                        session_run_budget,
                        drain_session,
                        cheap_syscall,
                        initial_ops,
                        barrier,
                    )
                })
                .expect("spawn worker");
            let worker_thread = rx.recv().expect("worker thread handle");
            count_workers.push(worker);
            count_workers[shard].thread = worker_thread;
            joins.push(join);
        } else {
            let ingress = LocalIngress::new(publish_slots, publish_batch);
            let worker = Worker {
                ingress: ingress.clone(),
                pending: pending.clone(),
                notified: notified.clone(),
                thread: thread::current(),
                done: done.clone(),
            };
            let initial_ops = if local_exec {
                std::mem::take(&mut parsed_ops_by_shard[shard])
            } else {
                Vec::new()
            };
            let join = thread::Builder::new()
                .name(format!("trace_replay_shard_{shard}"))
                .spawn(move || {
                    worker_loop(
                        ingress,
                        pending,
                        notified,
                        done,
                        tx,
                        slot_count,
                        session_run_budget,
                        drain_session,
                        cheap_syscall,
                        initial_ops,
                        barrier,
                    )
                })
                .expect("spawn worker");
            let worker_thread = rx.recv().expect("worker thread handle");
            workers.push(worker);
            workers[shard].thread = worker_thread;
            joins.push(join);
        }
    }

    let mut total_replayed = 0u64;
    let start = Instant::now();
    if local_exec {
        total_replayed = if cheap_mode {
            total_count_replayed
        } else {
            parsed_ops.len() as u64
        };
        if let Some(barrier) = &start_barrier {
            barrier.wait();
        }
    } else if prepartition {
        if cheap_mode {
            for (shard, mut shard_ops) in count_ops_by_shard.into_iter().enumerate() {
                if shard_ops.is_empty() {
                    continue;
                }
                total_replayed += shard_ops.iter().map(|op| op.count as u64).sum::<u64>();
                publish_count_batch_to_worker(&count_workers[shard], &mut shard_ops);
            }
        } else {
            for (shard, mut shard_ops) in parsed_ops_by_shard.into_iter().enumerate() {
                if shard_ops.is_empty() {
                    continue;
                }
                total_replayed += shard_ops.len() as u64;
                publish_batch_to_worker(&workers[shard], &mut shard_ops);
            }
        }
    } else if cheap_mode {
        let mut publisher = LocalShardPublisher::new(shard_count, publish_batch);
        for (shard, shard_ops) in count_ops_by_shard.into_iter().enumerate() {
            for op in shard_ops {
                total_replayed += op.count as u64;
                publisher.push_known_weight(shard, op, op.weight, |target_shard, batch| {
                    publish_count_batch_to_worker(&count_workers[target_shard], batch);
                });
            }
        }
        publisher.flush_all(|target_shard, batch| {
            publish_count_batch_to_worker(&count_workers[target_shard], batch);
        });
    } else {
        let mut publisher = LocalShardPublisher::new(shard_count, publish_batch);
        for (shard, op) in parsed_ops {
            let weight = scheduled_op_weight(&op, recv_weight, send_weight, accept_weight);
            if weight == 1 {
                publisher.push(shard, op, |target_shard, batch| {
                    publish_batch_to_worker(&workers[target_shard], batch);
                });
            } else {
                publisher.push_known_weight(shard, op, weight, |target_shard, batch| {
                    publish_batch_to_worker(&workers[target_shard], batch);
                });
            }
            total_replayed += 1;
        }
        publisher.flush_all(|target_shard, batch| {
            publish_batch_to_worker(&workers[target_shard], batch);
        });
    }

    if cheap_mode {
        for worker in &count_workers {
            loop {
                if worker.pending.load(Ordering::Acquire) == 0 {
                    break;
                }
                thread::yield_now();
            }
            worker.done.store(true, Ordering::Release);
            worker.notified.store(true, Ordering::Release);
            worker.thread.unpark();
        }
    } else {
        for worker in &workers {
            loop {
                if worker.pending.load(Ordering::Acquire) == 0 {
                    break;
                }
                thread::yield_now();
            }
            worker.done.store(true, Ordering::Release);
            worker.notified.store(true, Ordering::Release);
            worker.thread.unpark();
        }
    }
    let mut total_recv = 0u64;
    let mut total_recv_bytes = 0u64;
    let mut total_send = 0u64;
    let mut total_send_bytes = 0u64;
    let mut total_accept = 0u64;
    for (shard, join) in joins.into_iter().enumerate() {
        let stats = join.join().expect("join worker");
        total_recv += stats.recv_ops;
        total_recv_bytes += stats.recv_bytes;
        total_send += stats.send_ops;
        total_send_bytes += stats.send_bytes;
        total_accept += stats.accept_ops;
        println!(
            "shard={} recv_ops={} recv_bytes={} send_ops={} send_bytes={} accept_ops={}",
            shard,
            stats.recv_ops,
            stats.recv_bytes,
            stats.send_ops,
            stats.send_bytes,
            stats.accept_ops
        );
    }
    let elapsed = start.elapsed();
    println!(
        "replayed_ops={} elapsed_ms={:.3} ops_per_sec={:.0} reply_ok={} reply_error={} reply_timeout={} total_recv_ops={} total_send_ops={} total_accept_ops={} total_recv_bytes={} total_send_bytes={}",
        total_replayed,
        elapsed.as_secs_f64() * 1000.0,
        total_replayed as f64 / elapsed.as_secs_f64(),
        reply_ok,
        reply_error,
        reply_timeout,
        total_recv,
        total_send,
        total_accept,
        total_recv_bytes,
        total_send_bytes
    );
}

fn parse_replay_mode() -> ReplayMode {
    match std::env::var("TRACE_REPLAY_MODE").ok().as_deref() {
        Some("synthetic") => ReplayMode::Synthetic,
        _ => ReplayMode::File,
    }
}

fn default_shard_count() -> usize {
    std::thread::available_parallelism()
        .map(|n| n.get())
        .unwrap_or(1)
        .max(1)
}

fn load_trace_events(path: &str) -> LoadedTrace {
    let file = File::open(path).expect("open trace");
    let reader = BufReader::new(file);
    let mut events = Vec::new();
    let mut max_shard = 0usize;
    let mut reply_ok = 0u64;
    let mut reply_error = 0u64;
    let mut reply_timeout = 0u64;
    for line in reader.lines() {
        let line = line.expect("read line");
        let event = parse_line(&line);
        match &event {
            TraceEvent::RecvReq { shard, .. } | TraceEvent::SendReq { shard, .. } => {
                max_shard = max_shard.max(*shard);
                events.push(event);
            }
            TraceEvent::AcceptReq { .. } => events.push(event),
            TraceEvent::ReplyOk { request_id, bytes } => {
                let _ = (*request_id, *bytes);
                reply_ok += 1;
            }
            TraceEvent::ReplyError { request_id } => {
                let _ = *request_id;
                reply_error += 1;
            }
            TraceEvent::ReplyTimeout { request_id } => {
                let _ = *request_id;
                reply_timeout += 1;
            }
            TraceEvent::Other => {}
        }
    }
    LoadedTrace {
        events,
        max_shard,
        reply_ok,
        reply_error,
        reply_timeout,
    }
}

#[allow(clippy::too_many_arguments)]
fn build_workload_from_events(
    events: Vec<TraceEvent>,
    shard_count: usize,
    reshared: bool,
    recv_weight: usize,
    send_weight: usize,
    accept_weight: usize,
    reply_ok: u64,
    reply_error: u64,
    reply_timeout: u64,
) -> ReplayWorkload {
    let mut parsed_ops = Vec::with_capacity(events.len());
    let mut parsed_ops_by_shard: Vec<Vec<ScheduledOp>> =
        (0..shard_count).map(|_| Vec::new()).collect();
    let mut count_ops_maps: Vec<HashMap<usize, CountOp>> =
        (0..shard_count).map(|_| HashMap::new()).collect();
    let mut shard_slot_ids: Vec<HashMap<(u8, u64), usize>> =
        (0..shard_count).map(|_| HashMap::new()).collect();
    let mut shard_slot_counts = vec![0usize; shard_count];
    for event in events {
        let mapped = match event {
            TraceEvent::RecvReq {
                shard,
                session,
                len,
            } => {
                let target_shard = if reshared {
                    remap_shard(session, shard_count)
                } else {
                    shard % shard_count
                };
                Some((target_shard, (0u8, session), ReplayKind::Recv { len }))
            }
            TraceEvent::SendReq {
                shard,
                session,
                bytes,
            } => {
                let target_shard = if reshared {
                    remap_shard(session, shard_count)
                } else {
                    shard % shard_count
                };
                Some((target_shard, (0u8, session), ReplayKind::Send { bytes }))
            }
            TraceEvent::AcceptReq {
                listener,
                timeout_ms,
            } => {
                let target_shard = if reshared {
                    remap_shard(listener, shard_count)
                } else {
                    (listener as usize) % shard_count
                };
                Some((
                    target_shard,
                    (1u8, listener),
                    ReplayKind::Accept { timeout_ms },
                ))
            }
            _ => None,
        };
        if let Some((shard, key, kind)) = mapped {
            push_workload_op(
                shard,
                key,
                kind,
                recv_weight,
                send_weight,
                accept_weight,
                &mut parsed_ops,
                &mut parsed_ops_by_shard,
                &mut count_ops_maps,
                &mut shard_slot_ids,
                &mut shard_slot_counts,
            );
        }
    }
    let count_ops_by_shard = count_ops_maps
        .into_iter()
        .map(|m| m.into_values().collect())
        .collect();
    ReplayWorkload {
        parsed_ops,
        parsed_ops_by_shard,
        count_ops_by_shard,
        shard_slot_counts,
        reply_ok,
        reply_error,
        reply_timeout,
    }
}

fn build_synthetic_workload(
    shard_count: usize,
    reshared: bool,
    recv_weight: usize,
    send_weight: usize,
    accept_weight: usize,
    cfg: &SyntheticConfig,
) -> ReplayWorkload {
    let mut parsed_ops = Vec::with_capacity(cfg.total_ops);
    let mut parsed_ops_by_shard: Vec<Vec<ScheduledOp>> =
        (0..shard_count).map(|_| Vec::new()).collect();
    let mut count_ops_maps: Vec<HashMap<usize, CountOp>> =
        (0..shard_count).map(|_| HashMap::new()).collect();
    let mut shard_slot_ids: Vec<HashMap<(u8, u64), usize>> =
        (0..shard_count).map(|_| HashMap::new()).collect();
    let mut shard_slot_counts = vec![0usize; shard_count];
    let mut rng = XorShift64::new(cfg.seed);
    let cold_sessions = cfg.sessions.saturating_sub(cfg.hot_sessions);
    let ratio_total = (cfg.recv_ratio + cfg.send_ratio + cfg.accept_ratio).max(1);
    let listener_count = match cfg.case {
        SyntheticCase::Serial => cfg.hot_sessions.clamp(1, 8),
        SyntheticCase::Parallel => (cfg.sessions / 8).max(1),
        SyntheticCase::Mixed => (cfg.sessions / 16).max(1),
    };

    for _ in 0..cfg.total_ops {
        let choose_hot = match cfg.case {
            SyntheticCase::Serial => true,
            SyntheticCase::Parallel => false,
            SyntheticCase::Mixed => rng.pct() < cfg.hot_ratio,
        };
        let session = if choose_hot || cold_sessions == 0 {
            (rng.next_usize(cfg.hot_sessions) + 1) as u64
        } else {
            (cfg.hot_sessions + rng.next_usize(cold_sessions) + 1) as u64
        };
        let kind_pick = (rng.next_u64() % ratio_total as u64) as u32;
        if kind_pick < cfg.recv_ratio {
            let len = rng.next_usize(cfg.max_bytes) + 1;
            let shard = if reshared {
                remap_shard(session, shard_count)
            } else {
                (session as usize) % shard_count
            };
            push_workload_op(
                shard,
                (0u8, session),
                ReplayKind::Recv { len },
                recv_weight,
                send_weight,
                accept_weight,
                &mut parsed_ops,
                &mut parsed_ops_by_shard,
                &mut count_ops_maps,
                &mut shard_slot_ids,
                &mut shard_slot_counts,
            );
            continue;
        }
        if kind_pick < cfg.recv_ratio + cfg.send_ratio {
            let bytes = rng.next_usize(cfg.max_bytes) + 1;
            let shard = if reshared {
                remap_shard(session, shard_count)
            } else {
                (session as usize) % shard_count
            };
            push_workload_op(
                shard,
                (0u8, session),
                ReplayKind::Send { bytes },
                recv_weight,
                send_weight,
                accept_weight,
                &mut parsed_ops,
                &mut parsed_ops_by_shard,
                &mut count_ops_maps,
                &mut shard_slot_ids,
                &mut shard_slot_counts,
            );
            continue;
        }
        let listener = (rng.next_usize(listener_count) + 1) as u64;
        let shard = if reshared {
            remap_shard(listener, shard_count)
        } else {
            (listener as usize) % shard_count
        };
        push_workload_op(
            shard,
            (1u8, listener),
            ReplayKind::Accept { timeout_ms: 0 },
            recv_weight,
            send_weight,
            accept_weight,
            &mut parsed_ops,
            &mut parsed_ops_by_shard,
            &mut count_ops_maps,
            &mut shard_slot_ids,
            &mut shard_slot_counts,
        );
    }

    let count_ops_by_shard = count_ops_maps
        .into_iter()
        .map(|m| m.into_values().collect())
        .collect();
    ReplayWorkload {
        parsed_ops,
        parsed_ops_by_shard,
        count_ops_by_shard,
        shard_slot_counts,
        reply_ok: 0,
        reply_error: 0,
        reply_timeout: 0,
    }
}

#[allow(clippy::too_many_arguments)]
fn push_workload_op(
    shard: usize,
    key: (u8, u64),
    kind: ReplayKind,
    recv_weight: usize,
    send_weight: usize,
    accept_weight: usize,
    parsed_ops: &mut Vec<(usize, ScheduledOp)>,
    parsed_ops_by_shard: &mut [Vec<ScheduledOp>],
    count_ops_maps: &mut [HashMap<usize, CountOp>],
    shard_slot_ids: &mut [HashMap<(u8, u64), usize>],
    shard_slot_counts: &mut [usize],
) {
    let slot = *shard_slot_ids[shard].entry(key).or_insert_with(|| {
        let next = shard_slot_counts[shard];
        shard_slot_counts[shard] += 1;
        next
    });
    let op = ScheduledOp { slot, kind };
    parsed_ops.push((shard, op));
    parsed_ops_by_shard[shard].push(op);

    let entry = count_ops_maps[shard].entry(slot).or_insert(CountOp {
        slot,
        count: 0,
        weight: 0,
        recv_ops: 0,
        recv_bytes: 0,
        send_ops: 0,
        send_bytes: 0,
        accept_ops: 0,
    });
    entry.count += 1;
    match kind {
        ReplayKind::Recv { len } => {
            entry.weight += recv_weight;
            entry.recv_ops += 1;
            entry.recv_bytes += len as u64;
        }
        ReplayKind::Send { bytes } => {
            entry.weight += send_weight;
            entry.send_ops += 1;
            entry.send_bytes += bytes as u64;
        }
        ReplayKind::Accept { .. } => {
            entry.weight += accept_weight;
            entry.accept_ops += 1;
        }
    }
}

fn remap_shard(id: u64, shard_count: usize) -> usize {
    let mixed = id.wrapping_mul(0x9E37_79B9_7F4A_7C15);
    (mixed as usize) % shard_count
}

#[allow(clippy::too_many_arguments)]
fn worker_loop(
    ingress: LocalIngress<ScheduledOp>,
    pending: Arc<AtomicUsize>,
    notified: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
    thread_tx: std::sync::mpsc::SyncSender<Thread>,
    slot_count: usize,
    session_run_budget: usize,
    drain_session: bool,
    cheap_syscall: CheapSyscall,
    initial_ops: Vec<ScheduledOp>,
    start_barrier: Option<Arc<Barrier>>,
) -> ShardStats {
    let _ = thread_tx.send(thread::current());
    let mut local_exec = LocalExec::new(slot_count);
    let mut stats = ShardStats::default();
    if !initial_ops.is_empty() {
        for op in initial_ops {
            local_exec.push(op.slot, op.kind);
        }
    }
    if let Some(barrier) = start_barrier {
        barrier.wait();
    }
    loop {
        while let Some(idx) = ingress.pop_ready() {
            let batch_len = ingress.with_slot(idx, |batch| {
                let batch_len = batch.len();
                for op in batch.drain(..) {
                    local_exec.push(op.slot, op.kind);
                }
                batch_len
            });
            ingress.release(idx);
            pending.fetch_sub(batch_len, Ordering::AcqRel);
        }

        let progressed = local_exec.drain(
            RUNNABLE_DRAIN_BUDGET,
            session_run_budget,
            drain_session,
            |op| match op {
                ReplayKind::Recv { len } => {
                    run_cheap_syscall(cheap_syscall);
                    stats.recv_ops += 1;
                    stats.recv_bytes += len as u64;
                }
                ReplayKind::Send { bytes } => {
                    run_cheap_syscall(cheap_syscall);
                    stats.send_ops += 1;
                    stats.send_bytes += bytes as u64;
                }
                ReplayKind::Accept { timeout_ms } => {
                    let _ = timeout_ms;
                    run_cheap_syscall(cheap_syscall);
                    stats.accept_ops += 1;
                }
            },
        );

        let has_local_work = local_exec.has_work();
        if done.load(Ordering::Acquire) && pending.load(Ordering::Acquire) == 0 && !has_local_work {
            break;
        }
        if progressed == 0
            && pending.load(Ordering::Acquire) == 0
            && !has_local_work
            && !notified.swap(false, Ordering::AcqRel)
        {
            thread::park_timeout(Duration::from_micros(50));
        }
    }
    stats
}

fn publish_batch_to_worker(worker: &Worker, batch: &mut Vec<ScheduledOp>) {
    if batch.is_empty() {
        return;
    }
    let count = worker.ingress.publish_from_staging(batch);
    if worker.pending.fetch_add(count, Ordering::AcqRel) == 0 {
        worker.notified.store(true, Ordering::Release);
        worker.thread.unpark();
    }
}

#[allow(clippy::too_many_arguments)]
fn worker_loop_counts(
    ingress: LocalIngress<CountOp>,
    pending: Arc<AtomicUsize>,
    notified: Arc<AtomicBool>,
    done: Arc<AtomicBool>,
    thread_tx: std::sync::mpsc::SyncSender<Thread>,
    slot_count: usize,
    session_run_budget: usize,
    drain_session: bool,
    cheap_syscall: CheapSyscall,
    initial_ops: Vec<CountOp>,
    start_barrier: Option<Arc<Barrier>>,
) -> ShardStats {
    let _ = thread_tx.send(thread::current());
    let mut local_exec = LocalExecCounts::new(slot_count);
    let mut stats = ShardStats::default();
    if !initial_ops.is_empty() {
        for op in initial_ops {
            stats.recv_ops += op.recv_ops;
            stats.recv_bytes += op.recv_bytes;
            stats.send_ops += op.send_ops;
            stats.send_bytes += op.send_bytes;
            stats.accept_ops += op.accept_ops;
            local_exec.push_count(op.slot, op.count);
        }
    }
    if let Some(barrier) = start_barrier {
        barrier.wait();
    }
    loop {
        while let Some(idx) = ingress.pop_ready() {
            let batch_len = ingress.with_slot(idx, |batch| {
                let batch_len: usize = batch.iter().map(|op| op.count).sum();
                for op in batch.drain(..) {
                    stats.recv_ops += op.recv_ops;
                    stats.recv_bytes += op.recv_bytes;
                    stats.send_ops += op.send_ops;
                    stats.send_bytes += op.send_bytes;
                    stats.accept_ops += op.accept_ops;
                    local_exec.push_count(op.slot, op.count);
                }
                batch_len
            });
            ingress.release(idx);
            pending.fetch_sub(batch_len, Ordering::AcqRel);
        }

        let progressed = local_exec.drain(
            RUNNABLE_DRAIN_BUDGET,
            session_run_budget,
            drain_session,
            || run_cheap_syscall(cheap_syscall),
        );

        let has_local_work = local_exec.has_work();
        if done.load(Ordering::Acquire) && pending.load(Ordering::Acquire) == 0 && !has_local_work {
            break;
        }
        if progressed == 0
            && pending.load(Ordering::Acquire) == 0
            && !has_local_work
            && !notified.swap(false, Ordering::AcqRel)
        {
            thread::park_timeout(Duration::from_micros(50));
        }
    }
    stats
}

fn publish_count_batch_to_worker(worker: &CountWorker, batch: &mut Vec<CountOp>) {
    if batch.is_empty() {
        return;
    }
    let total_count: usize = batch.iter().map(|op| op.count).sum();
    let _published_items = worker.ingress.publish_from_staging(batch);
    if worker.pending.fetch_add(total_count, Ordering::AcqRel) == 0 {
        worker.notified.store(true, Ordering::Release);
        worker.thread.unpark();
    }
}

fn scheduled_op_weight(
    op: &ScheduledOp,
    recv_weight: usize,
    send_weight: usize,
    accept_weight: usize,
) -> usize {
    match op.kind {
        ReplayKind::Recv { .. } => recv_weight,
        ReplayKind::Send { .. } => send_weight,
        ReplayKind::Accept { .. } => accept_weight,
    }
}

#[inline(always)]
fn run_cheap_syscall(mode: CheapSyscall) {
    match mode {
        CheapSyscall::None => {}
        CheapSyscall::GetTid => {
            let tid = unsafe { libc::syscall(libc::SYS_gettid) };
            std::hint::black_box(tid);
        }
    }
}

fn parse_line(line: &str) -> TraceEvent {
    let mut fields = HashMap::new();
    for part in line.split('\t') {
        if let Some((k, v)) = part.split_once('=') {
            fields.insert(k, v);
        }
    }
    match fields.get("event").copied().unwrap_or("") {
        "recv_req" => match (
            parse_usize(fields.get("shard").copied()),
            parse_u64(fields.get("session").copied()),
            parse_usize(fields.get("len").copied()),
        ) {
            (Some(shard), Some(session), Some(len)) => TraceEvent::RecvReq {
                shard,
                session,
                len,
            },
            _ => TraceEvent::Other,
        },
        "send_req" => match (
            parse_usize(fields.get("shard").copied()),
            parse_u64(fields.get("session").copied()),
            parse_usize(fields.get("bytes").copied()),
        ) {
            (Some(shard), Some(session), Some(bytes)) => TraceEvent::SendReq {
                shard,
                session,
                bytes,
            },
            _ => TraceEvent::Other,
        },
        "accept_req" => match (
            parse_u64(fields.get("listener").copied()),
            parse_i64(fields.get("timeout_ms").copied()),
        ) {
            (Some(listener), Some(timeout_ms)) => TraceEvent::AcceptReq {
                listener,
                timeout_ms,
            },
            _ => TraceEvent::Other,
        },
        "reply_ok" => TraceEvent::ReplyOk {
            request_id: parse_u64(fields.get("request_id").copied()).unwrap_or(0),
            bytes: parse_usize(fields.get("bytes").copied()).unwrap_or(0),
        },
        "reply_error" => TraceEvent::ReplyError {
            request_id: parse_u64(fields.get("request_id").copied()).unwrap_or(0),
        },
        "reply_timeout" => TraceEvent::ReplyTimeout {
            request_id: parse_u64(fields.get("request_id").copied()).unwrap_or(0),
        },
        _ => TraceEvent::Other,
    }
}

fn parse_u64(v: Option<&str>) -> Option<u64> {
    v.and_then(|s| s.parse::<u64>().ok())
}

fn parse_usize(v: Option<&str>) -> Option<usize> {
    v.and_then(|s| s.parse::<usize>().ok())
}

fn parse_i64(v: Option<&str>) -> Option<i64> {
    v.and_then(|s| s.parse::<i64>().ok())
}

fn parse_env_usize(key: &str, default: usize) -> usize {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(default)
}

fn parse_env_u32(key: &str, default: u32) -> u32 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or(default)
}

fn parse_env_u64(key: &str, default: u64) -> u64 {
    std::env::var(key)
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(default)
}
