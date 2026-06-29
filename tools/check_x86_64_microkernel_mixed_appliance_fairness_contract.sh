#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
matrix_runner="tools/x86_64_microkernel_validation_matrix_run.sh"
matrix_guard="tools/check_x86_64_microkernel_validation_matrix_contract.sh"
makefile="Makefile"
fat32_guard="tools/check_x86_64_microkernel_fat32_contract.sh"
operator_guard="tools/check_x86_64_microkernel_cli_operator_contract.sh"
nontls_guard="tools/check_x86_64_microkernel_nontls_network_service_contract.sh"
service_ipc_guard="tools/check_x86_64_microkernel_service_ipc_audit_contract.sh"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_file() {
  local file="$1"
  [[ -f "$file" ]] || fail "required file missing: $file"
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

for file in \
  "$main" \
  "$runner" \
  "$matrix_runner" \
  "$matrix_guard" \
  "$makefile" \
  "$fat32_guard" \
  "$operator_guard" \
  "$nontls_guard" \
  "$service_ipc_guard"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel Mixed Appliance Fairness Contract Guard ==="

for marker in \
  'DPMK:MIXED-APPLIANCE-HTTP-STATUS-OK:' \
  'DPMK:MIXED-APPLIANCE-CLI-STATUS-OK:' \
  'DPMK:MIXED-APPLIANCE-FAT32-OK' \
  'DPMK:MIXED-APPLIANCE-NONTLS-NETWORK-OK:' \
  'DPMK:MIXED-APPLIANCE-TIMER-OK:' \
  'DPMK:MIXED-APPLIANCE-TIMER-MAXGAP:' \
  'DPMK:MIXED-APPLIANCE-CONTAINED-FAULT-OK' \
  'DPMK:MIXED-APPLIANCE-OK'; do
  require_literal "$main" "$marker" "guest must emit mixed appliance fairness marker $marker"
done

require_literal "$main" 'fn run_mixed_appliance_fairness_probe(&mut self)' \
  "guest must keep a focused mixed appliance fairness workload"
require_literal "$main" 'serial::write_str("DPMK:MIXED-APPLIANCE-TIMER-MAXGAP:");' \
  "guest must emit a measurable mixed appliance fairness timer max-gap"

for marker in \
  'DPMK:HTTP-GET-OK' \
  'DPMK:CLI-COMMANDS-OK' \
  'DPMK:FS-INDEX-OK' \
  'DPMK:NETWORK-CONTROL-PLANE-SLICE-OK' \
  'DPMK:NET-TIMER-MAXGAP:' \
  'DPMK:FAULT-CONTAINED'; do
  require_literal "$runner" "$marker" "runner must require host-visible mixed appliance progress marker $marker"
done

for literal in \
  'DP_MICROKERNEL_MIXED_APPLIANCE_FAIRNESS_PROOF' \
  '--mixed-appliance-fairness-proof' \
  'mixed_appliance_fairness_summary=' \
  'mixed_appliance_fairness_mode=' \
  'mixed_appliance_fairness_summary_status=pass' \
  'mixed_appliance_fairness_scheduler_source=' \
  'mixed_appliance_fairness_http_ok=true' \
  'mixed_appliance_fairness_cli_ok=true' \
  'mixed_appliance_fairness_fat32_ok=true' \
  'mixed_appliance_fairness_network_ok=true' \
  'mixed_appliance_fairness_nontls_network_ok=true' \
  'mixed_appliance_fairness_timer_ok=true' \
  'mixed_appliance_fairness_timer_bound_ok=true' \
  'mixed_appliance_fairness_fault_contained=true' \
  'mixed_appliance_fairness_containment_ok=true' \
  'mixed_appliance_fairness_status_ok=true' \
  'mixed_appliance_fairness_fixed_workload_contract=true' \
  'mixed_appliance_fairness_tls_deferred=true' \
  'mixed appliance fairness summary: ' \
  'mixed appliance fairness pcap: ' \
  'x86_64 microkernel mixed appliance fairness proof passed.'; do
  require_literal "$runner" "$literal" "runner must expose mixed appliance fairness evidence literal: $literal"
done

require_literal "$runner" 'mixed_appliance_fairness_summary="$log_dir/x86_64-microkernel-fat32-$run_id.mixed-appliance-fairness.summary"' \
  "mixed appliance fairness summary must stay under the configured log directory"

require_literal "$makefile" 'x86_64-microkernel-mixed-appliance-fairness-contract:' \
  "Makefile must expose the mixed appliance fairness contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_mixed_appliance_fairness_contract.sh' \
  "Makefile mixed appliance fairness contract must run this guard"
require_literal "$makefile" 'x86_64-microkernel-mixed-appliance-fairness: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-nontls-network-service-contract x86_64-microkernel-service-ipc-audit-contract x86_64-microkernel-scheduler-fairness-load-contract x86_64-microkernel-filesystem-read-matrix-contract x86_64-microkernel-network-control-plane-slice-contract x86_64-microkernel-fault-and-recovery-preconditions-contract x86_64-microkernel-mixed-appliance-fairness-contract' \
  "Makefile mixed appliance fairness target must preserve neighboring contract coverage"
require_literal "$makefile" 'DP_MICROKERNEL_MIXED_APPLIANCE_FAIRNESS_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --mixed-appliance-fairness-proof' \
  "Makefile mixed appliance fairness target must run the focused proof mode"

require_literal "$matrix_runner" 'mixed-appliance-fairness' \
  "validation matrix must know the mixed appliance fairness scenario"
require_literal "$matrix_runner" "mixed-appliance-fairness) echo 'make x86_64-microkernel-mixed-appliance-fairness' ;;" \
  "validation matrix must dispatch mixed appliance fairness through Make"
require_literal "$matrix_runner" "mixed-appliance-fairness) echo 'x86_64 microkernel mixed appliance fairness proof passed.' ;;" \
  "validation matrix must require the mixed appliance fairness positive marker"
require_literal "$matrix_runner" "mixed-appliance-fairness) echo 'mixed appliance fairness summary|client log|network host log|mixed appliance fairness pcap|serial log|qemu log' ;;" \
  "validation matrix must require mixed appliance fairness artifacts"
require_literal "$matrix_guard" 'mixed-appliance-fairness' \
  "validation matrix guard must cover mixed appliance fairness"

for scanned in "$runner" "$matrix_runner" "$makefile"; do
  reject_regex "$scanned" 'mixed[-_ ]appliance.*(HTTPS|https://|cert|certificate|entropy|OpenSSL|openssl|curl[[:space:]]+-k|rustls|embedded-tls|webpki|ring::|aws-lc-rs|TLS[^_[:alnum:]-])' \
    "mixed appliance fairness must not introduce TLS/HTTPS/certificate/entropy drift in $scanned"
  reject_regex "$scanned" 'mixed[-_ ]appliance.*(generic[[:space:]]+(IPC|ipc|socket)|generic[_-](IPC|ipc|socket)|socket abstraction)' \
    "mixed appliance fairness must not introduce generic IPC/socket scope in $scanned"
  reject_regex "$scanned" 'mixed[-_ ]appliance.*(restart command|reset command|reset task|replay claim)' \
    "mixed appliance fairness must not claim restart/reset/replay in $scanned"
  reject_regex "$scanned" 'mixed[-_ ]appliance.*(debugger|raw[[:space:]]+memory|MMIO|mmio|page-table|pagetable|shell)' \
    "mixed appliance fairness must not require debugger/raw memory/MMIO/page-table/shell evidence in $scanned"
  reject_regex "$scanned" 'mixed[-_ ]appliance.*(STRICT_FIVE_CALIBRATION|benchmark[[:space:]]+tuning|retune[[:space:]]+benchmarks|retuning[[:space:]]+benchmarks|calibration)' \
    "mixed appliance fairness must not retune benchmarks in $scanned"
  reject_regex "$scanned" 'mixed[-_ ]appliance.*(hardware-ready|hardware readiness|hardware[[:space:]]+ready|real[[:space:]]+hardware)' \
    "mixed appliance fairness must not claim hardware readiness in $scanned"
done

echo "x86_64 microkernel mixed appliance fairness contract guard passed."
