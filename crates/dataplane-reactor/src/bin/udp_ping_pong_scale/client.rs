use std::collections::HashMap;
use std::net::{SocketAddr, UdpSocket};
use std::os::fd::{AsRawFd, RawFd};
use std::time::Instant;

use dataplane_core_reactor::local_exec::LocalExec;
use dataplane_core_reactor::native_task::{NativeTask, NativeTaskCx, StepResult};
use dataplane_reactor::reactor::adaptive::{ReactorBackend, UnifiedReactor};
use dataplane_reactor::reactor::{NetOp, OpToken, RawNetOp};
use dataplane_runtime::runtime_profiles::{
    dispatch_profiled_runtime_loop_from_profile_with_policy, BalancedRecordingHostPolicy,
    RuntimeLoop, RuntimeLoopHandle, TopologyProfile,
};

use super::config::RunStats;
use super::socket::connect_client_socket;
use super::types::Backend;

type EmbeddedRuntimeHandle = RuntimeLoopHandle<
    dataplane_runtime::runtime_profiles::BalancedRuntime<
        UnifiedReactor,
        IdleTask,
        BalancedRecordingHostPolicy,
        dataplane_core_reactor::balanced_profile::EmbeddedTimerStore,
        dataplane_core_reactor::balanced_profile::EmbeddedParkStore,
    >,
>;

struct IdleTask;
impl NativeTask for IdleTask {
    fn step(&mut self, _cx: &mut NativeTaskCx<Self>) -> StepResult {
        StepResult::Complete
    }
}

pub(crate) fn run_shard(
    client_backend: Backend,
    runtime_profile: TopologyProfile,
    pairs_per_shard: usize,
    rounds_per_pair: usize,
    payload_len: usize,
    uring_send_wait: bool,
    server_addr: SocketAddr,
) -> RunStats {
    let driver = UnifiedReactor::new(
        512,
        match client_backend {
            Backend::Uring => ReactorBackend::IoUring,
            Backend::Syscall => ReactorBackend::Syscall,
        },
    )
    .expect("create unified reactor");
    dispatch_profiled_runtime_loop_from_profile_with_policy::<
        UnifiedReactor,
        IdleTask,
        BalancedRecordingHostPolicy,
        RunStats,
        _,
        _,
        _,
    >(
        runtime_profile,
        driver,
        1,
        BalancedRecordingHostPolicy::default(),
        |runtime| {
            run_shard_with_runtime(
                runtime,
                pairs_per_shard,
                rounds_per_pair,
                payload_len,
                uring_send_wait,
                server_addr,
            )
        },
        |runtime| {
            run_shard_with_embedded_runtime(
                runtime,
                pairs_per_shard,
                rounds_per_pair,
                payload_len,
                uring_send_wait,
                server_addr,
            )
        },
        |runtime| {
            run_shard_with_runtime(
                runtime,
                pairs_per_shard,
                rounds_per_pair,
                payload_len,
                uring_send_wait,
                server_addr,
            )
        },
    )
    .expect("dispatch shard runtime")
}

fn run_shard_with_runtime<R>(
    reactor: RuntimeLoopHandle<R>,
    pairs_per_shard: usize,
    rounds_per_pair: usize,
    payload_len: usize,
    uring_send_wait: bool,
    server_addr: SocketAddr,
) -> RunStats
where
    R: RuntimeLoop<Error = std::io::Error, Submit = RawNetOp, Token = OpToken>,
{
    run_shard_with_runtime_loop(
        reactor,
        pairs_per_shard,
        rounds_per_pair,
        payload_len,
        uring_send_wait,
        server_addr,
    )
}

fn run_shard_with_embedded_runtime(
    reactor: EmbeddedRuntimeHandle,
    pairs_per_shard: usize,
    rounds_per_pair: usize,
    payload_len: usize,
    uring_send_wait: bool,
    server_addr: SocketAddr,
) -> RunStats {
    run_shard_with_runtime_loop(
        reactor,
        pairs_per_shard,
        rounds_per_pair,
        payload_len,
        uring_send_wait,
        server_addr,
    )
}

