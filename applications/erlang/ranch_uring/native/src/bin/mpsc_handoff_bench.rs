use std::sync::{Arc, Barrier};
use std::thread;
use std::time::Instant;

use crossbeam_channel as cb;

const DEFAULT_LOGICAL_OPS: usize = 20_000_000;
const DEFAULT_BATCH: usize = 32;

#[derive(Clone, Copy, Debug)]
enum Backend {
    Std,
    StdSync,
    CrossbeamBounded,
    CrossbeamUnbounded,
}

#[derive(Clone, Copy, Debug)]
enum Shape {
    Scalar,
    Batch,
    Payload64,
}

#[derive(Clone, Copy)]
struct Batch32 {
    len: usize,
    items: [u64; DEFAULT_BATCH],
}

#[derive(Clone, Copy)]
struct Payload64 {
    words: [u64; 8],
}

fn main() {
    let backend = parse_backend();
    let shape = parse_shape();
    let logical_ops = parse_env_usize("MPSC_LOGICAL_OPS", DEFAULT_LOGICAL_OPS);
    let batch = parse_env_usize("MPSC_BATCH", DEFAULT_BATCH);
    assert!(
        batch > 0 && batch <= DEFAULT_BATCH,
        "MPSC_BATCH must be 1..={DEFAULT_BATCH}"
    );
    let queue_cap = parse_env_usize("MPSC_CAPACITY", 1024);

    println!(
        "mpsc_handoff_bench backend={backend:?} shape={shape:?} logical_ops={logical_ops} batch={batch} capacity={queue_cap}"
    );

    let started = Instant::now();
    let total = match (backend, shape) {
        (Backend::Std, Shape::Scalar) => run_std_scalar(logical_ops),
        (Backend::Std, Shape::Batch) => run_std_batch(logical_ops, batch),
        (Backend::Std, Shape::Payload64) => run_std_payload64(logical_ops),
        (Backend::StdSync, Shape::Scalar) => run_std_sync_scalar(logical_ops, queue_cap),
        (Backend::StdSync, Shape::Batch) => run_std_sync_batch(logical_ops, batch, queue_cap),
        (Backend::StdSync, Shape::Payload64) => run_std_sync_payload64(logical_ops, queue_cap),
        (Backend::CrossbeamBounded, Shape::Scalar) => run_cb_bounded_scalar(logical_ops, queue_cap),
        (Backend::CrossbeamBounded, Shape::Batch) => {
            run_cb_bounded_batch(logical_ops, batch, queue_cap)
        }
        (Backend::CrossbeamBounded, Shape::Payload64) => {
            run_cb_bounded_payload64(logical_ops, queue_cap)
        }
        (Backend::CrossbeamUnbounded, Shape::Scalar) => run_cb_unbounded_scalar(logical_ops),
        (Backend::CrossbeamUnbounded, Shape::Batch) => run_cb_unbounded_batch(logical_ops, batch),
        (Backend::CrossbeamUnbounded, Shape::Payload64) => run_cb_unbounded_payload64(logical_ops),
    };
    let elapsed = started.elapsed();
    let ops_per_sec = total as f64 / elapsed.as_secs_f64();
    println!(
        "logical_ops={} elapsed_ms={:.2} ops_per_sec={:.0}",
        total,
        elapsed.as_secs_f64() * 1000.0,
        ops_per_sec
    );
}

fn run_std_scalar(logical_ops: usize) -> usize {
    let (tx, rx) = std::sync::mpsc::channel::<u64>();
    let barrier = Arc::new(Barrier::new(2));
    let consumer_barrier = barrier.clone();
    let consumer = thread::Builder::new()
        .name("mpsc-consumer".into())
        .spawn(move || {
            pin_current_thread(1);
            consumer_barrier.wait();
            let mut seen = 0usize;
            while seen < logical_ops {
                let item = rx.recv().expect("recv");
                std::hint::black_box(item);
                seen += 1;
            }
            seen
        })
        .expect("spawn consumer");

    pin_current_thread(0);
    barrier.wait();
    for i in 0..logical_ops {
        tx.send(i as u64).expect("send");
    }
    consumer.join().expect("join consumer")
}

