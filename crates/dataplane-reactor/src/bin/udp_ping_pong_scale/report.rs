use super::config::{RunBackendConfig, UdpScaleBackendSummary, UdpScaleConfig, UdpScaleSample};
use super::server::run_backend;
use super::types::Backend;

pub(crate) fn run() {
    let config = UdpScaleConfig::from_env();
    println!(
        "udp_ping_pong_scale profile={:?} shards={:?} pairs_per_shard={} rounds_per_pair={} payload={} uring_send_wait={} uring_poll_first={} pin_threads={} server_reuseport={} server_sockets={} server_recv_mode={:?} server_batch={} warmup_runs={} measure_runs={}",
        config.runtime_profile.profile_kind,
        config.shard_list,
        config.pairs_per_shard,
        config.rounds_per_pair,
        config.payload_len,
        config.uring_send_wait,
        config.uring_poll_first,
        config.pin_threads,
        config.server_sockets > 1,
        config.server_sockets,
        config.server_recv_mode,
        config.server_batch,
        config.warmup_runs,
        config.measure_runs
    );

    for backend_summary in run_udp_scale_hosted(&config) {
        println!("backend={:?}", backend_summary.backend);
        for sample in backend_summary.samples {
            println!(
                "  shards={} datagrams={} median_datagrams_per_sec={:.0} avg_datagrams_per_sec={:.0} max_send_inflight={}",
                sample.shards,
                sample.datagrams,
                sample.median_datagrams_per_sec,
                sample.avg_datagrams_per_sec,
                sample.max_send_inflight
            );
        }
    }
}

fn run_udp_scale_hosted(config: &UdpScaleConfig) -> Vec<UdpScaleBackendSummary> {
    let mut out = Vec::with_capacity(2);
    for backend in [Backend::Syscall, Backend::Uring] {
        let mut samples_out = Vec::with_capacity(config.shard_list.len());
        for shards in config.shard_list.iter().copied() {
            let backend_config = RunBackendConfig {
                client_backend: backend,
                runtime_profile: config.runtime_profile.clone(),
                shards,
                pairs_per_shard: config.pairs_per_shard,
                rounds_per_pair: config.rounds_per_pair,
                payload_len: config.payload_len,
                uring_send_wait: config.uring_send_wait,
                server_sockets: config.server_sockets,
                server_recv_mode: config.server_recv_mode,
                server_batch: config.server_batch,
                local_client_shards: config.local_client_shards,
                local_server_workers: config.local_server_workers,
                pin_threads: config.pin_threads,
            };

            for _ in 0..config.warmup_runs {
                let _ = run_backend(backend_config.clone());
            }

            let mut throughputs = Vec::with_capacity(config.measure_runs);
            let mut completed_last = 0usize;
            let mut max_send_inflight_seen = 0usize;
            for _ in 0..config.measure_runs {
                let started = std::time::Instant::now();
                let stats = run_backend(backend_config.clone());
                let elapsed = started.elapsed();
                throughputs.push(stats.datagrams as f64 / elapsed.as_secs_f64());
                completed_last = stats.datagrams;
                max_send_inflight_seen = max_send_inflight_seen.max(stats.max_send_inflight);
            }
            throughputs.sort_by(|a, b| a.partial_cmp(b).expect("finite throughput samples"));
            let median = throughputs[throughputs.len() / 2];
            let avg = throughputs.iter().sum::<f64>() / (throughputs.len() as f64);
            samples_out.push(UdpScaleSample {
                shards,
                datagrams: completed_last,
                median_datagrams_per_sec: median,
                avg_datagrams_per_sec: avg,
                max_send_inflight: max_send_inflight_seen,
            });
        }
        out.push(UdpScaleBackendSummary {
            backend,
            samples: samples_out,
        });
    }
    out
}
