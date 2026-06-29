#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

runner="tools/x86_64_microkernel_appliance_load_fairness_pack.sh"
matrix_runner="tools/x86_64_microkernel_validation_matrix_run.sh"
makefile="Makefile"
spec="changes/x86_64-microkernel-appliance-load-fairness-pack/specs/appliance-load-fairness-pack/spec.md"
tasks="changes/x86_64-microkernel-appliance-load-fairness-pack/tasks.md"
http_guard="tools/check_x86_64_microkernel_http_policy_matrix_contract.sh"
cli_guard="tools/check_x86_64_microkernel_cli_http_operator_parity_contract.sh"
timer_guard="tools/check_x86_64_microkernel_timer_timeout_service_contract.sh"
fault_guard="tools/check_x86_64_microkernel_fault_policy_hardening_contract.sh"
network_guard="tools/check_x86_64_microkernel_nontls_network_service_contract.sh"
scheduler_guard="tools/check_x86_64_microkernel_scheduler_fairness_load_contract.sh"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_file() {
  local path="$1"
  [[ -f "$path" ]] || fail "required file missing: $path"
}

require_literal() {
  local path="$1"
  local literal="$2"
  local note="$3"
  rg -q --fixed-strings -- "$literal" "$path" || fail "$note"
}

reject_regex() {
  local path="$1"
  local regex="$2"
  local note="$3"
  local matches status
  set +e
  matches="$(rg -n -- "$regex" "$path")"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    printf '%s\n' "$matches"
    fail "$note"
  fi
  [[ "$status" -eq 1 ]] || fail "could not scan $path for forbidden regex: $regex"
}

for path in "$runner" "$matrix_runner" "$makefile" "$spec" "$tasks" "$http_guard" "$cli_guard" "$timer_guard" "$fault_guard" "$network_guard" "$scheduler_guard"; do
  require_file "$path"
done

echo "=== x86_64 Microkernel Appliance Load Fairness Pack Contract Guard ==="

for literal in \
  'http-heavy' \
  'cli-heavy' \
  'fat32-heavy' \
  'network-heavy' \
  'timer-heavy' \
  'contained-fault' \
  'appliance_load_fairness_http_heavy_summary=' \
  'appliance_load_fairness_cli_heavy_summary=' \
  'appliance_load_fairness_fat32_heavy_summary=' \
  'appliance_load_fairness_network_heavy_summary=' \
  'appliance_load_fairness_timer_heavy_summary=' \
  'appliance_load_fairness_contained_fault_summary=' \
  'appliance_load_fairness_summary_status=pass' \
  'appliance load fairness pack summary: ' \
  'x86_64 microkernel appliance load fairness pack passed.'; do
  require_literal "$runner" "$literal" "runner must expose appliance load fairness literal: $literal"
done

for literal in \
  'dp_emit_artifact_line "http-heavy summary" "$http_curl_log"' \
  'dp_emit_artifact_line "cli-heavy summary" "$cli_summary"' \
  'dp_emit_artifact_line "fat32-heavy summary" "$fat32_summary"' \
  'dp_emit_artifact_line "network-heavy summary" "$network_summary"' \
  'dp_emit_artifact_line "timer-heavy summary" "$timer_summary"' \
  'dp_emit_artifact_line "contained-fault summary" "$fault_summary"'; do
  require_literal "$runner" "$literal" "runner must expose per-scenario live evidence labels: $literal"
done

require_literal "$makefile" 'x86_64-microkernel-appliance-load-fairness-pack-contract:' \
  "Makefile must expose the appliance load fairness pack contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_appliance_load_fairness_pack_contract.sh' \
  "Makefile must run the packet-specific contract guard"
require_literal "$makefile" 'x86_64-microkernel-appliance-load-fairness-pack: guard-scripts-executable x86_64-microkernel-appliance-load-fairness-pack-contract' \
  "Makefile must expose the packet proof target"
require_literal "$makefile" './tools/x86_64_microkernel_appliance_load_fairness_pack.sh' \
  "Makefile must run the packet-specific wrapper"
require_literal "$matrix_runner" 'appliance-load-fairness-pack' \
  "validation matrix must know the appliance load fairness pack scenario"
require_literal "$matrix_runner" "appliance-load-fairness-pack) echo 'make x86_64-microkernel-appliance-load-fairness-pack' ;;" \
  "validation matrix must dispatch the packet scenario through Make"
require_literal "$matrix_runner" "appliance-load-fairness-pack) echo 'x86_64 microkernel appliance load fairness pack passed.' ;;" \
  "validation matrix must require the packet success marker"
require_literal "$matrix_runner" "appliance-load-fairness-pack) echo 'appliance load fairness pack summary|http-heavy summary|cli-heavy summary|fat32-heavy summary|network-heavy summary|timer-heavy summary|contained-fault summary' ;;" \
  "validation matrix must require packet artifacts"

reject_regex "$runner" 'HTTPS|https://|cert|certificate|entropy|OpenSSL|openssl|curl[[:space:]]+-k|rustls|embedded-tls|webpki|ring::|aws-lc-rs' \
  "packet wrapper must not introduce TLS/HTTPS drift"
reject_regex "$runner" 'generic[[:space:]]+(IPC|ipc|socket)|generic[_-](IPC|ipc|socket)|socket abstraction' \
  "packet wrapper must not introduce generic IPC/socket scope"
reject_regex "$runner" 'benchmark[[:space:]]+tuning|retune[[:space:]]+benchmarks|retuning[[:space:]]+benchmarks|STRICT_FIVE_CALIBRATION' \
  "packet wrapper must not retune benchmarks"

echo "x86_64 microkernel appliance load fairness pack contract guard passed."
