#[cfg(target_os = "linux")]
use std::collections::HashMap;
#[cfg(target_os = "linux")]
use std::hint::black_box;
#[cfg(target_os = "linux")]
use std::net::UdpSocket;
#[cfg(target_os = "linux")]
use std::os::fd::RawFd;

#[cfg(target_os = "linux")]
use criterion::{criterion_group, criterion_main, BenchmarkId, Criterion, Throughput};
#[cfg(target_os = "linux")]
use dataplane_core_reactor::host_loop::HostLoop;
#[cfg(target_os = "linux")]
use dataplane_core_reactor::local_exec::LocalExec;
#[cfg(target_os = "linux")]
use dataplane_core_reactor::native_task::{NativeTask, NativeTaskCx, NativeTaskEngine, StepResult};
#[cfg(target_os = "linux")]
use dataplane_core_reactor::reactor_model::ReactorCompletion;
#[cfg(target_os = "linux")]
use dataplane_core_reactor::reactor_runtime::ReactorRuntime;
#[cfg(target_os = "linux")]
use dataplane_reactor::reactor::adaptive::{ReactorBackend, UnifiedReactor};
#[cfg(target_os = "linux")]
use dataplane_reactor::reactor::{NetEvent, NetOp, NetOpKind, OpToken, Reactor};

#[cfg(target_os = "linux")]
const BATCHES: &[usize] = &[1, 8, 64, 256];
#[cfg(target_os = "linux")]
const RING_ENTRIES: u32 = 512;
#[cfg(target_os = "linux")]
const UDP_PAYLOAD_LEN: usize = 64;
#[cfg(target_os = "linux")]
const UDP_ROUNDS: &[usize] = &[64, 256];
#[cfg(target_os = "linux")]
const UDP_PAIRS: &[usize] = &[1, 2, 4, 8];

#[cfg(target_os = "linux")]
struct IdleTask;

#[cfg(target_os = "linux")]
impl NativeTask for IdleTask {
    fn step(&mut self, _cx: &mut NativeTaskCx<Self>) -> StepResult {
        StepResult::Complete
    }
}

#[cfg(target_os = "linux")]
type BenchReactorHost = HostLoop<UnifiedReactor, IdleTask>;

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum PairPhase {
    ReadySend,
    ReadyRecv,
    WaitingSend,
    WaitingRecv,
    Done,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ServerPhase {
    ReadyRecv,
    ReadySend,
    WaitingRecv,
    WaitingSend,
    Done,
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug)]
enum PairActor {
    Client(usize),
    Server(usize),
}

#[cfg(target_os = "linux")]
#[derive(Clone, Copy, Debug)]
enum Route {
    ClientSend { pair_id: usize },
    ClientRecv { pair_id: usize },
    ServerRecv { pair_id: usize },
    ServerSend { pair_id: usize },
}

#[cfg(target_os = "linux")]
struct BenchPair {
    _client_socket: UdpSocket,
    _server_socket: UdpSocket,
    client_fd: RawFd,
    server_fd: RawFd,
    client_tx: [u8; UDP_PAYLOAD_LEN],
    client_rx: [u8; UDP_PAYLOAD_LEN],
    server_rx: [u8; UDP_PAYLOAD_LEN],
    seq: usize,
    rounds: usize,
    last_server_recv: usize,
    client_phase: PairPhase,
    server_phase: ServerPhase,
    completed: usize,
}

#[cfg(target_os = "linux")]
impl BenchPair {
    fn new(rounds: usize) -> Self {
        let (client_socket, server_socket) = connected_udp_pair();
        let client_fd = std::os::fd::AsRawFd::as_raw_fd(&client_socket);
        let server_fd = std::os::fd::AsRawFd::as_raw_fd(&server_socket);
        Self {
            _client_socket: client_socket,
            _server_socket: server_socket,
            client_fd,
            server_fd,
            client_tx: [0u8; UDP_PAYLOAD_LEN],
            client_rx: [0u8; UDP_PAYLOAD_LEN],
            server_rx: [0u8; UDP_PAYLOAD_LEN],
            seq: 0,
            rounds,
            last_server_recv: 0,
            client_phase: PairPhase::ReadySend,
            server_phase: ServerPhase::ReadyRecv,
            completed: 0,
        }
    }

    fn done(&self) -> bool {
        matches!(self.client_phase, PairPhase::Done)
            && matches!(self.server_phase, ServerPhase::Done)
    }
}

