#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "${BASH_SOURCE[0]}")/.."

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
kernel="crates/dataplane-x86_64-microkernel-smoke/src/kernel.rs"
network_task="crates/dataplane-x86_64-microkernel-smoke/src/network_task.rs"
tcp_stream="crates/dataplane-x86_64-microkernel-smoke/src/tcp_stream.rs"
scenarios="crates/dataplane-x86_64-microkernel-smoke/src/scenarios.rs"
http="crates/dataplane-x86_64-microkernel-smoke/src/http.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
makefile="Makefile"
plan="aidocs/055_microkernel_tls_deferred_robust_appliance_plan_2026-05-31.md"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_file() {
  [[ -f "$1" ]] || fail "required file missing: $1"
}

require_literal() {
  local file="$1"
  local literal="$2"
  local note="$3"
  local status
  set +e
  rg -q --fixed-strings -- "$literal" "$file"
  status="$?"
  set -e
  if [[ "$status" -ne 0 ]]; then
    [[ "$status" -eq 1 ]] || fail "rg failed while scanning $file for required literal: $literal"
    fail "$note"
  fi
}

reject_regex() {
  local file="$1"
  local regex="$2"
  local note="$3"
  local status
  set +e
  rg -q --pcre2 -- "$regex" "$file"
  status="$?"
  set -e
  if [[ "$status" -eq 0 ]]; then
    fail "$note"
  fi
  [[ "$status" -eq 1 ]] || fail "rg failed while scanning $file for forbidden regex: $regex"
}

for path in "$main" "$kernel" "$network_task" "$tcp_stream" "$scenarios" "$http" "$runner" "$makefile" "$plan"; do
  require_file "$path"
done

require_literal "$plan" "x86_64-microkernel-network-counter-audit" \
  "TLS-deferred plan must name the network counter audit packet"
require_literal "$plan" "Expose only counters with a clear owner, update site, reset policy" \
  "plan must require counter ownership before exposure"

require_literal "$tcp_stream" "struct TcpControlCounters" \
  "guest must keep TCP counters source-owned"
require_literal "$network_task" "struct NetworkDropCounters" \
  "guest must keep network drop counters source-owned"
require_literal "$scenarios" "fn run_network_counter_audit_probe" \
  "guest must include a focused network counter audit probe"
require_literal "$scenarios" "DPMK:NETWORK-COUNTER-AUDIT-DROP-OWNER:TcpIpTask.network_drop_counters" \
  "audit must identify the drop counter owner"
require_literal "$scenarios" "DPMK:NETWORK-COUNTER-AUDIT-TCP-OWNER:TcpIpTask.tcp_counters" \
  "audit must identify the TCP counter owner"
require_literal "$scenarios" "DPMK:NETWORK-COUNTER-AUDIT-MARKER-ONLY-REJECTED" \
  "audit must reject marker-only success"
require_literal "$scenarios" "DPMK:NETWORK-COUNTER-AUDIT-OK" \
  "audit must emit a final success marker"

for literal in \
  "DPSTATUS:NET-COUNTERS ready=" \
  "DPSTATUS:TCP-COUNTERS accepted=" \
  "DPSTATUS:NET-DROPS arp_wrong_target=" \
  "udp_wrong_port=" \
  "udp_bad=" \
  "tcp_wrong_port=" \
  "tcp_malformed=" \
  "fin_during_response="; do
  require_literal "$scenarios" "$literal" "status output must expose bounded network counter field: $literal"
done

for literal in \
  "response_404=" \
  "response_405=" \
  "response_413=" \
  "headers_too_long=" \
  "stale_requests=" \
  "malformed_replies=" \
  "recovery_sends="; do
  require_literal "$scenarios" "$literal" "audit must expose already-owned status counter field: $literal"
done

for literal in \
  "DPCLI:NET-COUNTERS active=" \
  "DPMK:CLI-NET-COUNTERS-OK" \
  "DPMK:NET-DROP-POLICY-OK" \
  "DPMK:TCP-CTRL-OK" \
  "DPMK:NONTLS-NEG-COUNTERS-OK" \
  "DPMK:NETWORK-CONTROL-PLANE-SLICE-OK"; do
  require_literal "$scenarios" "$literal" "audit depends on existing source-owned evidence: $literal"
done

require_literal "$kernel" "kernel.run_network_counter_audit_probe()" \
  "kernel must execute the focused network counter audit probe"
require_literal "$http" "pub(crate) struct HttpPolicyCounters" \
  "HTTP status counters must remain source-owned by the HTTP task"

require_literal "$runner" 'network_counter_audit_summary="$log_dir/x86_64-microkernel-fat32-$run_id.network-counter-audit.summary"' \
  "runner must define a focused network counter audit summary artifact"
require_literal "$runner" 'DP_MICROKERNEL_NETWORK_COUNTER_AUDIT_PROOF' \
  "runner must expose the network counter audit env proof switch"
require_literal "$runner" '--network-counter-audit-proof' \
  "runner must expose the network counter audit proof mode"
require_literal "$runner" 'DP_NETWORK_COUNTER_AUDIT_PROOF' \
  "serial verifier must receive the network counter audit proof flag"
require_literal "$runner" 'network_counter_audit_checks = {' \
  "serial verifier must compute explicit audit checks"
require_literal "$runner" 'network_counter_audit_pcap_ok=true' \
  "runner must require pcap-backed network evidence"
require_literal "$runner" 'network_counter_audit_no_stale_artifacts_ok=true' \
  "runner must record stale artifact rejection"
require_literal "$runner" 'network_counter_audit_no_benchmark_retune_ok=true' \
  "runner must record no benchmark retuning"
require_literal "$runner" 'network_counter_audit_network_pcap=$net_pcap' \
  "summary must link the pcap artifact"

require_literal "$makefile" "x86_64-microkernel-network-counter-audit-contract:" \
  "Makefile must expose the audit contract target"
require_literal "$makefile" "DP_MICROKERNEL_NETWORK_COUNTER_AUDIT_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --network-counter-audit-proof" \
  "Makefile must run the focused proof mode"

reject_regex "$main" 'https://|curl[[:space:]]+-k|rustls|embedded[-_]tls|webpki|OpenSSL|certificate|private[[:space:]]+key|DP_MICROKERNEL_TLS_PROOF=|mode="?--tls-proof"?' \
  "network counter audit must not add TLS/HTTPS/certificate wiring"
reject_regex "$kernel" 'Dns|DNS|Ntp|NTP|mDNS|service discovery|generic socket|SocketApi|production TCP/IP|hardware readiness' \
  "network counter audit must not add service discovery, generic sockets, or overclaims"
reject_regex "$runner" 'STRICT_FIVE_CALIBRATION|benchmark[[:space:]]+tuning|calibration' \
  "network counter audit runner must not retune benchmarks"

echo "x86_64 microkernel network counter audit contract passed."
