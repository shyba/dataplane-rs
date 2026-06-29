use std::net::{SocketAddr, UdpSocket};
use std::sync::mpsc::{self, TryRecvError};
use std::thread;
use std::time::Instant;

use dataplane_runtime::runtime_topology::{pin_current_to_cpu, TopologyProfile};

// Keep local ownership defaults explicit.
const DEFAULT_SEQ_LOCAL_CLIENT_SHARDS: usize = 1;
const DEFAULT_SEQ_LOCAL_WORKER_SHARDS: usize = 3;

#[derive(Clone, Copy, Debug, Default)]
struct Counter {
    sent: u64,
    recv: u64,
}

#[derive(Clone, Copy, Debug)]
struct WireMsg {
    seq: u64,
    client_shard: u32,
    route_key: u32,
}

impl WireMsg {
    const LEN: usize = 16;

    fn encode(self) -> [u8; Self::LEN] {
        let mut out = [0u8; Self::LEN];
        out[0..8].copy_from_slice(&self.seq.to_le_bytes());
        out[8..12].copy_from_slice(&self.client_shard.to_le_bytes());
        out[12..16].copy_from_slice(&self.route_key.to_le_bytes());
        out
    }

    fn decode(buf: &[u8]) -> Option<Self> {
        if buf.len() != Self::LEN {
            return None;
        }
        let seq = u64::from_le_bytes(buf[0..8].try_into().ok()?);
        let client_shard = u32::from_le_bytes(buf[8..12].try_into().ok()?);
        let route_key = u32::from_le_bytes(buf[12..16].try_into().ok()?);
        Some(Self {
            seq,
            client_shard,
            route_key,
        })
    }
}

#[derive(Clone, Copy, Debug)]
struct WorkItem {
    payload: [u8; WireMsg::LEN],
    len: usize,
    from: SocketAddr,
    routed_shard: usize,
}

#[derive(Clone, Copy, Debug, Default)]
struct IngressCounter {
    recv: u64,
    routed: u64,
    invalid: u64,
}

#[derive(Clone, Debug)]
struct SeqPingConfig {
    shards: usize,
    client_shards: usize,
    rounds_per_client: usize,
    local_client_shards: usize,
    local_worker_shards: usize,
    pin_threads: bool,
    topology_profile: TopologyProfile,
}

#[derive(Clone, Debug)]
struct SeqPingRunSummary {
    client_counters: Vec<Counter>,
    server_counters: Vec<Counter>,
    ingress_counter: IngressCounter,
    elapsed: std::time::Duration,
}

#[derive(Clone, Debug, Default)]
struct IngressRunSummary {
    ingress_counter: IngressCounter,
    local_worker_counters: Vec<Counter>,
    ingress_buf: [u8; WireMsg::LEN],
}

enum IngressPumpStatus {
    Continue,
    Stop,
}

struct InlineIngressRun<'a> {
    ingress_pin_cpu: Option<usize>,
    inline_client_pin_cpu: Option<usize>,
    ingress_socket: &'a UdpSocket,
    ingress_send_socket: &'a UdpSocket,
    worker_txs: &'a [mpsc::Sender<WorkItem>],
    shards: usize,
    local_worker_shards: usize,
    local_client_shards: usize,
    rounds_per_client: usize,
    ingress_addr: SocketAddr,
    client_counters: &'a mut [Counter],
}

struct ClientRound<'a> {
    socket: &'a UdpSocket,
    client_shard: usize,
    seq: u64,
    server_addr: SocketAddr,
    client_buf: &'a mut [u8; WireMsg::LEN],
    out: &'a mut Counter,
}

struct IngressPump<'a> {
    socket: &'a UdpSocket,
    local_send_socket: &'a UdpSocket,
    worker_txs: &'a [mpsc::Sender<WorkItem>],
    shards: usize,
    local_worker_shards: usize,
    buf: &'a mut [u8; WireMsg::LEN],
    ingress_counter: &'a mut IngressCounter,
    local_worker_counters: &'a mut [Counter],
}

