use std::net::{SocketAddr, UdpSocket};
use std::os::fd::{AsRawFd, RawFd};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use dataplane_core_reactor::balanced_profile::{EmbeddedParkStore, EmbeddedTimerStore};
use dataplane_core_reactor::native_task::{NativeTask, NativeTaskCx, StepResult};
use dataplane_core_reactor::reactor_model::ReactorCompletion;
use dataplane_reactor::reactor::adaptive::{ReactorBackend, UnifiedReactor};
use dataplane_reactor::reactor::{NetOp, NetOpKind, OpToken, RawNetOp, UdpRecvSlot};
use dataplane_runtime::runtime_profiles::{
    dispatch_profiled_runtime_loop_from_profile_with_policy, BalancedRecordingHostPolicy,
    BalancedRuntime, RuntimeLoop, RuntimeLoopHandle, TopologyProfile,
};

use super::client::run_shard;
use super::config::{RunBackendConfig, RunStats};
use super::socket::{
    pin_current_thread, prepare_shared_server, sendto_nb_retry, spawn_host_thread,
};
use super::types::{Backend, ServerRecvMode};

type EmbeddedRuntimeHandle = RuntimeLoopHandle<
    BalancedRuntime<
        UnifiedReactor,
        IdleTask,
        BalancedRecordingHostPolicy,
        EmbeddedTimerStore,
        EmbeddedParkStore,
    >,
>;

struct IdleTask;
impl NativeTask for IdleTask {
    fn step(&mut self, _cx: &mut NativeTaskCx<Self>) -> StepResult {
        StepResult::Complete
    }
}

pub(crate) fn run_backend(config: RunBackendConfig) -> RunStats {
    run_backend_hosted(&config)
}

fn run_backend_hosted(config: &RunBackendConfig) -> RunStats {
    let RunBackendConfig {
        client_backend,
        runtime_profile,
        shards,
        pairs_per_shard,
        rounds_per_pair,
        payload_len,
        uring_send_wait,
        server_sockets,
        server_recv_mode,
        server_batch,
        local_client_shards,
        local_server_workers,
        pin_threads,
    } = config.clone();
    let local_client_shards = local_client_shards.clamp(1, shards);
    let local_server_workers = local_server_workers.clamp(1, server_sockets.max(1));
    let server_requests = shards * pairs_per_shard * rounds_per_pair;
    let (server_addr, server_sockets) = prepare_shared_server(server_sockets);
    if server_sockets.len() > 1 {
        return run_backend_hosted_with_spawned_server(SpawnedServerRun {
            client_backend,
            runtime_profile,
            shards,
            pairs_per_shard,
            rounds_per_pair,
            payload_len,
            uring_send_wait,
            server_sockets,
            server_addr,
            server_requests,
            server_recv_mode,
            server_batch,
            local_client_shards,
            local_server_workers,
            pin_threads,
        });
    }
    let server_runtime_profile = runtime_profile.clone();
    let server_processed = Arc::new(AtomicUsize::new(0));
    let start_barrier = Arc::new(std::sync::Barrier::new(shards + 1));
    let mut joins = Vec::with_capacity(shards);
    for shard in 0..shards {
        let barrier = Arc::clone(&start_barrier);
        let runtime_profile = runtime_profile.clone();
        joins.push(spawn_host_thread(
            format!("udp-scale-{client_backend:?}-{shard}"),
            pin_threads.then_some(shard),
            move || {
                barrier.wait();
                run_shard(
                    client_backend,
                    runtime_profile,
                    pairs_per_shard,
                    rounds_per_pair,
                    payload_len,
                    uring_send_wait,
                    server_addr,
                )
            },
        ));
    }
    if let Some(cpu) = pin_threads.then_some(0) {
        pin_current_thread(cpu);
    }
    start_barrier.wait();

    let mut out = RunStats::default();
    let echoed = run_shared_server_fanout(SharedServerFanout {
        sockets: server_sockets,
        processed: Arc::clone(&server_processed),
        expected_requests: server_requests,
        payload_len,
        recv_mode: server_recv_mode,
        recv_batch: server_batch,
        runtime_profile: server_runtime_profile,
        local_server_workers,
        pin_threads,
    });

    for join in joins {
        let stats = join.join().expect("join shard");
        out.datagrams += stats.datagrams;
        out.max_send_inflight = out.max_send_inflight.max(stats.max_send_inflight);
    }
    assert_eq!(
        echoed, server_requests,
        "shared server processed unexpected request count"
    );
    out
}

struct SharedServerFanout {
    sockets: Vec<UdpSocket>,
    processed: Arc<AtomicUsize>,
    expected_requests: usize,
    payload_len: usize,
    recv_mode: ServerRecvMode,
    recv_batch: usize,
    runtime_profile: TopologyProfile,
    local_server_workers: usize,
    pin_threads: bool,
}