fn run_std_batch(logical_ops: usize, batch: usize) -> usize {
    let batches = logical_ops.div_ceil(batch);
    let (tx, rx) = std::sync::mpsc::channel::<Batch32>();
    let barrier = Arc::new(Barrier::new(2));
    let consumer_barrier = barrier.clone();
    let consumer = thread::Builder::new()
        .name("mpsc-consumer".into())
        .spawn(move || {
            pin_current_thread(1);
            consumer_barrier.wait();
            let mut seen = 0usize;
            while seen < logical_ops {
                let batch_msg = rx.recv().expect("recv");
                for idx in 0..batch_msg.len {
                    std::hint::black_box(batch_msg.items[idx]);
                }
                seen += batch_msg.len;
            }
            seen
        })
        .expect("spawn consumer");

    pin_current_thread(0);
    barrier.wait();
    for b in 0..batches {
        tx.send(make_batch(b * batch, logical_ops, batch))
            .expect("send");
    }
    consumer.join().expect("join consumer")
}

fn run_std_payload64(logical_ops: usize) -> usize {
    let (tx, rx) = std::sync::mpsc::channel::<Payload64>();
    let barrier = Arc::new(Barrier::new(2));
    let consumer_barrier = barrier.clone();
    let consumer = thread::Builder::new()
        .name("mpsc-consumer".into())
        .spawn(move || {
            pin_current_thread(1);
            consumer_barrier.wait();
            let mut seen = 0usize;
            while seen < logical_ops {
                let item = rx.recv().expect("recv");
                std::hint::black_box(item.words[0]);
                seen += 1;
            }
            seen
        })
        .expect("spawn consumer");

    pin_current_thread(0);
    barrier.wait();
    for i in 0..logical_ops {
        tx.send(make_payload64(i)).expect("send");
    }
    consumer.join().expect("join consumer")
}

fn run_std_sync_scalar(logical_ops: usize, cap: usize) -> usize {
    let (tx, rx) = std::sync::mpsc::sync_channel::<u64>(cap);
    run_sync_scalar(logical_ops, tx, rx)
}

fn run_std_sync_batch(logical_ops: usize, batch: usize, cap: usize) -> usize {
    let (tx, rx) = std::sync::mpsc::sync_channel::<Batch32>(cap);
    run_sync_batch(logical_ops, batch, tx, rx)
}

fn run_std_sync_payload64(logical_ops: usize, cap: usize) -> usize {
    let (tx, rx) = std::sync::mpsc::sync_channel::<Payload64>(cap);
    let barrier = Arc::new(Barrier::new(2));
    let consumer_barrier = barrier.clone();
    let consumer = thread::Builder::new()
        .name("mpsc-consumer".into())
        .spawn(move || {
            pin_current_thread(1);
            consumer_barrier.wait();
            let mut seen = 0usize;
            while seen < logical_ops {
                let item = rx.recv().expect("recv");
                std::hint::black_box(item.words[0]);
                seen += 1;
            }
            seen
        })
        .expect("spawn consumer");

    pin_current_thread(0);
    barrier.wait();
    for i in 0..logical_ops {
        tx.send(make_payload64(i)).expect("send");
    }
    consumer.join().expect("join consumer")
}

fn run_sync_scalar<TX>(logical_ops: usize, tx: TX, rx: std::sync::mpsc::Receiver<u64>) -> usize
where
    TX: Send + 'static + FnSendU64,
{
    let barrier = Arc::new(Barrier::new(2));
    let consumer_barrier = barrier.clone();
    let consumer = thread::Builder::new()
        .name("mpsc-consumer".into())
        .spawn(move || {
            pin_current_thread(1);
            consumer_barrier.wait();
            let mut seen = 0usize;
            while seen < logical_ops {
                let item = rx.recv().expect("recv");
                std::hint::black_box(item);
                seen += 1;
            }
            seen
        })
        .expect("spawn consumer");

    pin_current_thread(0);
    barrier.wait();
    for i in 0..logical_ops {
        tx.send_u64(i as u64);
    }
    consumer.join().expect("join consumer")
}

