use std::thread;
use std::time::Instant;

const DEFAULT_SHARDS: usize = 1;
const DEFAULT_OPS_PER_SHARD: usize = 50_000_000;

fn main() {
    let shards = parse_env_usize("GETTID_LOOP_SHARDS", DEFAULT_SHARDS);
    let ops_per_shard = parse_env_usize("GETTID_LOOP_OPS", DEFAULT_OPS_PER_SHARD);

    println!(
        "gettid_loop_bench shards={} ops_per_shard={}",
        shards, ops_per_shard
    );

    let start = Instant::now();
    let mut joins = Vec::with_capacity(shards);
    for shard in 0..shards {
        joins.push(
            thread::Builder::new()
                .name(format!("gettid-loop-{shard}"))
                .spawn(move || {
                    pin_current_thread(shard);
                    run_shard(ops_per_shard)
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

fn run_shard(ops: usize) -> usize {
    for _ in 0..ops {
        let tid = unsafe { libc::syscall(libc::SYS_gettid) };
        std::hint::black_box(tid);
    }
    ops
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