fn main() {
    let config = SeqPingConfig::from_env();

    println!(
        "udp_shard_ping_pong_seq profile={:?} shards={} client_shards={} rounds_per_client={} local_client_shards={} local_worker_shards={} pin_threads={}",
        config.topology_profile.profile_kind,
        config.shards,
        config.client_shards,
        config.rounds_per_client,
        config.local_client_shards,
        config.local_worker_shards,
        config.pin_threads
    );

    let summary = run_seq_ping_hosted(&config);
    let expected = (config.client_shards as u64) * (config.rounds_per_client as u64);

    let client_sent_total = summary.client_counters.iter().map(|c| c.sent).sum::<u64>();
    let client_recv_total = summary.client_counters.iter().map(|c| c.recv).sum::<u64>();
    let server_recv_total = summary.server_counters.iter().map(|c| c.recv).sum::<u64>();
    let server_sent_total = summary.server_counters.iter().map(|c| c.sent).sum::<u64>();

    println!(
        "ingress recv={} routed={} invalid={}",
        summary.ingress_counter.recv,
        summary.ingress_counter.routed,
        summary.ingress_counter.invalid
    );
    for (idx, c) in summary.client_counters.iter().enumerate() {
        println!("client shard={} sent={} recv={}", idx, c.sent, c.recv);
    }
    for (idx, c) in summary.server_counters.iter().enumerate() {
        println!("worker shard={} recv={} sent={}", idx, c.recv, c.sent);
    }
    println!(
        "totals expected={} client_sent={} client_recv={} server_recv={} server_sent={} elapsed_ms={} req_per_sec={:.0}",
        expected,
        client_sent_total,
        client_recv_total,
        server_recv_total,
        server_sent_total,
        summary.elapsed.as_millis(),
        (expected as f64) / summary.elapsed.as_secs_f64()
    );

    assert_eq!(client_sent_total, expected, "client sent mismatch");
    assert_eq!(client_recv_total, expected, "client recv mismatch");
    assert_eq!(server_recv_total, expected, "server recv mismatch");
    assert_eq!(server_sent_total, expected, "server sent mismatch");
}

impl SeqPingConfig {
    fn from_env() -> Self {
        let shards = parse_env_usize("SEQ_PING_SHARDS", 4).max(1);
        let client_shards = parse_env_usize("SEQ_PING_CLIENT_SHARDS", shards).max(1);
        let rounds_per_client = parse_env_usize("SEQ_PING_ROUNDS_PER_CLIENT", 100_000).max(1);
        let local_client_shards = parse_env_usize(
            "SEQ_PING_LOCAL_CLIENT_SHARDS",
            DEFAULT_SEQ_LOCAL_CLIENT_SHARDS,
        )
        .clamp(1, client_shards);
        let local_worker_shards =
            parse_env_usize("SEQ_PING_LOCAL_WORKERS", DEFAULT_SEQ_LOCAL_WORKER_SHARDS)
                .clamp(1, shards);
        let pin_threads = parse_env_bool("SEQ_PING_PIN_THREADS", true);
        let topology_profile = parse_runtime_profile("SEQ_PING_RUNTIME_PROFILE", shards);

        Self {
            shards,
            client_shards,
            rounds_per_client,
            local_client_shards,
            local_worker_shards,
            pin_threads,
            topology_profile,
        }
    }
}

