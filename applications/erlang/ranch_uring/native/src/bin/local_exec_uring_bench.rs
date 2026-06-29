use std::thread;
use std::time::Instant;

use io_uring::{opcode, IoUring};
use ranch_uring_nif::local_exec::LocalExec;
use ranch_uring_nif::replay_protocol::{ReplayKind, ScheduledOp};

const DEFAULT_SHARDS: usize = 1;
const DEFAULT_SLOTS_PER_SHARD: usize = 1024;
const DEFAULT_OPS_PER_SLOT: usize = 100_000;
const DEFAULT_SESSION_RUN_BUDGET: usize = 64;
const DEFAULT_RUNNABLE_DRAIN_BUDGET: usize = 256;
const DEFAULT_RING_ENTRIES: u32 = 256;
const DEFAULT_SUBMIT_BATCH: usize = 1;

fn main() {
    let shards = parse_env_usize("LOCAL_EXEC_SHARDS", DEFAULT_SHARDS);
    let slots_per_shard = parse_env_usize("LOCAL_EXEC_SLOTS", DEFAULT_SLOTS_PER_SHARD);
    let ops_per_slot = parse_env_usize("LOCAL_EXEC_OPS_PER_SLOT", DEFAULT_OPS_PER_SLOT);
    let session_run_budget =
        parse_env_usize("LOCAL_EXEC_SESSION_BUDGET", DEFAULT_SESSION_RUN_BUDGET);
    let runnable_drain_budget =
        parse_env_usize("LOCAL_EXEC_RUNNABLE_BUDGET", DEFAULT_RUNNABLE_DRAIN_BUDGET);
    let drain_session = std::env::var("LOCAL_EXEC_DRAIN_SESSION").ok().as_deref() == Some("1");
    let ring_entries =
        parse_env_usize("LOCAL_EXEC_URING_ENTRIES", DEFAULT_RING_ENTRIES as usize) as u32;
    let submit_batch = parse_env_usize("LOCAL_EXEC_URING_BATCH", DEFAULT_SUBMIT_BATCH);

    println!(
        "local_exec_uring_bench shards={} slots_per_shard={} ops_per_slot={} session_budget={} runnable_budget={} drain_session={} ring_entries={} submit_batch={}",
        shards, slots_per_shard, ops_per_slot, session_run_budget, runnable_drain_budget, drain_session, ring_entries, submit_batch
    );

    let start = Instant::now();
    let mut joins = Vec::with_capacity(shards);
    for shard in 0..shards {
        joins.push(
            thread::Builder::new()
                .name(format!("local-exec-uring-{shard}"))
                .spawn(move || {
                    pin_current_thread(shard);
                    run_shard(
                        slots_per_shard,
                        ops_per_slot,
                        session_run_budget,
                        runnable_drain_budget,
                        drain_session,
                        ring_entries,
                        submit_batch,
                    )
                })
                .expect("spawn shard"),
        );
    }

    let mut total_ops = 0usize;
    for (shard, join) in joins.into_iter().enumerate() {
        let shard_ops = join.join().expect("join shard");
        total_ops += shard_ops;
        println!("shard={} ops={}", shard, shard_ops);
    }
    let elapsed = start.elapsed();
    let ops_per_sec = total_ops as f64 / elapsed.as_secs_f64();
    println!(
        "total_ops={} elapsed_ms={:.2} ops_per_sec={:.0}",
        total_ops,
        elapsed.as_secs_f64() * 1000.0,
        ops_per_sec
    );
}

fn run_shard(
    slots_per_shard: usize,
    ops_per_slot: usize,
    session_run_budget: usize,
    runnable_drain_budget: usize,
    _drain_session: bool,
    ring_entries: u32,
    submit_batch: usize,
) -> usize {
    let mut local_exec = LocalExec::new(slots_per_shard);
    for slot in 0..slots_per_shard {
        for _ in 0..ops_per_slot {
            let op = ScheduledOp {
                slot,
                kind: ReplayKind::Send { bytes: 64 },
            };
            local_exec.push(op.slot, op.kind);
        }
    }

    let mut ring = IoUring::new(ring_entries).expect("create io_uring");
    let mut completed = 0usize;
    let mut in_flight = 0usize;
    while local_exec.has_work() || in_flight != 0 {
        if local_exec.has_work() && in_flight < submit_batch {
            let room = submit_batch - in_flight;
            let mut progressed = 0usize;
            local_exec.drain(
                room.min(runnable_drain_budget),
                room.min(session_run_budget),
                false,
                |_op| {
                    progressed += 1;
                },
            );
            if progressed != 0 {
                submit_nops(&mut ring, progressed);
                in_flight += progressed;
            }
        }

        if in_flight != 0 {
            ring.submit_and_wait(1).expect("submit_and_wait");
            let mut cq = ring.completion();
            for cqe in &mut cq {
                std::hint::black_box(cqe.user_data());
                completed += 1;
                in_flight -= 1;
            }
        }
    }
    completed
}

fn submit_nops(ring: &mut IoUring, count: usize) {
    let mut submitted = 0usize;
    while submitted < count {
        {
            let mut sq = ring.submission();
            sq.sync();
            while submitted < count {
                let entry = opcode::Nop::new().build().user_data(0);
                if unsafe { sq.push(&entry) }.is_err() {
                    break;
                }
                submitted += 1;
            }
        }
        ring.submit().expect("submit nops");
    }
}

fn parse_env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|value| value.parse::<usize>().ok())
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
