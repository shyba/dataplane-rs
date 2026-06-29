#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
matrix_runner="tools/x86_64_microkernel_validation_matrix_run.sh"
matrix_guard="tools/check_x86_64_microkernel_validation_matrix_contract.sh"
makefile="Makefile"
plan="aidocs/051_microkernel_pre_tls_appliance_plan_2026-05-30.md"
frontier="aidocs/050_microkernel_robust_design_frontier_2026-05-30.md"

require_file() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    echo "missing required file: $path" >&2
    exit 1
  fi
}

require_literal() {
  local path="$1"
  local literal="$2"
  local message="$3"
  if ! rg -Fq -- "$literal" "$path"; then
    echo "$message" >&2
    echo "missing literal in $path: $literal" >&2
    exit 1
  fi
}

reject_regex() {
  local path="$1"
  local regex="$2"
  local message="$3"
  local tmp
  tmp="$(mktemp)"
  set +e
  rg -n -- "$regex" "$path" >"$tmp" 2>&1
  local status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    echo "$message" >&2
    cat "$tmp" >&2
    rm -f "$tmp"
    exit 1
  fi
  if [[ "$status" -ne 1 ]]; then
    echo "rg failed while scanning $path for $regex" >&2
    cat "$tmp" >&2
    rm -f "$tmp"
    exit 1
  fi
  rm -f "$tmp"
}

echo "=== x86_64 Microkernel Operator Appliance Consolidation Contract Guard ==="

for path in \
  "$main" \
  "$runner" \
  "$matrix_runner" \
  "$matrix_guard" \
  "$makefile" \
  "$plan" \
  "$frontier"; do
  require_file "$path"
done

require_literal "$plan" "x86_64-microkernel-operator-appliance-consolidation" \
  "pre-TLS plan must route the active operator appliance consolidation packet"
require_literal "$frontier" "x86_64-microkernel-operator-appliance-consolidation" \
  "frontier must route the active operator appliance consolidation packet"

for literal in \
  "operator-appliance-consolidation" \
  "make x86_64-microkernel-operator-appliance-consolidation" \
  "x86_64 microkernel operator appliance consolidation proof passed." \
  "operator appliance consolidation summary|client log|serial log|qemu log|network pcap"; do
  require_literal "$matrix_runner" "$literal" \
    "validation matrix must include operator appliance consolidation literal: $literal"
  require_literal "$matrix_guard" "$literal" \
    "validation matrix guard must preserve operator appliance consolidation literal: $literal"
done

require_literal "$makefile" "x86_64-microkernel-operator-appliance-consolidation-contract:" \
  "Makefile must expose the operator appliance consolidation contract"
require_literal "$makefile" "./tools/check_x86_64_microkernel_operator_appliance_consolidation_contract.sh" \
  "Makefile contract target must run this guard"
require_literal "$makefile" "DP_MICROKERNEL_OPERATOR_APPLIANCE_CONSOLIDATION_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --operator-appliance-consolidation-proof" \
  "Makefile proof target must run the focused operator appliance mode"

for guard in \
  "tools/check_x86_64_microkernel_cli_operator_contract.sh" \
  "tools/check_x86_64_microkernel_status_snapshot_consistency_contract.sh" \
  "tools/check_x86_64_microkernel_http_policy_matrix_contract.sh" \
  "tools/check_x86_64_microkernel_filesystem_read_matrix_contract.sh" \
  "tools/check_x86_64_microkernel_network_control_plane_slice_contract.sh" \
  "tools/check_x86_64_microkernel_storage_service_counters_contract.sh" \
  "tools/check_x86_64_microkernel_operator_recovery_runbook_contract.sh"; do
  require_file "$guard"
done

require_literal "$makefile" "x86_64-microkernel-fat32-contract" \
  "operator appliance proof must preserve the base FAT32/operator guard dependency"

for path in "$main" "$runner" "$matrix_runner"; do
  reject_regex "$path" 'operator[_-]appliance.*(TLS|HTTPS|rustls|embedded-tls|webpki|openssl|crypto-provider)' \
    "operator appliance consolidation must not add TLS/HTTPS/crypto scope in $path"
  reject_regex "$path" 'operator[_-]appliance.*(generic socket|GenericSocket|SocketApi|BSD socket)' \
    "operator appliance consolidation must not add a generic socket API in $path"
  reject_regex "$path" 'operator[_-]appliance.*(debugger|raw memory|MMIO|page-table|page table|restart|reset|JSON|json)' \
    "operator appliance consolidation must not add debugger, raw inspection, restart/reset, or JSON scope in $path"
  reject_regex "$path" 'operator[_-]appliance.*(crash consistency|CrashConsistency|full TCP compliance|tcp compliance)' \
    "operator appliance consolidation must not make storage durability or TCP compliance claims in $path"
done

echo "x86_64 microkernel operator appliance consolidation contract guard passed."