fn run_seq_ping_hosted(config: &SeqPingConfig) -> SeqPingRunSummary {
    let topology = config
        .topology_profile
        .resolve()
        .expect("resolve sequential ping topology");
    let shards = config.shards;
    let client_shards = config.client_shards;
    let rounds_per_client = config.rounds_per_client;
    let local_client_shards = config.local_client_shards.min(client_shards);
    let local_worker_shards = config.local_worker_shards.min(shards);
    let pin_threads = config.pin_threads;
    let cpu_plan = topology.topology.cpu_plan();
    let ingress_socket = UdpSocket::bind("127.0.0.1:0").expect("bind ingress socket");
    ingress_socket
        .set_nonblocking(true)
        .expect("set nonblocking ingress socket");
    let ingress_addr = ingress_socket.local_addr().expect("ingress local addr");

    let mut worker_joins = Vec::with_capacity(shards.saturating_sub(local_worker_shards));
    let mut worker_txs = Vec::with_capacity(shards.saturating_sub(local_worker_shards));
    for shard in local_worker_shards..shards {
        let (tx, rx) = mpsc::channel::<WorkItem>();
        worker_txs.push(tx);

        let send_socket = ingress_socket.try_clone().expect("clone ingress socket");
        let pin_cpu = pin_threads.then(|| cpu_plan_for_shard(&cpu_plan, shard));
        worker_joins.push(spawn_host_thread(
            format!("seq-ping-worker-{shard}"),
            pin_cpu,
            move || run_worker(shard, send_socket, rx),
        ));
    }

    let ingress_pin_cpu = pin_threads.then(|| cpu_plan_for_shard(&cpu_plan, 0));
    let ingress_send_socket = ingress_socket
        .try_clone()
        .expect("clone ingress send socket");

    let started = Instant::now();
    let (client_tx, client_rx) = mpsc::channel::<(usize, Counter)>();
    let mut client_joins = Vec::with_capacity(client_shards.saturating_sub(local_client_shards));
    for client_shard in local_client_shards..client_shards {
        let tx = client_tx.clone();
        let client_pin_cpu =
            pin_threads.then(|| cpu_plan_for_client(&cpu_plan, shards, client_shard));
        client_joins.push(spawn_host_thread(
            format!("seq-ping-client-{client_shard}"),
            client_pin_cpu,
            move || {
                let counter = run_client(client_shard, rounds_per_client, ingress_addr);
                tx.send((client_shard, counter))
                    .expect("send client counters");
            },
        ));
    }
    let inline_client_pin_cpu = pin_threads.then(|| cpu_plan_for_client(&cpu_plan, shards, 0));
    drop(client_tx);

    let mut client_counters = vec![Counter::default(); client_shards];
    let mut ingress_summary = run_inline_clients_and_ingress(InlineIngressRun {
        ingress_pin_cpu,
        inline_client_pin_cpu,
        ingress_socket: &ingress_socket,
        ingress_send_socket: &ingress_send_socket,
        worker_txs: &worker_txs,
        shards,
        local_worker_shards,
        local_client_shards,
        rounds_per_client,
        ingress_addr,
        client_counters: &mut client_counters[..local_client_shards],
    });
    let mut received_remote_clients = 0usize;
    let remote_client_count = client_shards.saturating_sub(local_client_shards);
    while received_remote_clients < remote_client_count {
        match client_rx.try_recv() {
            Ok((client_shard, counter)) => {
                client_counters[client_shard] = counter;
                received_remote_clients += 1;
            }
            Err(TryRecvError::Empty) => {
                let mut pump = IngressPump {
                    socket: &ingress_socket,
                    local_send_socket: &ingress_send_socket,
                    worker_txs: &worker_txs,
                    shards,
                    local_worker_shards,
                    buf: &mut ingress_summary.ingress_buf,
                    ingress_counter: &mut ingress_summary.ingress_counter,
                    local_worker_counters: &mut ingress_summary.local_worker_counters,
                };
                match pump.pump_once(true) {
                    IngressPumpStatus::Continue => {}
                    IngressPumpStatus::Stop => break,
                }
            }
            Err(TryRecvError::Disconnected) => break,
        }
    }
    for join in client_joins {
        join.join().expect("join client");
    }
    drop(worker_txs);

    let mut server_counters = vec![Counter::default(); shards];
    for (shard, counter) in ingress_summary
        .local_worker_counters
        .into_iter()
        .enumerate()
    {
        server_counters[shard] = counter;
    }
    for (offset, join) in worker_joins.into_iter().enumerate() {
        let shard = offset + local_worker_shards;
        server_counters[shard] = join.join().expect("join worker");
    }

    SeqPingRunSummary {
        client_counters,
        server_counters,
        ingress_counter: ingress_summary.ingress_counter,
        elapsed: started.elapsed(),
    }
}

fn run_inline_clients_and_ingress(run: InlineIngressRun<'_>) -> IngressRunSummary {
    if let Some(cpu) = run.ingress_pin_cpu.or(run.inline_client_pin_cpu) {
        pin_current_thread(cpu);
    }

    let mut ingress_counter = IngressCounter::default();
    let mut local_worker_counters = vec![Counter::default(); run.local_worker_shards];
    let mut ingress_buf = [0u8; WireMsg::LEN];

    {
        let mut pump = IngressPump {
            socket: run.ingress_socket,
            local_send_socket: run.ingress_send_socket,
            worker_txs: run.worker_txs,
            shards: run.shards,
            local_worker_shards: run.local_worker_shards,
            buf: &mut ingress_buf,
            ingress_counter: &mut ingress_counter,
            local_worker_counters: &mut local_worker_counters,
        };

        for client_shard in 0..run.local_client_shards {
            let client_socket = UdpSocket::bind("127.0.0.1:0").expect("bind inline client socket");
            client_socket
                .set_nonblocking(true)
                .expect("set nonblocking inline client socket");
            let mut client_buf = [0u8; WireMsg::LEN];

            for seq in 0..run.rounds_per_client {
                run_client_round_with_ingress(
                    ClientRound {
                        socket: &client_socket,
                        client_shard,
                        seq: seq as u64,
                        server_addr: run.ingress_addr,
                        client_buf: &mut client_buf,
                        out: &mut run.client_counters[client_shard],
                    },
                    &mut pump,
                );
            }
        }

        while matches!(pump.pump_once(false), IngressPumpStatus::Continue) {}
    }

    IngressRunSummary {
        ingress_counter,
        local_worker_counters,
        ingress_buf,
    }
}

