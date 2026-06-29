//! Bench-only helper internals for NIF testing support.
//!
//! These types and functions are separated from NIF entrypoints so that the
//! benchmark helpers remain clearly visible. They are not part of the public
//! runtime API.

use std::collections::VecDeque;
use std::sync::atomic::{AtomicU64, AtomicUsize, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::Arc;
use std::thread;

use once_cell::sync::OnceCell;
use rustler::{Binary, Decoder, Encoder, LocalPid, NewBinary, OwnedBinary, OwnedEnv, Term};

use crate::atoms;

// --- Bench message types ---

// Note: Debug is not derived for BenchMsg variants containing LocalPid
// because LocalPid does not implement Debug.
pub enum BenchMsg {
    Ok(LocalPid),
    Binary(LocalPid, usize),
    ZeroBinary(LocalPid, usize),
    ZeroMany(LocalPid, usize, usize),
    ZeroBatch(LocalPid, usize, usize),
}

#[derive(Debug)]
pub enum BenchQueueBatchOp {
    Read { id: u64, size: usize },
    Write { id: u64, data: Vec<u8> },
}

pub struct BenchQueueRequest {
    pub request_id: u64,
    pub reply_pid: LocalPid,
    pub ops: Vec<BenchQueueBatchOp>,
}

pub struct BenchQueueWorker {
    pub tx: Sender<BenchQueueRequest>,
    pub next_request_id: AtomicU64,
    pub pending: Arc<AtomicUsize>,
}

#[derive(Debug, Clone, Copy)]
pub enum BenchOp {
    Read,
    Write,
}

impl<'a> Decoder<'a> for BenchOp {
    fn decode(term: Term<'a>) -> rustler::NifResult<Self> {
        let atom = term.decode::<rustler::Atom>()?;
        if atom == atoms::read() {
            Ok(Self::Read)
        } else if atom == atoms::write() {
            Ok(Self::Write)
        } else {
            Err(rustler::Error::BadArg)
        }
    }
}

#[derive(Debug, Clone, Copy)]
pub enum SessionBatchOp {
    Read,
    Write,
}

impl<'a> Decoder<'a> for SessionBatchOp {
    fn decode(term: Term<'a>) -> rustler::NifResult<Self> {
        let atom = term.decode::<rustler::Atom>()?;
        if atom == atoms::read() {
            Ok(Self::Read)
        } else if atom == atoms::write() || atom == atoms::writev() {
            Ok(Self::Write)
        } else {
            Err(rustler::Error::BadArg)
        }
    }
}

// --- Global worker handles ---

static BENCH_WORKER: OnceCell<Option<Sender<BenchMsg>>> = OnceCell::new();
static BENCH_QUEUE_WORKER: OnceCell<Option<BenchQueueWorker>> = OnceCell::new();

// --- Helper functions ---

/// Sends an `{error, error}` tuple to the given pid using the provided OwnedEnv.
pub fn send_bench_error(env: &mut OwnedEnv, pid: LocalPid) {
    let _ = env.send_and_clear(&pid, |env| (atoms::error(), atoms::error()).encode(env));
}

/// Creates an owned binary of the given size filled with `fill` byte.
pub fn bench_owned_binary(size: usize, fill: u8) -> Option<OwnedBinary> {
    let mut bin = OwnedBinary::new(size)?;
    bin.as_mut_slice().fill(fill);
    Some(bin)
}

/// Returns a reference to the global bench worker sender, initializing it if needed.
pub fn bench_worker() -> Option<&'static Sender<BenchMsg>> {
    BENCH_WORKER
        .get_or_init(|| {
            let (tx, rx) = mpsc::channel::<BenchMsg>();
            let spawn_ok = thread::Builder::new()
                .name("bench_owned_env".into())
                .spawn(move || {
                    let mut env = OwnedEnv::new();
                    while let Ok(msg) = rx.recv() {
                        match msg {
                            BenchMsg::Ok(pid) => {
                                let _ = env
                                    .send_and_clear(&pid, |env| atoms::bench_reply().encode(env));
                            }
                            BenchMsg::Binary(pid, size) => {
                                let bin = match bench_owned_binary(size, 0x5a) {
                                    Some(bin) => bin,
                                    None => {
                                        send_bench_error(&mut env, pid);
                                        continue;
                                    }
                                };
                                let _ = env.send_and_clear(&pid, |env| {
                                    (atoms::bench_reply_bin(), Binary::from_owned(bin, env))
                                        .encode(env)
                                });
                            }
                            BenchMsg::ZeroBinary(pid, size) => {
                                let bin = match bench_owned_binary(size, 0) {
                                    Some(bin) => bin,
                                    None => {
                                        send_bench_error(&mut env, pid);
                                        continue;
                                    }
                                };
                                let _ = env.send_and_clear(&pid, |env| {
                                    (atoms::bench_reply_bin(), Binary::from_owned(bin, env))
                                        .encode(env)
                                });
                            }
                            BenchMsg::ZeroMany(pid, size, count) => {
                                let mut failed = false;
                                for _ in 0..count {
                                    let bin = match bench_owned_binary(size, 0) {
                                        Some(bin) => bin,
                                        None => {
                                            send_bench_error(&mut env, pid);
                                            failed = true;
                                            break;
                                        }
                                    };
                                    let _ = env.send_and_clear(&pid, |env| {
                                        (atoms::bench_reply_bin(), Binary::from_owned(bin, env))
                                            .encode(env)
                                    });
                                }
                                if failed {
                                    continue;
                                }
                            }
                            BenchMsg::ZeroBatch(pid, size, count) => {
                                let _ = env.send_and_clear(&pid, |env| {
                                    let mut items = Vec::with_capacity(count);
                                    for id in 0..count {
                                        let bin = match bench_owned_binary(size, 0) {
                                            Some(bin) => bin,
                                            None => {
                                                return (atoms::error(), atoms::error())
                                                    .encode(env);
                                            }
                                        };
                                        items.push(
                                            (id as u64, Binary::from_owned(bin, env)).encode(env),
                                        );
                                    }
                                    (atoms::bench_reply_batch(), items).encode(env)
                                });
                            }
                        }
                    }
                })
                .is_ok();
            if spawn_ok {
                Some(tx)
            } else {
                None
            }
        })
        .as_ref()
}

