#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

runner="tools/x86_64_microkernel_fat32_run.sh"
helper="tools/x86_64_microkernel_validation_matrix_lib.sh"
makefile="Makefile"
plan="changes/x86_64-microkernel-appliance-consolidation/proposal.md"
tasks="changes/x86_64-microkernel-appliance-consolidation/tasks.md"
spec="changes/x86_64-microkernel-appliance-consolidation/specs/appliance-consolidation/spec.md"

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
  local status
  set +e
  rg -Fq -- "$literal" "$path"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    return 0
  fi
  if [[ "$status" -eq 1 ]]; then
    echo "$message" >&2
    echo "missing literal in $path: $literal" >&2
    exit 1
  fi
  echo "rg failed while scanning $path for required literal: $literal" >&2
  exit 1
}

reject_regex() {
  local path="$1"
  local regex="$2"
  local message="$3"
  local tmp status
  tmp="$(mktemp)"
  set +e
  rg -n -- "$regex" "$path" >"$tmp" 2>&1
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    echo "$message" >&2
    cat "$tmp" >&2
    rm -f "$tmp"
    exit 1
  fi
  if [[ "$status" -ne 1 ]]; then
    echo "rg failed while scanning $path for forbidden regex: $regex" >&2
    cat "$tmp" >&2
    rm -f "$tmp"
    exit 1
  fi
  rm -f "$tmp"
}

extract_make_target_body() {
  local target="$1"
  awk -v target="^${target}:" '
    $0 ~ target { flag=1; print; next }
    flag && $0 ~ /^[^[:space:]].*:/ { exit }
    flag { print }
  ' "$makefile"
}

echo "=== x86_64 Microkernel Appliance Consolidation Contract Guard ==="

for path in "$runner" "$helper" "$makefile" "$plan" "$tasks" "$spec"; do
  require_file "$path"
done

require_literal "$plan" "applies the already-proven helper pattern to the FAT32/appliance runner path" \
  "proposal must state the appliance consolidation scope"
require_literal "$tasks" "summary path construction" \
  "tasks must require summary path construction reuse"
require_literal "$tasks" "duplicate-key validation" \
  "tasks must require duplicate-key validation reuse"
require_literal "$tasks" "run-id/log-root reuse" \
  "tasks must require run-id/log-root reuse"
require_literal "$tasks" "labeled summary/artifact emission" \
  "tasks must require labeled summary/artifact emission"

require_literal "$helper" "dp_default_log_root()" \
  "shared helper must own default log-root resolution"
require_literal "$helper" "dp_new_run_id()" \
  "shared helper must own run-id generation"
require_literal "$helper" "dp_artifact_path()" \
  "shared helper must own artifact path construction"
require_literal "$helper" "dp_emit_artifact_line()" \
  "shared helper must own labeled artifact output"
require_literal "$helper" "dp_reject_duplicate_keys()" \
  "shared helper must own duplicate-key validation"

make_target_body="$(
  extract_make_target_body "x86_64-microkernel-appliance-consolidation"
)"

if [[ -z "$make_target_body" ]]; then
  echo "missing appliance consolidation target body in Makefile" >&2
  exit 1
fi

if ! grep -Fq $'x86_64-microkernel-appliance-consolidation: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-appliance-consolidation-contract\n\tDP_MICROKERNEL_STORAGE_SERVICE_COUNTERS_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --storage-service-counters-proof' <<<"$make_target_body"; then
  echo "appliance consolidation Makefile target body is not the expected direct runner invocation" >&2
  printf '%s\n' "$make_target_body" >&2
  exit 1
fi

if grep -Eq 'x86_64-microkernel-operator-appliance-consolidation|--operator-appliance-consolidation-proof' <<<"$make_target_body"; then
  echo "appliance consolidation Makefile target still depends on stale operator consolidation plumbing" >&2
  printf '%s\n' "$make_target_body" >&2
  exit 1
fi