#[cfg(target_os = "linux")]
fn bench_submit_poll_send_zero(c: &mut Criterion) {
    let mut group = c.benchmark_group("reactor/unified_send_zero");
    for backend in [ReactorBackend::Syscall, ReactorBackend::IoUring] {
        if !backend_supported(backend) {
            continue;
        }
        for batch in BATCHES {
            group.throughput(Throughput::Elements(*batch as u64));
            group.bench_function(
                BenchmarkId::new(format!("{backend:?}"), format!("batch_{batch}")),
                |b| {
                    let (tx_fd, rx_fd) = socket_pair_stream_nonblocking();
                    b.iter(|| {
                        let mut reactor =
                            UnifiedReactor::new(RING_ENTRIES, backend).expect("create reactor");
                        for _ in 0..*batch {
                            let token = reactor
                                .submit(NetOp::Send {
                                    fd: tx_fd,
                                    ptr: std::ptr::null(),
                                    len: 0,
                                })
                                .expect("submit send");
                            black_box(token);
                        }
                        let submitted = reactor.submit_pending().expect("submit pending");
                        black_box(submitted);
                        let mut done = 0usize;
                        while done < *batch {
                            let events = reactor.poll(true).expect("poll");
                            for event in events {
                                let NetEvent::OpComplete { result, .. } = event;
                                black_box(result);
                                done += 1;
                            }
                        }
                        black_box(done);
                    });
                    close_fd(tx_fd);
                    close_fd(rx_fd);
                },
            );
        }
    }
    group.finish();
}

#[cfg(target_os = "linux")]
fn bench_udp_ping_pong_async(c: &mut Criterion) {
    let mut group = c.benchmark_group("reactor/unified_udp_ping_pong_async");
    for backend in [ReactorBackend::Syscall, ReactorBackend::IoUring] {
        if !backend_supported(backend) {
            continue;
        }
        for pairs in UDP_PAIRS {
            for rounds in UDP_ROUNDS {
                let total_msgs = (*pairs as u64) * (*rounds as u64) * 2;
                group.throughput(Throughput::Elements(total_msgs));
                group.bench_function(
                    BenchmarkId::new(format!("{backend:?}"), format!("p{pairs}_r{rounds}")),
                    |b| {
                        b.iter(|| {
                            let completed = run_udp_pairs(*rounds, *pairs, backend);
                            black_box(completed);
                        });
                    },
                );
            }
        }
    }
    group.finish();
}

#[cfg(target_os = "linux")]
fn backend_supported(backend: ReactorBackend) -> bool {
    match backend {
        ReactorBackend::IoUring => UnifiedReactor::new(RING_ENTRIES, backend).is_ok(),
        ReactorBackend::Syscall => true,
        ReactorBackend::Auto => true,
    }
}

#[cfg(target_os = "linux")]
fn run_udp_pairs(rounds: usize, pair_count: usize, backend: ReactorBackend) -> usize {
    let driver = UnifiedReactor::new(RING_ENTRIES, backend).expect("create pair reactor");
    let mut reactor = BenchReactorHost::new(
        ReactorRuntime::new(driver),
        NativeTaskEngine::with_task_capacity(1),
    );
    let mut ready = LocalExec::new(pair_count.saturating_mul(2).max(1));
    let mut routes = HashMap::with_capacity(pair_count.saturating_mul(4).max(1));
    let mut pairs = Vec::with_capacity(pair_count);
    for pair_id in 0..pair_count {
        pairs.push(BenchPair::new(rounds));
        ready.push(pair_id * 2, PairActor::Client(pair_id)).expect("push");
        ready.push(pair_id * 2 + 1, PairActor::Server(pair_id)).expect("push");
    }

    let mut completed_pairs = 0usize;
    while completed_pairs < pair_count {
        let mut progressed = 0usize;
        progressed += ready.drain(pair_count.saturating_mul(2).max(1), 1, false, |actor| {
            drive_pair_actor(&mut reactor, &mut pairs, actor, &mut routes);
        });

        let mut events = reactor.poll(false, usize::MAX).expect("poll pair reactor");
        if events.is_empty() && progressed == 0 && !routes.is_empty() {
            events = reactor.poll(true, usize::MAX).expect("wait pair reactor");
        }

        for event in events {
            progressed += 1;
            handle_pair_completion(
                event,
                &mut pairs,
                &mut routes,
                &mut ready,
                &mut completed_pairs,
            );
        }

        if progressed == 0 {
            std::hint::spin_loop();
        }
    }

    pairs.into_iter().map(|pair| pair.completed).sum()
}