fn run_sync_batch<TX>(
    logical_ops: usize,
    batch: usize,
    tx: TX,
    rx: std::sync::mpsc::Receiver<Batch32>,
) -> usize
where
    TX: Send + 'static + FnSendBatch,
{
    let batches = logical_ops.div_ceil(batch);
    let barrier = Arc::new(Barrier::new(2));
    let consumer_barrier = barrier.clone();
    let consumer = thread::Builder::new()
        .name("mpsc-consumer".into())
        .spawn(move || {
            pin_current_thread(1);
            consumer_barrier.wait();
            let mut seen = 0usize;
            while seen < logical_ops {
                let batch_msg = rx.recv().expect("recv");
                for idx in 0..batch_msg.len {
                    std::hint::black_box(batch_msg.items[idx]);
                }
                seen += batch_msg.len;
            }
            seen
        })
        .expect("spawn consumer");

    pin_current_thread(0);
    barrier.wait();
    for b in 0..batches {
        tx.send_batch(make_batch(b * batch, logical_ops, batch));
    }
    consumer.join().expect("join consumer")
}

fn run_cb_bounded_scalar(logical_ops: usize, cap: usize) -> usize {
    let (tx, rx) = cb::bounded::<u64>(cap);
    let barrier = Arc::new(Barrier::new(2));
    let consumer_barrier = barrier.clone();
    let consumer = thread::Builder::new()
        .name("cb-consumer".into())
        .spawn(move || {
            pin_current_thread(1);
            consumer_barrier.wait();
            let mut seen = 0usize;
            while seen < logical_ops {
                let item = rx.recv().expect("recv");
                std::hint::black_box(item);
                seen += 1;
            }
            seen
        })
        .expect("spawn consumer");

    pin_current_thread(0);
    barrier.wait();
    for i in 0..logical_ops {
        tx.send(i as u64).expect("send");
    }
    consumer.join().expect("join consumer")
}

fn run_cb_bounded_batch(logical_ops: usize, batch: usize, cap: usize) -> usize {
    let batches = logical_ops.div_ceil(batch);
    let (tx, rx) = cb::bounded::<Batch32>(cap);
    let barrier = Arc::new(Barrier::new(2));
    let consumer_barrier = barrier.clone();
    let consumer = thread::Builder::new()
        .name("cb-consumer".into())
        .spawn(move || {
            pin_current_thread(1);
            consumer_barrier.wait();
            let mut seen = 0usize;
            while seen < logical_ops {
                let batch_msg = rx.recv().expect("recv");
                for idx in 0..batch_msg.len {
                    std::hint::black_box(batch_msg.items[idx]);
                }
                seen += batch_msg.len;
            }
            seen
        })
        .expect("spawn consumer");

    pin_current_thread(0);
    barrier.wait();
    for b in 0..batches {
        tx.send(make_batch(b * batch, logical_ops, batch))
            .expect("send");
    }
    consumer.join().expect("join consumer")
}

fn run_cb_bounded_payload64(logical_ops: usize, cap: usize) -> usize {
    let (tx, rx) = cb::bounded::<Payload64>(cap);
    let barrier = Arc::new(Barrier::new(2));
    let consumer_barrier = barrier.clone();
    let consumer = thread::Builder::new()
        .name("cb-consumer".into())
        .spawn(move || {
            pin_current_thread(1);
            consumer_barrier.wait();
            let mut seen = 0usize;
            while seen < logical_ops {
                let item = rx.recv().expect("recv");
                std::hint::black_box(item.words[0]);
                seen += 1;
            }
            seen
        })
        .expect("spawn consumer");

    pin_current_thread(0);
    barrier.wait();
    for i in 0..logical_ops {
        tx.send(make_payload64(i)).expect("send");
    }
    consumer.join().expect("join consumer")
}

fn run_cb_unbounded_scalar(logical_ops: usize) -> usize {
    let (tx, rx) = cb::unbounded::<u64>();
    let barrier = Arc::new(Barrier::new(2));
    let consumer_barrier = barrier.clone();
    let consumer = thread::Builder::new()
        .name("cb-consumer".into())
        .spawn(move || {
            pin_current_thread(1);
            consumer_barrier.wait();
            let mut seen = 0usize;
            while seen < logical_ops {
                let item = rx.recv().expect("recv");
                std::hint::black_box(item);
                seen += 1;
            }
            seen
        })
        .expect("spawn consumer");

    pin_current_thread(0);
    barrier.wait();
    for i in 0..logical_ops {
        tx.send(i as u64).expect("send");
    }
    consumer.join().expect("join consumer")
}

