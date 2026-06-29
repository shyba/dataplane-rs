#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-virtio-smoke/src/main.rs"
manifest="crates/dataplane-x86_64-virtio-smoke/Cargo.toml"
build_script="tools/x86_64_virtio_smoke_build.sh"
runner="tools/x86_64_virtio_udp_bench_run.sh"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_literal() {
  local file="$1"
  local needle="$2"
  local note="$3"
  rg -q --fixed-strings -- "$needle" "$file" || fail "$note"
}

echo "=== x86_64 Virtio UDP Bench Contract Guard ==="

[[ -f "$main" ]] || fail "missing x86_64 virtio smoke main"
[[ -f "$manifest" ]] || fail "missing x86_64 virtio manifest"
[[ -f "$build_script" ]] || fail "missing x86_64 virtio build script"
[[ -f "$runner" ]] || fail "missing x86_64 virtio UDP bench runner"

require_literal "$manifest" 'udp-bench = []' \
  "UDP bench must be feature-gated away from the default smoke"
require_literal Makefile 'x86_64-virtio-udp-bench-build' \
  "Makefile must expose a UDP bench build target"
require_literal Makefile 'x86_64-virtio-udp-bench-contract' \
  "Makefile must expose a UDP bench contract guard target"
require_literal Makefile 'x86_64-virtio-udp-bench:' \
  "Makefile must expose a runnable UDP bench target"
require_literal "$build_script" 'X86_64_VIRTIO_FEATURES' \
  "build script must support opt-in feature builds for the UDP bench"

require_literal "$main" 'UDP_BENCH_PACKETS' \
  "guest must keep the UDP bench packet count explicit"
require_literal "$main" 'NetworkDriverTask' \
  "UDP bench must run inside the existing network task object"
require_literal "$main" 'arm_receive' \
  "driver abstraction must be able to re-arm RX between UDP packets"
require_literal "$main" 'prepare_rx_buffer' \
  "UDP bench must recycle the virtio RX buffer instead of using a TX-only path"
require_literal "$main" 'validate_udp_bench_request' \
  "guest must parse and validate inbound UDP requests"
require_literal "$main" 'prepare_udp_bench_response_packet' \
  "guest must generate deterministic UDP responses"
require_literal "$main" 'ipv4_header_checksum' \
  "guest must use an IPv4 header checksum instead of opaque payload matching"
require_literal "$main" 'DPX86:UDP-BENCH' \
  "guest must emit a UDP bench completion marker"
require_literal "$main" 'protected_net_region' \
  "UDP bench must keep driver memory behind the protected task region"
require_literal "$main" 'mmu::allow_region' \
  "protected task access must still be explicitly allowed"
require_literal "$main" 'mmu::deny_region' \
  "protected task access must still be explicitly denied"
require_literal "$main" 'DRIVER_FAULT_SEEN' \
  "UDP bench must preserve the denied-page fault containment proof"

require_literal "$runner" 'qemu-system-x86_64' \
  "UDP bench runner must use qemu-system-x86_64"
require_literal "$runner" '-device virtio-net-pci-transitional,netdev=xnet,mac=52:54:00:12:34:56' \
  "UDP bench runner must attach the same QEMU virtio network device"
require_literal "$runner" '-netdev socket,id=xnet,udp=127.0.0.1:"$host_port",localaddr=127.0.0.1:"$qemu_port"' \
  "UDP bench runner must expose a host raw-frame socket"
require_literal "$runner" 'DP_PACKET_COUNT' \
  "UDP bench runner must pass an explicit packet count to the host peer"
require_literal "$runner" 'udp_packets=' \
  "UDP bench runner must log the requested packet count"
require_literal "$runner" 'udp_responses=' \
  "UDP bench runner must log the completed response count"
require_literal "$runner" 'elapsed_ms=' \
  "UDP bench runner must log elapsed time"
require_literal "$runner" 'pps=' \
  "UDP bench runner must log packets-per-second evidence"
require_literal "$runner" 'DPX86:UDP-BENCH' \
  "UDP bench runner must require the guest UDP bench marker"
require_literal "$runner" 'DPX86:DRIVER-FAULT-CONTAINED' \
  "UDP bench runner must preserve fault containment validation"
require_literal "$runner" '020000000004525400123456080045' \
  "UDP bench runner must pcap-check a guest IPv4/UDP response"

echo "x86_64 virtio UDP bench contract guard passed."