fn run_client_round_with_ingress(round: ClientRound<'_>, pump: &mut IngressPump<'_>) {
    let route_key = round.seq as u32;
    let msg = WireMsg {
        seq: round.seq,
        client_shard: round.client_shard as u32,
        route_key,
    };
    let payload = msg.encode();

    loop {
        match round.socket.send_to(&payload, round.server_addr) {
            Ok(sent) => {
                assert_eq!(sent, WireMsg::LEN, "client sent partial payload");
                round.out.sent += 1;
                break;
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                let _ = pump.pump_once(true);
            }
            Err(err) => panic!("client send_to failed: {err}"),
        }
    }

    loop {
        match pump.pump_once(true) {
            IngressPumpStatus::Continue => {}
            IngressPumpStatus::Stop => {}
        }
        match round.socket.recv_from(round.client_buf) {
            Ok((n, _from)) => {
                let echoed =
                    WireMsg::decode(&round.client_buf[..n]).expect("decode client payload");
                assert_eq!(echoed.seq, msg.seq, "echoed seq mismatch");
                assert_eq!(
                    echoed.client_shard, msg.client_shard,
                    "echoed client shard mismatch"
                );
                assert_eq!(echoed.route_key, msg.route_key, "echoed route key mismatch");
                round.out.recv += 1;
                break;
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::hint::spin_loop();
            }
            Err(err) => panic!("client recv_from failed: {err}"),
        }
    }
}

impl<'a> IngressPump<'a> {
    fn pump_once(&mut self, run_flag: bool) -> IngressPumpStatus {
        match self.socket.recv_from(self.buf) {
            Ok((n, from)) => {
                if n == 1 && !run_flag {
                    return IngressPumpStatus::Stop;
                }
                self.ingress_counter.recv += 1;
                if n != WireMsg::LEN {
                    self.ingress_counter.invalid += 1;
                    return IngressPumpStatus::Continue;
                }
                let Some(msg) = WireMsg::decode(&self.buf[..n]) else {
                    self.ingress_counter.invalid += 1;
                    return IngressPumpStatus::Continue;
                };
                let routed = route_shard(&msg, self.shards);
                let item = WorkItem {
                    payload: *self.buf,
                    len: n,
                    from,
                    routed_shard: routed,
                };
                if routed < self.local_worker_shards {
                    let counter = &mut self.local_worker_counters[routed];
                    process_work_item(routed, self.local_send_socket, item, counter);
                } else if self.worker_txs[routed - self.local_worker_shards]
                    .send(item)
                    .is_err()
                {
                    return IngressPumpStatus::Stop;
                }
                self.ingress_counter.routed += 1;
                IngressPumpStatus::Continue
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                if !run_flag {
                    IngressPumpStatus::Stop
                } else {
                    std::hint::spin_loop();
                    IngressPumpStatus::Continue
                }
            }
            Err(err) => panic!("ingress recv_from failed: {err}"),
        }
    }
}

fn run_worker(shard: usize, send_socket: UdpSocket, rx: mpsc::Receiver<WorkItem>) -> Counter {
    let mut out = Counter::default();
    while let Ok(item) = rx.recv() {
        process_work_item(shard, &send_socket, item, &mut out);
    }
    out
}

fn process_work_item(shard: usize, send_socket: &UdpSocket, item: WorkItem, out: &mut Counter) {
    assert_eq!(
        item.routed_shard, shard,
        "message routed to wrong worker shard"
    );
    out.recv += 1;
    loop {
        match send_socket.send_to(&item.payload[..item.len], item.from) {
            Ok(sent) => {
                assert_eq!(sent, item.len, "worker echoed partial payload");
                out.sent += 1;
                break;
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::hint::spin_loop();
            }
            Err(err) => panic!("worker send_to failed: {err}"),
        }
    }
}