fn run_cb_unbounded_batch(logical_ops: usize, batch: usize) -> usize {
    let batches = logical_ops.div_ceil(batch);
    let (tx, rx) = cb::unbounded::<Batch32>();
    let barrier = Arc::new(Barrier::new(2));
    let consumer_barrier = barrier.clone();
    let consumer = thread::Builder::new()
        .name("cb-consumer".into())
        .spawn(move || {
            pin_current_thread(1);
            consumer_barrier.wait();
            let mut seen = 0usize;
            while seen < logical_ops {
                let batch_msg = rx.recv().expect("recv");
                for idx in 0..batch_msg.len {
                    std::hint::black_box(batch_msg.items[idx]);
                }
                seen += batch_msg.len;
            }
            seen
        })
        .expect("spawn consumer");

    pin_current_thread(0);
    barrier.wait();
    for b in 0..batches {
        tx.send(make_batch(b * batch, logical_ops, batch))
            .expect("send");
    }
    consumer.join().expect("join consumer")
}

fn run_cb_unbounded_payload64(logical_ops: usize) -> usize {
    let (tx, rx) = cb::unbounded::<Payload64>();
    let barrier = Arc::new(Barrier::new(2));
    let consumer_barrier = barrier.clone();
    let consumer = thread::Builder::new()
        .name("cb-consumer".into())
        .spawn(move || {
            pin_current_thread(1);
            consumer_barrier.wait();
            let mut seen = 0usize;
            while seen < logical_ops {
                let item = rx.recv().expect("recv");
                std::hint::black_box(item.words[0]);
                seen += 1;
            }
            seen
        })
        .expect("spawn consumer");

    pin_current_thread(0);
    barrier.wait();
    for i in 0..logical_ops {
        tx.send(make_payload64(i)).expect("send");
    }
    consumer.join().expect("join consumer")
}

trait FnSendU64 {
    fn send_u64(&self, value: u64);
}

impl FnSendU64 for std::sync::mpsc::SyncSender<u64> {
    fn send_u64(&self, value: u64) {
        self.send(value).expect("send");
    }
}

trait FnSendBatch {
    fn send_batch(&self, value: Batch32);
}

impl FnSendBatch for std::sync::mpsc::SyncSender<Batch32> {
    fn send_batch(&self, value: Batch32) {
        self.send(value).expect("send");
    }
}

fn make_batch(base: usize, logical_ops: usize, batch: usize) -> Batch32 {
    let len = (logical_ops - base).min(batch);
    let mut items = [0u64; DEFAULT_BATCH];
    let mut idx = 0usize;
    while idx < len {
        items[idx] = (base + idx) as u64;
        idx += 1;
    }
    Batch32 { len, items }
}

fn parse_backend() -> Backend {
    match std::env::var("MPSC_BACKEND").ok().as_deref() {
        Some("std") => Backend::Std,
        Some("std_sync") => Backend::StdSync,
        Some("crossbeam_bounded") => Backend::CrossbeamBounded,
        Some("crossbeam_unbounded") => Backend::CrossbeamUnbounded,
        _ => Backend::CrossbeamBounded,
    }
}

fn parse_shape() -> Shape {
    match std::env::var("MPSC_SHAPE").ok().as_deref() {
        Some("batch") => Shape::Batch,
        Some("payload64") => Shape::Payload64,
        _ => Shape::Scalar,
    }
}

fn make_payload64(i: usize) -> Payload64 {
    Payload64 {
        words: [
            i as u64,
            (i as u64).wrapping_add(1),
            (i as u64).wrapping_add(2),
            (i as u64).wrapping_add(3),
            (i as u64).wrapping_add(4),
            (i as u64).wrapping_add(5),
            (i as u64).wrapping_add(6),
            (i as u64).wrapping_add(7),
        ],
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