require_literal "$runner" 'source "tools/x86_64_microkernel_validation_matrix_lib.sh"' \
  "runner must source the shared helper library"
require_literal "$runner" 'log_root="$(dp_default_log_root DP_MICROKERNEL_FAT32_LOG_ROOT)"' \
  "runner must use shared default log-root resolution"
require_literal "$runner" 'run_id="$(dp_new_run_id)"' \
  "runner must use shared run-id generation"
require_literal "$runner" 'operator_appliance_surface_summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "operator-appliance-surface.summary")"' \
  "runner must use shared artifact path construction for operator appliance summary"
require_literal "$runner" 'operator_appliance_surface_source_scan="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "operator-appliance-surface.source-scan.txt")"' \
  "runner must use shared artifact path construction for operator appliance source scan"
require_literal "$runner" 'storage_service_counters_artifacts=summary,source_scan' \
  "runner must keep the storage service counter artifact labels explicit"
require_literal "$runner" 'if ! dp_reject_duplicate_keys "$operator_appliance_surface_summary"; then' \
  "runner must fail closed on duplicate operator appliance summary keys"
require_literal "$runner" 'if ! dp_reject_duplicate_keys "$storage_summary"; then' \
  "runner must fail closed on duplicate storage summary keys"
require_literal "$runner" 'dp_emit_artifact_line "operator appliance surface summary" "$operator_appliance_surface_summary"' \
  "runner must emit labeled operator appliance summary artifacts"
require_literal "$runner" 'dp_emit_artifact_line "operator appliance surface source scan" "$operator_appliance_surface_source_scan"' \
  "runner must emit labeled operator appliance source scan artifacts"
require_literal "$runner" 'dp_emit_artifact_line "storage service counters summary" "$storage_summary"' \
  "runner must emit labeled storage summary artifacts"
require_literal "$runner" 'dp_emit_artifact_line "storage service counters source scan" "$storage_source_scan"' \
  "runner must emit labeled storage source scan artifacts"
require_literal "$runner" 'operator_appliance_surface_artifacts=summary,source_scan' \
  "runner must preserve operator appliance artifact labels"
require_literal "$runner" 'storage_service_counters_proof_kind=source_scan' \
  "runner must preserve storage service counters proof kind"
require_literal "$runner" 'storage_service_counters_read_only=true' \
  "runner must preserve storage service counters read-only claim"
require_literal "$runner" 'if [[ "$mode" == "--storage-service-counters-proof" ]]; then' \
  "runner must keep storage service counters on its own explicit branch"

reject_regex "$runner" 'scenario_command|scenario_marker|scenario_artifact_labels|scenario_summary|generic[[:space:]]+artifact[[:space:]]+helper' \
  "runner must not introduce a broad scenario abstraction"
reject_regex "$runner" 'HTTPS|generic[[:space:]]+socket' \
  "runner must not broaden scope into unrelated TLS/socket/benchmark concerns"
reject_regex "$runner" 'storage_service_counters_[a-z_]*="\$log_dir/x86_64-microkernel-fat32-\$run_id\.storage-service-counters\.(source-scan\.txt|summary)"' \
  "runner must not hand-build storage service counters artifact paths"

require_literal "$makefile" "x86_64-microkernel-appliance-consolidation-contract:" \
  "Makefile must expose the appliance consolidation contract target"
require_literal "$makefile" "./tools/check_x86_64_microkernel_appliance_consolidation_contract.sh" \
  "Makefile contract target must run the appliance consolidation guard"
require_literal "$makefile" "x86_64-microkernel-appliance-consolidation: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-appliance-consolidation-contract" \
  "Makefile must expose the appliance consolidation proof target"
require_literal "$makefile" "DP_MICROKERNEL_STORAGE_SERVICE_COUNTERS_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --storage-service-counters-proof" \
  "Makefile proof target must keep the storage service counters proof mode"

echo "x86_64 microkernel appliance consolidation contract guard passed."
