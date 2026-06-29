#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
makefile="Makefile"
frontier="aidocs/050_microkernel_robust_design_frontier_2026-05-30.md"
plan="aidocs/055_microkernel_tls_deferred_robust_appliance_plan_2026-05-31.md"

require_literal() {
  local file="$1"
  local literal="$2"
  local message="$3"
  if ! grep -Fq -- "$literal" "$file"; then
    echo "FAIL: $message"
    echo "missing literal in $file: $literal"
    exit 1
  fi
}

reject_regex() {
  local file="$1"
  local regex="$2"
  local message="$3"
  local matches
  set +e
  matches="$(rg -n "$regex" "$file")"
  local status=$?
  set -e
  if [[ $status -eq 0 ]]; then
    echo "$matches"
    echo "FAIL: $message"
    exit 1
  fi
  if [[ $status -ne 1 ]]; then
    echo "FAIL: regex check failed for $file"
    exit 1
  fi
}

echo "=== x86_64 Microkernel Non-TLS Control Service Frontier Contract Guard ==="

for file in "$main" "$runner" "$makefile" "$frontier" "$plan"; do
  [[ -f "$file" ]] || {
    echo "FAIL: required file missing: $file"
    exit 1
  }
done

require_literal "$main" 'DPMK:NONTLS-CTRL-STATUS-NETREADY-OK' \
  "guest must prove CLI/HTTP status net_ready and NET-COUNTERS readiness agree"
require_literal "$main" 'DPMK:NONTLS-CONTROL-SERVICE-FRONTIER-OK' \
  "guest must emit focused non-TLS control-service completion marker"
require_literal "$main" 'DPSTATUS:NET-COUNTERS ready=' \
  "status body must include bounded network counter readiness"
require_literal "$main" 'DPSTATUS:ROUTE-TABLE routes=' \
  "status body must expose bounded route-table fields"
require_literal "$main" 'DPMK:SERVICE-ROUTE-TABLE-OK' \
  "route-table service proof must remain present"
require_literal "$main" 'DPMK:NETWORK-CONTROL-PLANE-SLICE-OK' \
  "frontier must build on existing non-TLS control-plane evidence"

for literal in \
  'DP_MICROKERNEL_NONTLS_CONTROL_SERVICE_FRONTIER_PROOF' \
  'DP_NONTLS_CONTROL_SERVICE_FRONTIER_PROOF' \
  '--nontls-control-service-frontier-proof' \
  'nontls_control_service_frontier_summary=' \
  'nontls_control_service_frontier_summary_status=pass' \
  'nontls_control_service_frontier_mode=bounded-nontls-status-route-control-service' \
  'nontls_control_service_frontier_tls_deferred=true' \
  'nontls_control_service_frontier_status_crosscheck_ok=true' \
  'nontls_control_service_frontier_route_table_ok=true' \
  'nontls_control_service_frontier_host_pcap_ok=true' \
  'nontls_control_service_frontier_no_benchmark_retune_ok=true' \
  'non-TLS control service frontier summary: '; do
  require_literal "$runner" "$literal" "runner must include $literal"
done

require_literal "$makefile" 'x86_64-microkernel-nontls-control-service-frontier-contract:' \
  "Makefile must expose focused contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_nontls_control_service_frontier_contract.sh' \
  "Makefile must run this guard"
require_literal "$makefile" 'x86_64-microkernel-nontls-control-service-frontier: guard-scripts-executable' \
  "Makefile must expose focused proof target"
require_literal "$makefile" 'DP_MICROKERNEL_NONTLS_CONTROL_SERVICE_FRONTIER_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --nontls-control-service-frontier-proof' \
  "Makefile proof target must run exact env/mode"

require_literal "$frontier" 'x86_64-microkernel-nontls-control-service-frontier' \
  "frontier plan must route the current packet"
require_literal "$plan" 'x86_64-microkernel-nontls-control-service-frontier' \
  "TLS-deferred plan must name the current packet"
require_literal "$plan" 'TLS is deferred.' \
  "TLS deferral must stay explicit"

reject_regex "$main" '(rustls|embedded-tls|webpki|ring::|aws-lc-rs|openssl|OpenSSL|crypto-provider|certificate|entropy|https://|HTTPS)' \
  "guest code must not introduce TLS/HTTPS/certificate/entropy drift"
reject_regex "$main" '(Dns|DNS|Ntp|NTP|mDNS|service discovery|GenericSocket|SocketApi|DebuggerShell|restart command|reset command)' \
  "guest code must not broaden scope into DNS/NTP/service discovery/sockets/debugger/restart"
reject_regex "$runner" 'nontls_control_service_frontier_.*(benchmark retun|strict-five|calibration)' \
  "frontier runner summary must not substitute benchmark or calibration evidence"

echo "x86_64 microkernel non-TLS control service frontier contract guard passed."