#[cfg(target_os = "linux")]
fn drive_pair_actor(
    reactor: &mut BenchReactorHost,
    pairs: &mut [BenchPair],
    actor: PairActor,
    routes: &mut HashMap<OpToken, Route>,
) {
    match actor {
        PairActor::Client(pair_id) => {
            drive_client_actor(reactor, pair_id, &mut pairs[pair_id], routes)
        }
        PairActor::Server(pair_id) => {
            drive_server_actor(reactor, pair_id, &mut pairs[pair_id], routes)
        }
    }
}

#[cfg(target_os = "linux")]
fn drive_client_actor(
    reactor: &mut BenchReactorHost,
    pair_id: usize,
    pair: &mut BenchPair,
    routes: &mut HashMap<OpToken, Route>,
) {
    match pair.client_phase {
        PairPhase::ReadySend => {
            if pair.seq >= pair.rounds {
                pair.client_phase = PairPhase::Done;
                return;
            }
            pair.client_tx[0] = (pair.seq & 0xFF) as u8;
            let token = submit_pair_op(
                reactor,
                NetOp::UdpSend {
                    fd: pair.client_fd,
                    ptr: pair.client_tx.as_ptr(),
                    len: pair.client_tx.len(),
                },
                "submit pair client send",
            );
            assert!(routes
                .insert(token, Route::ClientSend { pair_id })
                .is_none());
            pair.client_phase = PairPhase::WaitingSend;
        }
        PairPhase::ReadyRecv => {
            let token = submit_pair_op(
                reactor,
                NetOp::UdpRecv {
                    fd: pair.client_fd,
                    ptr: pair.client_rx.as_mut_ptr(),
                    len: pair.client_rx.len(),
                },
                "submit pair client recv",
            );
            assert!(routes
                .insert(token, Route::ClientRecv { pair_id })
                .is_none());
            pair.client_phase = PairPhase::WaitingRecv;
        }
        PairPhase::WaitingSend | PairPhase::WaitingRecv | PairPhase::Done => {}
    }
}

#[cfg(target_os = "linux")]
fn drive_server_actor(
    reactor: &mut BenchReactorHost,
    pair_id: usize,
    pair: &mut BenchPair,
    routes: &mut HashMap<OpToken, Route>,
) {
    match pair.server_phase {
        ServerPhase::ReadyRecv => {
            let token = submit_pair_op(
                reactor,
                NetOp::UdpRecv {
                    fd: pair.server_fd,
                    ptr: pair.server_rx.as_mut_ptr(),
                    len: pair.server_rx.len(),
                },
                "submit pair server recv",
            );
            assert!(routes
                .insert(token, Route::ServerRecv { pair_id })
                .is_none());
            pair.server_phase = ServerPhase::WaitingRecv;
        }
        ServerPhase::ReadySend => {
            let token = submit_pair_op(
                reactor,
                NetOp::UdpSend {
                    fd: pair.server_fd,
                    ptr: pair.server_rx.as_ptr(),
                    len: pair.last_server_recv,
                },
                "submit pair server send",
            );
            assert!(routes
                .insert(token, Route::ServerSend { pair_id })
                .is_none());
            pair.server_phase = ServerPhase::WaitingSend;
        }
        ServerPhase::WaitingRecv | ServerPhase::WaitingSend | ServerPhase::Done => {}
    }
}