fn run_client(client_shard: usize, rounds_per_client: usize, server_addr: SocketAddr) -> Counter {
    let socket = UdpSocket::bind("127.0.0.1:0").expect("bind client socket");
    socket
        .set_nonblocking(true)
        .expect("set nonblocking client socket");

    let mut out = Counter::default();
    let mut buf = [0u8; WireMsg::LEN];
    for seq in 0..rounds_per_client {
        run_client_round(
            &socket,
            client_shard,
            seq as u64,
            server_addr,
            &mut buf,
            &mut out,
        );
    }

    out
}

fn run_client_round(
    socket: &UdpSocket,
    client_shard: usize,
    seq: u64,
    server_addr: SocketAddr,
    buf: &mut [u8; WireMsg::LEN],
    out: &mut Counter,
) {
    let route_key = seq as u32;
    let msg = WireMsg {
        seq,
        client_shard: client_shard as u32,
        route_key,
    };
    let payload = msg.encode();

    loop {
        match socket.send_to(&payload, server_addr) {
            Ok(sent) => {
                assert_eq!(sent, WireMsg::LEN, "client sent partial payload");
                out.sent += 1;
                break;
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::hint::spin_loop();
            }
            Err(err) => panic!("client send_to failed: {err}"),
        }
    }

    loop {
        match socket.recv_from(buf) {
            Ok((n, _from)) => {
                let echoed = WireMsg::decode(&buf[..n]).expect("decode client payload");
                assert_eq!(echoed.seq, msg.seq, "echoed seq mismatch");
                assert_eq!(
                    echoed.client_shard, msg.client_shard,
                    "echoed client shard mismatch"
                );
                assert_eq!(echoed.route_key, msg.route_key, "echoed route key mismatch");
                out.recv += 1;
                break;
            }
            Err(err) if err.kind() == std::io::ErrorKind::WouldBlock => {
                std::hint::spin_loop();
            }
            Err(err) => panic!("client recv_from failed: {err}"),
        }
    }
}

fn route_shard(msg: &WireMsg, shards: usize) -> usize {
    (msg.route_key as usize) % shards
}

fn parse_env_bool(name: &str, default: bool) -> bool {
    match std::env::var(name)
        .ok()
        .map(|s| s.to_ascii_lowercase())
        .as_deref()
    {
        Some("1") | Some("true") | Some("yes") | Some("on") => true,
        Some("0") | Some("false") | Some("no") | Some("off") => false,
        _ => default,
    }
}

fn parse_env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .unwrap_or(default)
}

fn pin_current_thread(shard: usize) {
    assert!(pin_current_to_cpu(shard), "sched_setaffinity failed");
}

fn spawn_host_thread<F, T>(name: String, pin_cpu: Option<usize>, f: F) -> thread::JoinHandle<T>
where
    F: FnOnce() -> T + Send + 'static,
    T: Send + 'static,
{
    thread::Builder::new()
        .name(name.clone())
        .spawn(move || {
            if let Some(cpu) = pin_cpu {
                pin_current_thread(cpu);
            }
            f()
        })
        .unwrap_or_else(|err| panic!("spawn host thread {name} failed: {err}"))
}

fn cpu_plan_for_shard(cpu_plan: &[usize], shard: usize) -> usize {
    cpu_plan.get(shard).copied().unwrap_or_else(|| {
        shard
            % thread::available_parallelism()
                .map(|n| n.get())
                .unwrap_or(1)
    })
}

fn cpu_plan_for_client(cpu_plan: &[usize], shards: usize, client_shard: usize) -> usize {
    let cpu_count = thread::available_parallelism()
        .map(|count| count.get())
        .unwrap_or(1);
    cpu_plan
        .get((shards + client_shard) % cpu_plan.len().max(1))
        .copied()
        .unwrap_or_else(|| (shards + client_shard) % cpu_count)
}

fn parse_runtime_profile(name: &str, shards: usize) -> TopologyProfile {
    std::env::var(name)
        .ok()
        .as_deref()
        .and_then(dataplane_runtime::runtime_topology::ProfileKind::parse_name)
        .map(TopologyProfile::for_kind)
        .unwrap_or_else(TopologyProfile::balanced_dual_shard)
        .with_shard_count(shards)
}