struct SpawnedServerRun {
    client_backend: Backend,
    runtime_profile: TopologyProfile,
    shards: usize,
    pairs_per_shard: usize,
    rounds_per_pair: usize,
    payload_len: usize,
    uring_send_wait: bool,
    server_sockets: Vec<UdpSocket>,
    server_addr: SocketAddr,
    server_requests: usize,
    server_recv_mode: ServerRecvMode,
    server_batch: usize,
    local_client_shards: usize,
    local_server_workers: usize,
    pin_threads: bool,
}

fn run_backend_hosted_with_spawned_server(run: SpawnedServerRun) -> RunStats {
    let SpawnedServerRun {
        client_backend,
        runtime_profile,
        shards,
        pairs_per_shard,
        rounds_per_pair,
        payload_len,
        uring_send_wait,
        server_sockets,
        server_addr,
        server_requests,
        server_recv_mode,
        server_batch,
        local_client_shards,
        local_server_workers,
        pin_threads,
    } = run;

    let server_runtime_profile = runtime_profile.clone();
    let server_join = spawn_host_thread(
        format!("udp-scale-shared-server-{server_recv_mode:?}"),
        pin_threads.then_some(0),
        move || {
            run_shared_server_fanout(SharedServerFanout {
                sockets: server_sockets,
                processed: Arc::new(AtomicUsize::new(0)),
                expected_requests: server_requests,
                payload_len,
                recv_mode: server_recv_mode,
                recv_batch: server_batch,
                runtime_profile: server_runtime_profile,
                local_server_workers,
                pin_threads,
            })
        },
    );
    let start_barrier = Arc::new(std::sync::Barrier::new(
        shards.saturating_sub(local_client_shards) + 1,
    ));
    let mut joins = Vec::with_capacity(shards.saturating_sub(local_client_shards));
    for shard in local_client_shards..shards {
        let barrier = Arc::clone(&start_barrier);
        let runtime_profile = runtime_profile.clone();
        joins.push(spawn_host_thread(
            format!("udp-scale-{client_backend:?}-{shard}"),
            pin_threads.then_some(shard),
            move || {
                barrier.wait();
                run_shard(
                    client_backend,
                    runtime_profile,
                    pairs_per_shard,
                    rounds_per_pair,
                    payload_len,
                    uring_send_wait,
                    server_addr,
                )
            },
        ));
    }
    if let Some(cpu) = pin_threads.then_some(0) {
        pin_current_thread(cpu);
    }
    start_barrier.wait();

    let mut out = RunStats::default();
    for shard in 0..local_client_shards {
        let stats = run_shard(
            client_backend,
            runtime_profile.clone(),
            pairs_per_shard,
            rounds_per_pair,
            payload_len,
            uring_send_wait,
            server_addr,
        );
        if shard == 0 {
            out = stats;
        } else {
            out.datagrams += stats.datagrams;
            out.max_send_inflight = out.max_send_inflight.max(stats.max_send_inflight);
        }
    }

    for join in joins {
        let stats = join.join().expect("join shard");
        out.datagrams += stats.datagrams;
        out.max_send_inflight = out.max_send_inflight.max(stats.max_send_inflight);
    }
    let echoed = server_join.join().expect("join shared server");
    assert_eq!(
        echoed, server_requests,
        "shared server processed unexpected request count"
    );
    out
}

fn run_shared_server_fanout(fanout: SharedServerFanout) -> usize {
    let SharedServerFanout {
        sockets,
        processed,
        expected_requests,
        payload_len,
        recv_mode,
        recv_batch,
        runtime_profile,
        local_server_workers,
        pin_threads,
    } = fanout;

    if sockets.len() > 1 {
        let mut joins = Vec::with_capacity(sockets.len());
        for (idx, socket) in sockets.into_iter().enumerate() {
            let processed = Arc::clone(&processed);
            let runtime_profile = runtime_profile.clone();
            joins.push(spawn_host_thread(
                format!("udp-scale-shared-server-worker-{idx}"),
                pin_threads.then_some(idx),
                move || {
                    run_shared_server_reactor(
                        socket,
                        processed,
                        expected_requests,
                        payload_len,
                        recv_mode,
                        recv_batch,
                        runtime_profile,
                    )
                },
            ));
        }
        for join in joins {
            let _ = join.join().expect("join server worker");
        }
        return processed.load(Ordering::Relaxed);
    }

    let mut sockets = sockets.into_iter();
    let local_server_workers = local_server_workers.max(1);
    let mut local_sockets = Vec::with_capacity(local_server_workers);
    for _ in 0..local_server_workers {
        let Some(socket) = sockets.next() else {
            break;
        };
        local_sockets.push(socket);
    }
    if local_sockets.is_empty() {
        return processed.load(Ordering::Relaxed);
    }

    let mut joins = Vec::new();
    for (idx, socket) in sockets.enumerate() {
        let worker_idx = idx + local_sockets.len();
        let processed = Arc::clone(&processed);
        let runtime_profile = runtime_profile.clone();
        joins.push(spawn_host_thread(
            format!("udp-scale-shared-server-worker-{worker_idx}"),
            pin_threads.then_some(worker_idx),
            move || {
                run_shared_server_reactor(
                    socket,
                    processed,
                    expected_requests,
                    payload_len,
                    recv_mode,
                    recv_batch,
                    runtime_profile,
                )
            },
        ));
    }

    for socket in local_sockets {
        run_shared_server_reactor(
            socket,
            Arc::clone(&processed),
            expected_requests,
            payload_len,
            recv_mode,
            recv_batch,
            runtime_profile.clone(),
        );
    }

    for join in joins {
        let _ = join.join().expect("join server worker");
    }
    processed.load(Ordering::Relaxed)
}