#[cfg(target_os = "linux")]
fn handle_pair_completion(
    completion: ReactorCompletion,
    pairs: &mut [BenchPair],
    routes: &mut HashMap<OpToken, Route>,
    ready: &mut LocalExec<PairActor>,
    completed_pairs: &mut usize,
) {
    let ReactorCompletion {
        token,
        kind,
        result,
        ..
    } = completion;
    let route = routes.remove(&token).expect("unknown pair route token");
    assert!(result >= 0, "pair op failed with errno={}", -result);
    match route {
        Route::ClientSend { pair_id } => {
            let pair = &mut pairs[pair_id];
            assert_eq!(kind, NetOpKind::UdpSend);
            assert_eq!(result as usize, pair.client_tx.len());
            pair.client_phase = PairPhase::ReadyRecv;
            ready.push(pair_id * 2, PairActor::Client(pair_id)).expect("push");
        }
        Route::ClientRecv { pair_id } => {
            let pair = &mut pairs[pair_id];
            assert_eq!(kind, NetOpKind::UdpRecv);
            let n = result as usize;
            assert_eq!(n, pair.client_tx.len());
            assert_eq!(pair.client_rx[..n], pair.client_tx[..n]);
            pair.completed += 2;
            pair.seq += 1;
            if pair.seq < pair.rounds {
                pair.client_phase = PairPhase::ReadySend;
                ready.push(pair_id * 2, PairActor::Client(pair_id)).expect("push");
            } else {
                pair.client_phase = PairPhase::Done;
                if pair.done() {
                    *completed_pairs += 1;
                }
            }
        }
        Route::ServerRecv { pair_id } => {
            let pair = &mut pairs[pair_id];
            assert_eq!(kind, NetOpKind::UdpRecv);
            pair.last_server_recv = result as usize;
            pair.server_phase = ServerPhase::ReadySend;
            ready.push(pair_id * 2 + 1, PairActor::Server(pair_id)).expect("push");
        }
        Route::ServerSend { pair_id } => {
            let pair = &mut pairs[pair_id];
            assert_eq!(kind, NetOpKind::UdpSend);
            assert_eq!(result as usize, pair.last_server_recv);
            pair.completed += 2;
            if pair.seq >= pair.rounds {
                pair.server_phase = ServerPhase::Done;
                if pair.done() {
                    *completed_pairs += 1;
                }
            } else {
                pair.server_phase = ServerPhase::ReadyRecv;
                ready.push(pair_id * 2 + 1, PairActor::Server(pair_id)).expect("push");
            }
        }
    }
}

#[cfg(target_os = "linux")]
fn submit_pair_op(reactor: &mut BenchReactorHost, op: NetOp, context: &str) -> OpToken {
    reactor
        .submit_and_flush(op, dataplane_core_reactor::wake_handle::WakeHandle::None)
        .unwrap_or_else(|err| panic!("{context}: {err}"))
        .token()
}

#[cfg(target_os = "linux")]
fn connected_udp_pair() -> (UdpSocket, UdpSocket) {
    let a = UdpSocket::bind("127.0.0.1:0").expect("bind udp a");
    let b = UdpSocket::bind("127.0.0.1:0").expect("bind udp b");
    a.set_nonblocking(true).expect("set nonblocking a");
    b.set_nonblocking(true).expect("set nonblocking b");
    let a_addr = a.local_addr().expect("a local addr");
    let b_addr = b.local_addr().expect("b local addr");
    a.connect(b_addr).expect("connect a->b");
    b.connect(a_addr).expect("connect b->a");
    (a, b)
}

#[cfg(target_os = "linux")]
fn socket_pair_stream_nonblocking() -> (RawFd, RawFd) {
    let mut fds = [0; 2];
    let rc = unsafe {
        libc::socketpair(
            libc::AF_UNIX,
            libc::SOCK_STREAM | libc::SOCK_NONBLOCK | libc::SOCK_CLOEXEC,
            0,
            fds.as_mut_ptr(),
        )
    };
    assert_eq!(
        rc,
        0,
        "socketpair failed: {}",
        std::io::Error::last_os_error()
    );
    (fds[0], fds[1])
}

#[cfg(target_os = "linux")]
fn close_fd(fd: RawFd) {
    let rc = unsafe { libc::close(fd) };
    assert_eq!(rc, 0, "close failed: {}", std::io::Error::last_os_error());
}

#[cfg(all(test, target_os = "linux"))]
#[allow(unused_imports)]
mod tests {
    use super::{
        handle_pair_completion, BenchPair, LocalExec, NetOpKind, OpToken, PairActor, PairPhase,
        ReactorCompletion, Route, ServerPhase, UDP_PAYLOAD_LEN,
    };
    use std::collections::HashMap;

    #[test]
    fn bench_pair_new_starts_in_ready_send_ready_recv() {
        let pair = BenchPair::new(1);
        assert_eq!(pair.client_phase, PairPhase::ReadySend);
        assert_eq!(pair.server_phase, ServerPhase::ReadyRecv);
        assert!(!pair.done());
    }