fn run_shard_with_runtime_loop<R>(
    reactor: RuntimeLoopHandle<R>,
    pairs_per_shard: usize,
    rounds_per_pair: usize,
    payload_len: usize,
    uring_send_wait: bool,
    server_addr: SocketAddr,
) -> RunStats
where
    R: RuntimeLoop<Error = std::io::Error, Submit = RawNetOp, Token = OpToken>,
{
    ShardSession::new(
        reactor,
        pairs_per_shard,
        rounds_per_pair,
        payload_len,
        server_addr,
    )
    .run(pairs_per_shard, uring_send_wait)
}

struct ShardSession<R>
where
    R: RuntimeLoop<Error = std::io::Error, Submit = RawNetOp, Token = OpToken>,
{
    runtime: ClientShardRuntime<R>,
    ready: LocalExec<usize>,
    pairs: Vec<ClientRequestState>,
    completed_pairs: usize,
    started: Instant,
}

impl<R> ShardSession<R>
where
    R: RuntimeLoop<Error = std::io::Error, Submit = RawNetOp, Token = OpToken>,
{
    fn new(
        reactor: RuntimeLoopHandle<R>,
        pairs_per_shard: usize,
        rounds_per_pair: usize,
        payload_len: usize,
        server_addr: SocketAddr,
    ) -> Self {
        let mut ready = LocalExec::new(pairs_per_shard.max(1));
        let mut pairs = Vec::with_capacity(pairs_per_shard);
        for pair_id in 0..pairs_per_shard {
            pairs.push(ClientRequestState::new(
                pair_id,
                server_addr,
                payload_len,
                rounds_per_pair,
            ));
            ready.push(pair_id, pair_id).expect("pair id is a configured ready slot");
        }

        Self {
            runtime: ClientShardRuntime::new(reactor, pairs_per_shard),
            ready,
            pairs,
            completed_pairs: 0,
            started: Instant::now(),
        }
    }

    fn run(mut self, pairs_per_shard: usize, uring_send_wait: bool) -> RunStats {
        while self.completed_pairs < pairs_per_shard {
            let scheduled = self
                .ready
                .drain(pairs_per_shard.max(1), 1, false, |pair_id| {
                    self.runtime
                        .drive_client_request(&mut self.pairs[pair_id], uring_send_wait)
                });
            let step = drive_host_runtime_step(
                &mut self.runtime.reactor,
                self.started,
                1,
                pairs_per_shard.max(1),
                scheduled == 0 && self.runtime.routes.is_empty(),
            );
            for event in step.completions {
                self.runtime.handle_client_completion(
                    event,
                    &mut self.pairs,
                    uring_send_wait,
                    &mut self.ready,
                    &mut self.completed_pairs,
                );
            }
        }

        let mut out = RunStats::default();
        for pair in self.pairs {
            out.datagrams += pair.completed_datagrams;
            out.max_send_inflight = out.max_send_inflight.max(pair.max_send_inflight);
        }
        out
    }
}

struct ClientRequestState {
    pair_id: usize,
    _socket: UdpSocket,
    fd: RawFd,
    tx_buf: Vec<u8>,
    rx_buf: Vec<u8>,
    seq: usize,
    rounds: usize,
    send_inflight: Vec<OpToken>,
    max_send_inflight: usize,
    completed_datagrams: usize,
    phase: ClientRequestPhase,
}

