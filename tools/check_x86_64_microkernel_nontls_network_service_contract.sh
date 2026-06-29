#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
network_task="crates/dataplane-x86_64-microkernel-smoke/src/network_task.rs"
scenarios="crates/dataplane-x86_64-microkernel-smoke/src/scenarios.rs"
virtio_net="crates/dataplane-x86_64-microkernel-smoke/src/virtio_net.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
matrix="tools/x86_64_microkernel_validation_matrix_run.sh"
makefile="Makefile"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_file() {
  [[ -f "$1" ]] || fail "missing required file: $1"
}

require_literal() {
  local file="$1"
  local needle="$2"
  local note="$3"
  rg -q --fixed-strings -- "$needle" "$file" || fail "$note"
}

reject_regex() {
  local file="$1"
  local regex="$2"
  local note="$3"
  local matches status
  set +e
  matches="$(rg -n -- "$regex" "$file")"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    printf '%s\n' "$matches"
    fail "$note"
  fi
  [[ "$status" -eq 1 ]] || fail "could not scan $file for forbidden regex: $regex"
}

rust_impl_body() {
  local file="$1"
  local item="$2"
  local body status
  set +e
  body="$(awk -v target="impl ${item} {" '
    BEGIN { depth = 0; found = 0; started = 0 }
    !started && index($0, target) {
      started = 1
      found = 1
    }
    started {
      print
      line = $0
      opens = gsub(/\{/, "{", line)
      line = $0
      closes = gsub(/\}/, "}", line)
      depth += opens - closes
      if (depth == 0) {
        exit 0
      }
    }
    END {
      if (!found || depth != 0) {
        exit 42
      }
    }
  ' "$file")"
  status=$?
  set -e
  [[ "$status" -eq 0 ]] || fail "could not extract complete impl ${item} body from $file"
  printf '%s\n' "$body"
}

reject_impl_literal() {
  local file="$1"
  local item="$2"
  local needle="$3"
  local note="$4"
  local body
  body="$(rust_impl_body "$file" "$item")"
  if [[ "$body" == *"$needle"* ]]; then
    printf '%s\n' "$body" | rg -n --fixed-strings -- "$needle" || true
    fail "$note"
  fi
}

echo "=== x86_64 Microkernel Non-TLS Network Service Contract Guard ==="

require_file "$main"
require_file "$network_task"
require_file "$scenarios"
require_file "$virtio_net"
require_file "$runner"
require_file "$matrix"
require_file "$makefile"

require_literal "$runner" '--nontls-network-service-proof' \
  "FAT32 runner must expose the non-TLS network service proof mode"
require_literal "$runner" 'DP_MICROKERNEL_NONTLS_NETWORK_SERVICE_PROOF' \
  "FAT32 runner must expose an env override for the non-TLS proof"
require_literal "$runner" 'negative_network_inputs_sent=true' \
  "FAT32 runner must record negative network input evidence"
require_literal "$runner" 'negative_enabled = nontls_network_service or nontls_network_negative_matrix' \
  "FAT32 runner must share negative network-frame enablement across service and negative-matrix proofs"
require_literal "$runner" 'negative_enabled and seen_arp_reply and negative_rounds < negative_target_rounds' \
  "FAT32 runner must inject negative network frames only after the live ARP reply reaches TcpIpTask"
require_literal "$runner" 'negative_done = (not negative_enabled) or negative_rounds >= negative_target_rounds' \
  "FAT32 runner must finish negative network input evidence only after the full pressure target"
require_literal "$runner" 'not negative_enabled or negative_rounds > 0' \
  "FAT32 runner must not report negative inputs sent when no negative frame was sent"
require_literal "$runner" 'DPMK:NET-DROP-POLICY-OK' \
  "FAT32 runner must require the guest drop-policy marker"
require_literal "$runner" 'DPMK:NET-DROP-ARP-WRONG-TARGET:' \
  "FAT32 runner must require ARP wrong-target drop evidence"
require_literal "$runner" 'DPMK:NET-DROP-ARP-MALFORMED:' \
  "FAT32 runner must require ARP malformed drop evidence"
require_literal "$runner" 'DPMK:NET-DROP-UNSUPPORTED-ETHERTYPE:' \
  "FAT32 runner must require unsupported ethertype drop evidence"
require_literal "$runner" 'DPMK:NET-DROP-IPV4-WRONG-TARGET:' \
  "FAT32 runner must require IPv4 wrong-target drop evidence"
require_literal "$runner" 'DPMK:NET-DROP-UNSUPPORTED-PROTO:' \
  "FAT32 runner must require unsupported IPv4 protocol drop evidence"
require_literal "$runner" 'DPMK:NET-DROP-ICMP-NON-ECHO:' \
  "FAT32 runner must require ICMP non-echo drop evidence"
require_literal "$runner" 'DPMK:NET-DROP-ICMP-MALFORMED:' \
  "FAT32 runner must require ICMP malformed drop evidence"
require_literal "$runner" 'DPMK:NET-DROP-UDP-WRONG-PORT:' \
  "FAT32 runner must require UDP wrong-port drop evidence"
require_literal "$runner" 'DPMK:NET-DROP-UDP-BAD-PAYLOAD:' \
  "FAT32 runner must require UDP bad-payload drop evidence"
require_literal "$runner" 'DPMK:NET-DROP-UDP-MALFORMED:' \
  "FAT32 runner must require UDP malformed drop evidence"
require_literal "$runner" '4450554e53555050' \
  "FAT32 runner must require pcap evidence for unsupported protocol payload"
require_literal "$runner" '4450455448455221' \
  "FAT32 runner must require pcap evidence for unsupported ethertype payload"
require_literal "$runner" '44504e4f4543484f' \
  "FAT32 runner must require pcap evidence for ICMP non-echo payload"
require_literal "$runner" '44504d414c4621' \
  "FAT32 runner must require pcap evidence for malformed ICMP payload"
require_literal "$runner" '4241445544503030' \
  "FAT32 runner must require pcap evidence for bad UDP payload"

require_literal "$network_task" 'struct NetworkDropCounters' \
  "TcpIpTask must own fixed non-TLS drop counters"
require_literal "$network_task" 'network_drop_counters: NetworkDropCounters' \
  "TcpIpTask must store network drop counters"
require_literal "$network_task" 'network_drop_counters.arp_wrong_target += 1' \
  "TcpIpTask must count ARP wrong-target drops"
require_literal "$network_task" 'network_drop_counters.arp_malformed += 1' \
  "TcpIpTask must count ARP malformed drops"
require_literal "$network_task" 'network_drop_counters.unsupported_ethertype += 1' \
  "TcpIpTask must count unsupported ethertype drops"
require_literal "$network_task" 'src != VM_MAC && (dst == VM_MAC || dst == [0xff; 6])' \
  "TcpIpTask must not count its own raw probe as a host unsupported-ethertype drop"
require_literal "$network_task" 'network_drop_counters.ipv4_wrong_target += 1' \
  "TcpIpTask must count IPv4 wrong-target drops"
require_literal "$network_task" 'network_drop_counters.unsupported_ipv4_protocol += 1' \
  "TcpIpTask must count unsupported IPv4 protocol drops"
require_literal "$network_task" 'network_drop_counters.icmp_non_echo += 1' \
  "TcpIpTask must count ICMP non-echo drops"
require_literal "$network_task" 'network_drop_counters.icmp_malformed += 1' \
  "TcpIpTask must count ICMP malformed drops"
require_literal "$network_task" 'network_drop_counters.udp_wrong_port += 1' \
  "TcpIpTask must count UDP wrong-port drops"
require_literal "$network_task" 'network_drop_counters.udp_bad_payload += 1' \
  "TcpIpTask must count UDP bad-payload drops"
require_literal "$network_task" 'network_drop_counters.udp_malformed += 1' \
  "TcpIpTask must count UDP malformed drops"
require_literal "$scenarios" 'fn emit_network_drop_policy_markers' \
  "kernel must emit explicit drop-policy markers"
require_literal "$scenarios" 'fn network_drop_policy_started' \
  "kernel must distinguish started live drop evidence from absent drop evidence"

python3 - "$scenarios" <<'PY'
import sys

path = sys.argv[1]
source = open(path, encoding="utf-8").read()
needle = "fn verify_static_network_drop_policy"
start = source.find(needle)
if start < 0:
    raise SystemExit("FAIL: missing verify_static_network_drop_policy")
brace = source.find("{", start)
if brace < 0:
    raise SystemExit("FAIL: verify_static_network_drop_policy has no body")
depth = 0
end = None
for offset, char in enumerate(source[brace:], brace):
    if char == "{":
        depth += 1
    elif char == "}":
        depth -= 1
        if depth == 0:
            end = offset + 1
            break
if end is None:
    raise SystemExit("FAIL: could not extract verify_static_network_drop_policy body")
body = source[start:end]
if "let mut tcpip_task = TcpIpTask::new();" not in body:
    raise SystemExit("FAIL: static drop-policy probe must use an isolated TcpIpTask")
for forbidden in ("emit_network_drop_policy", "DPMK:NET-DROP"):
    if forbidden in body:
        raise SystemExit(
            "FAIL: static drop-policy probe must not emit live drop-policy markers"
        )
PY

require_literal "$matrix" 'nontls-network-service' \
  "validation matrix runner must know the non-TLS network service scenario"
require_literal "$matrix" 'make x86_64-microkernel-nontls-network-service-matrix' \
  "validation matrix runner must dispatch the non-TLS Make target"
require_literal "$matrix" 'x86_64 microkernel non-TLS network service matrix passed.' \
  "validation matrix runner must require the non-TLS positive marker"
require_literal "$matrix" 'network host log|network pcap|serial log|qemu log' \
  "validation matrix runner must require host, pcap, serial, and QEMU artifacts"
require_literal "$makefile" 'x86_64-microkernel-nontls-network-service-contract:' \
  "Makefile must expose the non-TLS network service contract"
require_literal "$makefile" 'x86_64-microkernel-nontls-network-service-matrix:' \
  "Makefile must expose the non-TLS network service matrix"

for forbidden in \
  ETHER_TYPE_ARP \
  ETHER_TYPE_IPV4 \
  IPV4_PROTOCOL_ICMP \
  IPV4_PROTOCOL_UDP \
  IPV4_PROTOCOL_TCP \
  DHCP \
  classify_; do
  reject_impl_literal "$virtio_net" "NetDriverTask" "$forbidden" \
    "NetDriverTask must not grow protocol parsing for Phase 7: $forbidden"
done

for file in "$runner" "$matrix" "$makefile"; do
  reject_regex "$file" 'DP_MICROKERNEL_TLS_PROOF=|mode="?--tls-proof"?' \
    "non-TLS network work must not add executable TLS proof-mode wiring"
  reject_regex "$file" 'https://|curl[[:space:]]+-k|openssl|rustls|embedded-tls|webpki|ring::|aws-lc-rs' \
    "non-TLS network work must not add TLS/HTTPS dependencies or commands"
  reject_regex "$file" 'generic[[:space:]]+socket[[:space:]]+api|socket[[:space:]]+API|listen[[:space:]]+socket|accept[[:space:]]+socket' \
    "non-TLS network work must not introduce a generic socket API"
  reject_regex "$file" 'STRICT_FIVE_CALIBRATION|calibration|benchmark[[:space:]]+tuning' \
    "non-TLS network work must not tune benchmarks"
done

echo "x86_64 microkernel non-TLS network service contract guard passed."
