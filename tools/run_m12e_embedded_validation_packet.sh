#!/usr/bin/env bash

set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

run_check() {
  local label="$1"
  local cmd="$2"

  echo "[m12e-packet] start ${label}"
  echo "[m12e-packet] cmd: ${cmd}"
  if bash -lc "${cmd}"; then
    echo "[m12e-packet] pass ${label}"
  else
    echo "[m12e-packet] fail ${label}" >&2
    return 1
  fi
}

checks=(
  "dp-emb-0171|cargo test -p dataplane-core-reactor embedded_ -- --list"
  "dp-emb-0172|cargo test -p dataplane-runtime embedded_ -- --list"
  "dp-emb-0173|cargo test -p dataplane-runtime embedded_host_loop"
  "dp-emb-0174|cargo test -p dataplane-runtime runtime_profiles"
  "dp-emb-0175|cargo test -p dataplane-runtime"
  "dp-emb-0176|cargo test -p dataplane-reactor --bin udp_ping_pong_scale --no-run"
  "dp-emb-0177|UDP_SCALE_SHARDS=1,2 UDP_SCALE_SERVER_SOCKETS=1 UDP_SCALE_SERVER_REUSEPORT=0 UDP_SCALE_PAIRS_PER_SHARD=1 UDP_SCALE_ROUNDS_PER_PAIR=128 UDP_SCALE_WARMUP_RUNS=0 UDP_SCALE_MEASURE_RUNS=1 cargo run -p dataplane-reactor --bin udp_ping_pong_scale"
  "dp-emb-0178|UDP_SCALE_SHARDS=1,2 UDP_SCALE_SERVER_SOCKETS=2 UDP_SCALE_SERVER_REUSEPORT=1 UDP_SCALE_PAIRS_PER_SHARD=1 UDP_SCALE_ROUNDS_PER_PAIR=128 UDP_SCALE_WARMUP_RUNS=0 UDP_SCALE_MEASURE_RUNS=1 cargo run -p dataplane-reactor --bin udp_ping_pong_scale"
  "esp32-compile-gate|./tools/check_esp32_feature_gate_compile.sh"
)

for entry in "${checks[@]}"; do
  label="${entry%%|*}"
  cmd="${entry#*|}"
  run_check "${label}" "${cmd}"
done

echo "[m12e-packet] all checks passed"