impl ClientRequestState {
    fn new(pair_id: usize, server_addr: SocketAddr, payload_len: usize, rounds: usize) -> Self {
        let socket = connect_client_socket(server_addr, true);
        let fd = socket.as_raw_fd();
        Self {
            pair_id,
            _socket: socket,
            fd,
            tx_buf: vec![0u8; payload_len],
            rx_buf: vec![0u8; payload_len],
            seq: 0,
            rounds,
            send_inflight: Vec::with_capacity(16),
            max_send_inflight: 0,
            completed_datagrams: 0,
            phase: ClientRequestPhase::ReadyToSend,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum ClientRequestPhase {
    ReadyToSend,
    WaitingSend,
    ReadyToRecv,
    WaitingRecv,
    Draining,
    Done,
}

#[derive(Clone, Copy, Debug)]
enum InflightRoute {
    Send { pair_id: usize },
    Recv { pair_id: usize },
}

struct ClientShardRuntime<R>
where
    R: RuntimeLoop<Error = std::io::Error, Submit = RawNetOp, Token = OpToken>,
{
    reactor: RuntimeLoopHandle<R>,
    routes: HashMap<OpToken, InflightRoute>,
}

impl<R> ClientShardRuntime<R>
where
    R: RuntimeLoop<Error = std::io::Error, Submit = RawNetOp, Token = OpToken>,
{
    fn new(reactor: RuntimeLoopHandle<R>, pairs_per_shard: usize) -> Self {
        Self {
            reactor,
            routes: HashMap::with_capacity(pairs_per_shard.saturating_mul(4).max(1)),
        }
    }

    fn drive_client_request(&mut self, pair: &mut ClientRequestState, uring_send_wait: bool) {
        match pair.phase {
            ClientRequestPhase::ReadyToSend => {
                if pair.seq >= pair.rounds {
                    if pair.send_inflight.is_empty() {
                        pair.phase = ClientRequestPhase::Done;
                    } else {
                        pair.phase = ClientRequestPhase::Draining;
                    }
                    return;
                }

                pair.tx_buf[0] = (pair.seq & 0xFF) as u8;
                // SAFETY: pair.tx_buf and its fd stay valid until the send completion.
                let send_token = unsafe { self.submit_client_op(
                    NetOp::UdpSend {
                        fd: pair.fd,
                        ptr: pair.tx_buf.as_ptr(),
                        len: pair.tx_buf.len(),
                    },
                    "submit client send",
                ) };
                assert!(
                    self.routes
                        .insert(
                            send_token,
                            InflightRoute::Send {
                                pair_id: pair.pair_id
                            }
                        )
                        .is_none(),
                    "duplicate send token route"
                );

                if uring_send_wait {
                    pair.phase = ClientRequestPhase::WaitingSend;
                } else {
                    pair.send_inflight.push(send_token);
                    pair.max_send_inflight = pair.max_send_inflight.max(pair.send_inflight.len());
                    // SAFETY: pair.rx_buf and fd stay valid and exclusive until completion.
                    let recv_token = unsafe { self.submit_client_op(
                        NetOp::UdpRecv {
                            fd: pair.fd,
                            ptr: pair.rx_buf.as_mut_ptr(),
                            len: pair.rx_buf.len(),
                        },
                        "submit client recv",
                    ) };
                    assert!(
                        self.routes
                            .insert(
                                recv_token,
                                InflightRoute::Recv {
                                    pair_id: pair.pair_id
                                }
                            )
                            .is_none(),
                        "duplicate recv token route"
                    );
                    pair.phase = ClientRequestPhase::WaitingRecv;
                }
            }
            ClientRequestPhase::ReadyToRecv => {
                // SAFETY: pair.rx_buf and fd stay valid and exclusive until completion.
                let recv_token = unsafe { self.submit_client_op(
                    NetOp::UdpRecv {
                        fd: pair.fd,
                        ptr: pair.rx_buf.as_mut_ptr(),
                        len: pair.rx_buf.len(),
                    },
                    "submit client recv",
                ) };
                assert!(
                    self.routes
                        .insert(
                            recv_token,
                            InflightRoute::Recv {
                                pair_id: pair.pair_id
                            }
                        )
                        .is_none(),
                    "duplicate recv token route"
                );
                pair.phase = ClientRequestPhase::WaitingRecv;
            }
            ClientRequestPhase::WaitingSend
            | ClientRequestPhase::WaitingRecv
            | ClientRequestPhase::Draining
            | ClientRequestPhase::Done => {}
        }
    }

    unsafe fn submit_client_op(&mut self, op: NetOp, context: &str) -> OpToken {
        // SAFETY: each caller retains the operation resources through its completion.
        let op = unsafe { dataplane_reactor::reactor::RawNetOp::new(op) };
        self.reactor
            .submit_and_flush_token(op, dataplane_core_reactor::wake_handle::WakeHandle::None)
            .unwrap_or_else(|err| panic!("{context}: {err}"))
    }

    fn handle_client_completion(
        &mut self,
        event: dataplane_core_reactor::reactor_model::ReactorCompletion,
        pairs: &mut [ClientRequestState],
        uring_send_wait: bool,
        ready: &mut LocalExec<usize>,
        completed_pairs: &mut usize,
    ) {
        let dataplane_core_reactor::reactor_model::ReactorCompletion {
            token,
            kind,
            result,
            ..
        } = event;
        let route = self
            .routes
            .remove(&token)
            .expect("unknown client route token");
        match route {
            InflightRoute::Send { pair_id } => {
                let pair = &mut pairs[pair_id];
                assert_eq!(
                    kind,
                    dataplane_reactor::reactor::NetOpKind::UdpSend,
                    "unexpected client send op kind"
                );
                assert!(result >= 0, "client send failed with errno={}", -result);
                assert_eq!(
                    result as usize,
                    pair.tx_buf.len(),
                    "client sent partial datagram"
                );
                if !uring_send_wait {
                    remove_inflight_token(&mut pair.send_inflight, token);
                }
                if uring_send_wait {
                    pair.phase = ClientRequestPhase::ReadyToRecv;
                    ready.push(pair_id, pair_id).expect("pair id is a configured ready slot");
                } else if matches!(pair.phase, ClientRequestPhase::Draining)
                    && pair.send_inflight.is_empty()
                {
                    Self::finish_request(pair, completed_pairs);
                }
            }
            InflightRoute::Recv { pair_id } => {
                let pair = &mut pairs[pair_id];
                assert_eq!(
                    kind,
                    dataplane_reactor::reactor::NetOpKind::UdpRecv,
                    "unexpected client recv op kind"
                );
                assert!(result >= 0, "client recv failed with errno={}", -result);
                let n = result as usize;
                assert_eq!(n, pair.tx_buf.len(), "client received short datagram");
                assert_eq!(
                    pair.rx_buf[..n],
                    pair.tx_buf[..n],
                    "client payload mismatch"
                );
                pair.completed_datagrams += 2;
                pair.seq += 1;
                if pair.seq < pair.rounds {
                    pair.phase = ClientRequestPhase::ReadyToSend;
                    ready.push(pair_id, pair_id).expect("pair id is a configured ready slot");
                } else if pair.send_inflight.is_empty() {
                    Self::finish_request(pair, completed_pairs);
                } else {
                    pair.phase = ClientRequestPhase::Draining;
                }
            }
        }
    }

    fn finish_request(pair: &mut ClientRequestState, completed_pairs: &mut usize) {
        if !matches!(pair.phase, ClientRequestPhase::Done) {
            pair.phase = ClientRequestPhase::Done;
            *completed_pairs += 1;
        }
    }
}

fn drive_host_runtime_step<R>(
    reactor: &mut RuntimeLoopHandle<R>,
    started: Instant,
    min_events: usize,
    task_budget: usize,
    idle: bool,
) -> HostRuntimeStep
where
    R: RuntimeLoop<Error = std::io::Error, Submit = RawNetOp, Token = OpToken>,
{
    let tick = reactor
        .tick_completions_or_wait(
            started.elapsed().as_nanos() as u64,
            usize::MAX,
            min_events,
            task_budget,
        )
        .expect("tick hosted runtime reactor");
    let out = HostRuntimeStep {
        tasks: tick.tasks,
        completions: tick.completions,
    };
    if idle && out.tasks == 0 {
        std::hint::spin_loop();
    }
    out
}

struct HostRuntimeStep {
    tasks: usize,
    completions: Vec<dataplane_core_reactor::reactor_model::ReactorCompletion>,
}

fn remove_inflight_token(send_inflight: &mut Vec<OpToken>, token: OpToken) {
    if let Some(pos) = send_inflight.iter().position(|t| *t == token) {
        send_inflight.swap_remove(pos);
    }
}