fn run_shared_server_reactor(
    server: UdpSocket,
    processed: Arc<AtomicUsize>,
    expected_requests: usize,
    payload_len: usize,
    recv_mode: ServerRecvMode,
    recv_batch: usize,
    runtime_profile: TopologyProfile,
) -> usize {
    let fd = server.as_raw_fd();
    let batch = match recv_mode {
        ServerRecvMode::From => 1,
        ServerRecvMode::Mmsg | ServerRecvMode::MsgMultishot => recv_batch.clamp(1, 128),
    };
    let preferred_backend = match recv_mode {
        ServerRecvMode::From | ServerRecvMode::Mmsg => ReactorBackend::Syscall,
        ServerRecvMode::MsgMultishot => ReactorBackend::IoUring,
    };
    let ring_entries = (batch.max(64).next_power_of_two().min(2048)) as u32;
    let driver = match UnifiedReactor::new(ring_entries, preferred_backend) {
        Ok(r) => r,
        Err(_e) => UnifiedReactor::new(ring_entries, ReactorBackend::Syscall)
            .expect("create fallback syscall reactor"),
    };

    dispatch_profiled_runtime_loop_from_profile_with_policy::<
        UnifiedReactor,
        IdleTask,
        BalancedRecordingHostPolicy,
        usize,
        _,
        _,
        _,
    >(
        runtime_profile,
        driver,
        1,
        BalancedRecordingHostPolicy::default(),
        |runtime| {
            run_shared_server_reactor_with_runtime(
                runtime,
                fd,
                payload_len,
                recv_mode,
                batch,
                Arc::clone(&processed),
                expected_requests,
            )
        },
        |runtime| {
            run_shared_server_reactor_with_embedded_runtime(
                runtime,
                fd,
                payload_len,
                recv_mode,
                batch,
                Arc::clone(&processed),
                expected_requests,
            )
        },
        |runtime| {
            run_shared_server_reactor_with_runtime(
                runtime,
                fd,
                payload_len,
                recv_mode,
                batch,
                Arc::clone(&processed),
                expected_requests,
            )
        },
    )
    .expect("dispatch server runtime")
}

fn run_shared_server_reactor_with_runtime<R>(
    reactor: RuntimeLoopHandle<R>,
    fd: RawFd,
    payload_len: usize,
    recv_mode: ServerRecvMode,
    batch: usize,
    processed: Arc<AtomicUsize>,
    expected_requests: usize,
) -> usize
where
    R: RuntimeLoop<Error = std::io::Error, Submit = RawNetOp, Token = OpToken>,
{
    run_shared_server_reactor_with_runtime_loop(
        reactor,
        fd,
        payload_len,
        recv_mode,
        batch,
        processed,
        expected_requests,
    )
}

fn run_shared_server_reactor_with_embedded_runtime(
    reactor: EmbeddedRuntimeHandle,
    fd: RawFd,
    payload_len: usize,
    recv_mode: ServerRecvMode,
    batch: usize,
    processed: Arc<AtomicUsize>,
    expected_requests: usize,
) -> usize {
    run_shared_server_reactor_with_runtime_loop(
        reactor,
        fd,
        payload_len,
        recv_mode,
        batch,
        processed,
        expected_requests,
    )
}

fn run_shared_server_reactor_with_runtime_loop<R>(
    reactor: RuntimeLoopHandle<R>,
    fd: RawFd,
    payload_len: usize,
    recv_mode: ServerRecvMode,
    batch: usize,
    processed: Arc<AtomicUsize>,
    expected_requests: usize,
) -> usize
where
    R: RuntimeLoop<Error = std::io::Error, Submit = RawNetOp, Token = OpToken>,
{
    let owner = ServerIngressOwner::new(
        reactor,
        fd,
        payload_len,
        recv_mode,
        batch,
        processed,
        expected_requests,
    );
    let mut pump = ServerPump::new(owner);

    while !pump.done() {
        pump.step();
    }

    pump.processed()
}