/// Returns a reference to the global bench queue worker, initializing it if needed.
pub fn bench_queue_worker() -> Option<&'static BenchQueueWorker> {
    BENCH_QUEUE_WORKER
        .get_or_init(|| {
            let (tx, rx) = mpsc::channel::<BenchQueueRequest>();
            let pending = Arc::new(AtomicUsize::new(0));
            let worker = BenchQueueWorker {
                tx,
                next_request_id: AtomicU64::new(1),
                pending: pending.clone(),
            };
            thread::Builder::new()
                .name("bench_queue_worker".into())
                .spawn(move || {
                    let mut env = OwnedEnv::new();
                    while let Ok(req) = rx.recv() {
                        let mut local = VecDeque::new();
                        local.push_back(req);
                        while let Ok(req) = rx.try_recv() {
                            local.push_back(req);
                        }
                        drain_queue(&mut env, &mut local, &pending);
                    }
                })
                .is_ok()
                .then_some(worker)
        })
        .as_ref()
}

fn drain_queue(env: &mut OwnedEnv, local: &mut VecDeque<BenchQueueRequest>, pending: &AtomicUsize) {
    while let Some(req) = local.pop_front() {
        let mut results = Vec::with_capacity(req.ops.len());
        for op in req.ops {
            match op {
                BenchQueueBatchOp::Write { id, data } => {
                    let _ = data.len();
                    results.push((id, QueueResult::Ok));
                }
                BenchQueueBatchOp::Read { id, size } => {
                    results.push((id, QueueResult::BinarySize(size)));
                }
            }
        }
        let _ = env.send_and_clear(&req.reply_pid, |env| {
            let encoded: Vec<_> = results
                .into_iter()
                .map(|(id, result)| match result {
                    QueueResult::Ok => (id, atoms::ok()).encode(env),
                    QueueResult::BinarySize(size) => {
                        let mut bin = NewBinary::new(env, size);
                        for byte in bin.as_mut_slice() {
                            *byte = 0x5a;
                        }
                        let out: Binary<'_> = bin.into();
                        (id, (atoms::ok(), out)).encode(env)
                    }
                })
                .collect();
            (atoms::bench_queue_reply(), req.request_id, encoded).encode(env)
        });
        pending.fetch_sub(1, Ordering::AcqRel);
    }
}

#[derive(Debug)]
enum QueueResult {
    Ok,
    BinarySize(usize),
}
