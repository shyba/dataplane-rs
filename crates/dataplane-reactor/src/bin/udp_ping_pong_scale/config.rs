use super::types::{Backend, ServerRecvMode};
use dataplane_runtime::runtime_profiles::TopologyProfile;

// Keep local ownership defaults explicit.
pub(crate) const DEFAULT_SCALE_LOCAL_CLIENT_SHARDS: usize = 1;
pub(crate) const DEFAULT_SCALE_LOCAL_SERVER_WORKERS: usize = 1;

#[derive(Clone, Debug)]
pub(crate) struct UdpScaleConfig {
    pub(crate) shard_list: Vec<usize>,
    pub(crate) runtime_profile: TopologyProfile,
    pub(crate) pairs_per_shard: usize,
    pub(crate) rounds_per_pair: usize,
    pub(crate) payload_len: usize,
    pub(crate) uring_send_wait: bool,
    pub(crate) server_sockets: usize,
    pub(crate) server_recv_mode: ServerRecvMode,
    pub(crate) server_batch: usize,
    pub(crate) uring_poll_first: bool,
    pub(crate) local_client_shards: usize,
    pub(crate) local_server_workers: usize,
    pub(crate) pin_threads: bool,
    pub(crate) warmup_runs: usize,
    pub(crate) measure_runs: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct RunBackendConfig {
    pub(crate) client_backend: Backend,
    pub(crate) runtime_profile: TopologyProfile,
    pub(crate) shards: usize,
    pub(crate) pairs_per_shard: usize,
    pub(crate) rounds_per_pair: usize,
    pub(crate) payload_len: usize,
    pub(crate) uring_send_wait: bool,
    pub(crate) server_sockets: usize,
    pub(crate) server_recv_mode: ServerRecvMode,
    pub(crate) server_batch: usize,
    pub(crate) local_client_shards: usize,
    pub(crate) local_server_workers: usize,
    pub(crate) pin_threads: bool,
}

#[derive(Clone, Copy, Debug, Default)]
pub(crate) struct RunStats {
    pub(crate) datagrams: usize,
    pub(crate) max_send_inflight: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct UdpScaleSample {
    pub(crate) shards: usize,
    pub(crate) datagrams: usize,
    pub(crate) median_datagrams_per_sec: f64,
    pub(crate) avg_datagrams_per_sec: f64,
    pub(crate) max_send_inflight: usize,
}

#[derive(Clone, Debug)]
pub(crate) struct UdpScaleBackendSummary {
    pub(crate) backend: Backend,
    pub(crate) samples: Vec<UdpScaleSample>,
}

impl UdpScaleConfig {
    pub(crate) fn from_env() -> Self {
        let shard_list = parse_shard_list("UDP_SCALE_SHARDS", &[1, 2, 4, 8]);
        let runtime_profile = parse_runtime_profile("UDP_SCALE_RUNTIME_PROFILE");
        let pairs_per_shard = parse_env_usize("UDP_SCALE_PAIRS_PER_SHARD", 1);
        let rounds_per_pair = parse_env_usize("UDP_SCALE_ROUNDS_PER_PAIR", 200_000);
        let payload_len = parse_env_usize("UDP_SCALE_PAYLOAD", 64);
        let uring_send_wait = parse_env_bool("UDP_SCALE_URING_SEND_WAIT", false);
        let reuseport = parse_env_bool("UDP_SCALE_SERVER_REUSEPORT", false);
        let mut server_sockets = parse_env_usize("UDP_SCALE_SERVER_SOCKETS", 1);
        if !reuseport {
            server_sockets = 1;
        }
        let server_recv_mode = parse_server_recv_mode("UDP_SCALE_SERVER_RECV_MODE");
        let server_batch = parse_env_usize("UDP_SCALE_SERVER_BATCH", 16).clamp(1, 64);
        let uring_poll_first = parse_env_bool("UDP_SCALE_URING_POLL_FIRST", false);
        let local_client_shards = parse_env_usize(
            "UDP_SCALE_LOCAL_CLIENT_SHARDS",
            DEFAULT_SCALE_LOCAL_CLIENT_SHARDS,
        );
        let local_server_workers = parse_env_usize(
            "UDP_SCALE_LOCAL_SERVER_WORKERS",
            DEFAULT_SCALE_LOCAL_SERVER_WORKERS,
        );
        let pin_threads = parse_env_bool("UDP_SCALE_PIN_THREADS", true);
        let warmup_runs = parse_env_usize("UDP_SCALE_WARMUP_RUNS", 0);
        let measure_runs = parse_env_usize("UDP_SCALE_MEASURE_RUNS", 1);

        Self {
            shard_list,
            runtime_profile,
            pairs_per_shard,
            rounds_per_pair,
            payload_len,
            uring_send_wait,
            server_sockets,
            server_recv_mode,
            server_batch,
            uring_poll_first,
            local_client_shards,
            local_server_workers,
            pin_threads,
            warmup_runs,
            measure_runs,
        }
    }
}

fn parse_env_usize(name: &str, default: usize) -> usize {
    std::env::var(name)
        .ok()
        .and_then(|s| s.parse::<usize>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(default)
}

fn parse_env_bool(name: &str, default: bool) -> bool {
    match std::env::var(name).ok().as_deref().map(|s| s.trim()) {
        Some("1") | Some("true") | Some("on") => true,
        Some("0") | Some("false") | Some("off") => false,
        _ => default,
    }
}

fn parse_server_recv_mode(name: &str) -> ServerRecvMode {
    match std::env::var(name)
        .ok()
        .as_deref()
        .map(|s| s.trim().to_ascii_lowercase())
        .as_deref()
    {
        Some("recvmmsg") | Some("mmsg") | Some("batch") => ServerRecvMode::Mmsg,
        Some("recvmsg_multishot") | Some("recvmsgmulti") | Some("multishot") | Some("mshot") => {
            ServerRecvMode::MsgMultishot
        }
        _ => ServerRecvMode::From,
    }
}

fn parse_runtime_profile(name: &str) -> TopologyProfile {
    std::env::var(name)
        .ok()
        .as_deref()
        .and_then(dataplane_runtime::runtime_profiles::ProfileKind::parse_name)
        .map(TopologyProfile::for_kind)
        .unwrap_or_else(TopologyProfile::balanced_dual_shard)
}

fn parse_shard_list(name: &str, default: &[usize]) -> Vec<usize> {
    let Some(value) = std::env::var(name).ok() else {
        return default.to_vec();
    };
    let mut out = Vec::new();
    for part in value.split(',') {
        if let Ok(v) = part.trim().parse::<usize>() {
            if v > 0 {
                out.push(v);
            }
        }
    }
    if out.is_empty() {
        default.to_vec()
    } else {
        out
    }
}