struct ServerIngressOwner<R>
where
    R: RuntimeLoop<Error = std::io::Error, Submit = RawNetOp, Token = OpToken>,
{
    reactor: RuntimeLoopHandle<R>,
    fd: RawFd,
    bufs: Vec<Vec<u8>>,
    slots: Vec<UdpRecvSlot>,
    recv_mode: ServerRecvMode,
    batch_token: Option<OpToken>,
    processed: Arc<AtomicUsize>,
    expected_requests: usize,
}

impl<R> ServerIngressOwner<R>
where
    R: RuntimeLoop<Error = std::io::Error, Submit = RawNetOp, Token = OpToken>,
{
    fn new(
        reactor: RuntimeLoopHandle<R>,
        fd: RawFd,
        payload_len: usize,
        recv_mode: ServerRecvMode,
        batch: usize,
        processed: Arc<AtomicUsize>,
        expected_requests: usize,
    ) -> Self {
        let mut bufs = vec![vec![0u8; payload_len.max(1)]; batch];
        let slots = (0..batch)
            .map(|idx| UdpRecvSlot {
                buf_ptr: bufs[idx].as_mut_ptr(),
                buf_len: bufs[idx].len(),
                recv_len: 0,
                addr: unsafe { std::mem::zeroed::<libc::sockaddr_storage>() },
                addr_len: 0,
            })
            .collect::<Vec<_>>();
        Self {
            reactor,
            fd,
            bufs,
            slots,
            recv_mode,
            batch_token: None,
            processed,
            expected_requests,
        }
    }

    fn done(&self) -> bool {
        self.processed.load(Ordering::Acquire) >= self.expected_requests
    }

    fn advance(&mut self) -> usize {
        if self.done() {
            return 0;
        }

        let submitted = self
            .reactor
            .submit_if_idle(
                &mut self.batch_token,
                // SAFETY: the server owns the fd, slots, and backing buffers until completion.
                unsafe { RawNetOp::new(NetOp::UdpRecvBatch {
                    fd: self.fd,
                    slots_ptr: self.slots.as_mut_ptr(),
                    slots_len: self.slots.len(),
                    flags: libc::MSG_DONTWAIT,
                    prefer_multishot: matches!(self.recv_mode, ServerRecvMode::MsgMultishot),
                }) },
                dataplane_core_reactor::wake_handle::WakeHandle::None,
            )
            .expect("submit udp recv batch");
        usize::from(submitted)
    }

    fn handle_completion(&mut self, event: ReactorCompletion) {
        let ReactorCompletion {
            token,
            kind,
            result,
            ..
        } = event;
        if Some(token) != self.batch_token {
            return;
        }
        assert_eq!(kind, NetOpKind::UdpRecvBatch, "unexpected server op kind");
        assert!(result >= 0, "udp recv batch failed with errno={}", -result);
        let ready = result as usize;
        self.batch_token = None;
        for idx in 0..ready {
            let len = self.slots[idx].recv_len;
            if len == 0 {
                continue;
            }
            sendto_nb_retry(
                self.fd,
                self.bufs[idx].as_ptr(),
                len,
                (&self.slots[idx].addr as *const libc::sockaddr_storage).cast(),
                self.slots[idx].addr_len,
            );
            self.processed.fetch_add(1, Ordering::AcqRel);
            if self.done() {
                break;
            }
        }
    }
}

struct ServerPump<R>
where
    R: RuntimeLoop<Error = std::io::Error, Submit = RawNetOp, Token = OpToken>,
{
    owner: ServerIngressOwner<R>,
}

impl<R> ServerPump<R>
where
    R: RuntimeLoop<Error = std::io::Error, Submit = RawNetOp, Token = OpToken>,
{
    fn new(owner: ServerIngressOwner<R>) -> Self {
        Self { owner }
    }

    fn done(&self) -> bool {
        self.owner.done()
    }

    fn step(&mut self) {
        let submitted = self.owner.advance();
        let completions = self
            .owner
            .reactor
            .poll(false, usize::MAX)
            .expect("poll server reactor");
        let idle = submitted == 0 && self.owner.batch_token.is_none();
        if idle && completions.is_empty() {
            std::hint::spin_loop();
        }
        for event in completions {
            self.owner.handle_completion(event);
        }
    }

    fn processed(&self) -> usize {
        self.owner.processed.load(Ordering::Relaxed)
    }
}