    #[test]
    fn client_send_completion_moves_client_to_ready_recv() {
        let mut pairs = vec![BenchPair::new(1)];
        let token = OpToken(41);
        let mut routes = HashMap::from([(token, Route::ClientSend { pair_id: 0 })]);
        let mut ready = LocalExec::new(2);
        let mut completed_pairs = 0usize;

        handle_pair_completion(
            ReactorCompletion {
                token,
                kind: NetOpKind::UdpSend,
                result: UDP_PAYLOAD_LEN as i32,
                flags: 0,
            },
            &mut pairs,
            &mut routes,
            &mut ready,
            &mut completed_pairs,
        );

        assert_eq!(pairs[0].client_phase, PairPhase::ReadyRecv);
        assert_eq!(pairs[0].server_phase, ServerPhase::ReadyRecv);
        assert!(routes.is_empty());
        assert_eq!(ready.pending(), 1);
        assert_eq!(completed_pairs, 0);
    }

    #[test]
    fn server_recv_completion_moves_server_to_ready_send_and_records_length() {
        let mut pairs = vec![BenchPair::new(1)];
        let token = OpToken(42);
        let mut routes = HashMap::from([(token, Route::ServerRecv { pair_id: 0 })]);
        let mut ready = LocalExec::new(2);
        let mut completed_pairs = 0usize;

        handle_pair_completion(
            ReactorCompletion {
                token,
                kind: NetOpKind::UdpRecv,
                result: 17,
                flags: 0,
            },
            &mut pairs,
            &mut routes,
            &mut ready,
            &mut completed_pairs,
        );

        assert_eq!(pairs[0].server_phase, ServerPhase::ReadySend);
        assert_eq!(pairs[0].last_server_recv, 17);
        assert_eq!(pairs[0].completed, 0);
        assert!(routes.is_empty());
        assert_eq!(ready.pending(), 1);
        assert_eq!(completed_pairs, 0);
    }

    #[test]
    fn client_recv_nonfinal_round_rearms_client_for_next_send() {
        let mut pair = BenchPair::new(2);
        pair.client_phase = PairPhase::WaitingRecv;
        pair.server_phase = ServerPhase::ReadyRecv;
        pair.client_tx[0] = 9;
        pair.client_rx.copy_from_slice(&pair.client_tx);
        let mut pairs = vec![pair];
        let token = OpToken(43);
        let mut routes = HashMap::from([(token, Route::ClientRecv { pair_id: 0 })]);
        let mut ready = LocalExec::new(2);
        let mut completed_pairs = 0usize;

        handle_pair_completion(
            ReactorCompletion {
                token,
                kind: NetOpKind::UdpRecv,
                result: UDP_PAYLOAD_LEN as i32,
                flags: 0,
            },
            &mut pairs,
            &mut routes,
            &mut ready,
            &mut completed_pairs,
        );

        assert_eq!(pairs[0].seq, 1);
        assert_eq!(pairs[0].completed, 2);
        assert_eq!(pairs[0].client_phase, PairPhase::ReadySend);
        assert_eq!(completed_pairs, 0);
        assert_eq!(ready.pending(), 1);
    }

    #[test]
    fn server_send_final_round_marks_pair_done_only_when_client_is_done() {
        let mut pair = BenchPair::new(1);
        pair.seq = pair.rounds;
        pair.client_phase = PairPhase::Done;
        pair.server_phase = ServerPhase::WaitingSend;
        pair.last_server_recv = UDP_PAYLOAD_LEN;
        let mut pairs = vec![pair];
        let token = OpToken(44);
        let mut routes = HashMap::from([(token, Route::ServerSend { pair_id: 0 })]);
        let mut ready = LocalExec::new(2);
        let mut completed_pairs = 0usize;

        handle_pair_completion(
            ReactorCompletion {
                token,
                kind: NetOpKind::UdpSend,
                result: UDP_PAYLOAD_LEN as i32,
                flags: 0,
            },
            &mut pairs,
            &mut routes,
            &mut ready,
            &mut completed_pairs,
        );

        assert_eq!(pairs[0].server_phase, ServerPhase::Done);
        assert_eq!(pairs[0].completed, 2);
        assert_eq!(completed_pairs, 1);
        assert_eq!(ready.pending(), 0);
    }
}

#[cfg(target_os = "linux")]
criterion_group!(
    uring_benches,
    bench_submit_poll_send_zero,
    bench_udp_ping_pong_async
);
#[cfg(target_os = "linux")]
criterion_main!(uring_benches);

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("uring_reactor bench is linux-only");
}
