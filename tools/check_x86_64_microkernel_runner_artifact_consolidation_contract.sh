#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

plan="aidocs/051_microkernel_pre_tls_appliance_plan_2026-05-30.md"
diary="aidocs/050_microkernel_robust_design_diary_2026-05-30.md"
runner="tools/x86_64_microkernel_validation_matrix_run.sh"
failure_runner="tools/x86_64_microkernel_validation_failure_injection.sh"
helper="tools/x86_64_microkernel_validation_matrix_lib.sh"
matrix_guard="tools/check_x86_64_microkernel_validation_matrix_contract.sh"
failure_guard="tools/check_x86_64_microkernel_validation_failure_injection_contract.sh"
makefile="Makefile"

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

echo "=== x86_64 Microkernel Runner Artifact Consolidation Contract Guard ==="

for path in "$plan" "$diary" "$runner" "$failure_runner" "$helper" "$matrix_guard" "$failure_guard" "$makefile"; do
  require_file "$path"
done

require_literal "$plan" "#### Queue 3: Runner And Artifact Consolidation" \
  "pre-TLS plan must name runner/artifact consolidation queue"
require_literal "$plan" "Preserve scenario-specific names, success markers, summary" \
  "pre-TLS plan must preserve scenario-specific evidence"
require_literal "$diary" "## Round 24 Plan" \
  "diary must include the active round plan"
require_literal "$diary" 'Implement `x86_64-microkernel-runner-artifact-consolidation`' \
  "diary must identify the active consolidation target"

require_literal "$helper" "dp_default_log_root()" \
  "helper must own shared default log-root resolution"
require_literal "$helper" 'local value="${!env_name:-/home/user/mnt/dataplane/logs}"' \
  "helper must preserve /home/user/mnt/dataplane/logs default"
require_literal "$helper" "dp_new_run_id()" \
  "helper must own shared run-id generation"
require_literal "$helper" "date -u +%Y%m%dT%H%M%SZ" \
  "shared run-id generation must keep UTC timestamp shape"
require_literal "$helper" "dp_artifact_path()" \
  "helper must own shared artifact path construction"
require_literal "$helper" "dp_emit_artifact_line()" \
  "helper must own labeled artifact output"
require_literal "$helper" "dp_extract_artifact()" \
  "helper must keep fail-closed artifact extraction"
require_literal "$helper" 'if [[ "$path" != /home/user/mnt/dataplane/* ]]; then' \
  "helper must continue rejecting wrong-root artifacts"
require_literal "$helper" 'if [[ ! -s "$path" ]]; then' \
  "helper must continue rejecting empty artifacts"

require_literal "$runner" 'log_root="$(dp_default_log_root DP_VALIDATION_MATRIX_LOG_ROOT)"' \
  "matrix runner must use shared default log-root helper"
require_literal "$runner" 'run_id="$(dp_new_run_id)"' \
  "matrix runner must use shared run-id helper"
require_literal "$runner" 'summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-validation-matrix" "$run_id" "summary")"' \
  "matrix runner must use shared artifact path helper for summaries"
require_literal "$runner" 'scenario_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-validation-matrix" "$run_id" "$scenario.log")"' \
  "matrix runner must use shared artifact path helper for scenario logs"

require_literal "$failure_runner" 'log_root="$(dp_default_log_root DP_VALIDATION_FAILURE_LOG_ROOT)"' \
  "failure injection runner must use shared default log-root helper"
require_literal "$failure_runner" 'run_id="$(dp_new_run_id)"' \
  "failure injection runner must use shared run-id helper"
require_literal "$failure_runner" 'summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-validation-failure-injection" "$run_id" "summary")"' \
  "failure injection runner must use shared artifact path helper for summaries"
require_literal "$failure_runner" 'detail_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-validation-failure-injection" "$run_id" "log")"' \
  "failure injection runner must use shared artifact path helper for detail logs"
require_literal "$failure_runner" 'dp_emit_artifact_line "validation failure injection summary" "$summary"' \
  "failure injection runner must use shared labeled artifact output"
require_literal "$failure_runner" 'dp_emit_artifact_line "failure injection log" "$detail_log"' \
  "failure injection runner must use shared labeled artifact output"

require_literal "$runner" "validation-failure-injection) echo 'validation failure injection summary|failure injection log' ;;" \
  "consolidation must preserve scenario-specific failure injection labels"
require_literal "$runner" "storage-service-counters) echo 'storage service counters summary|client log|serial log|qemu log|network pcap' ;;" \
  "consolidation must preserve scenario-specific storage labels"
require_literal "$runner" "validation-failure-injection) echo 'x86_64 microkernel validation failure injection proof passed.' ;;" \
  "consolidation must preserve scenario-specific positive markers"

require_literal "$matrix_guard" 'dp_default_log_root()' \
  "matrix guard must cover shared log-root helper"
require_literal "$failure_guard" 'dp_emit_artifact_line()' \
  "failure guard must cover shared artifact-label output"
require_literal "$makefile" "x86_64-microkernel-runner-artifact-consolidation-contract:" \
  "Makefile must expose the consolidation contract target"
require_literal "$makefile" "x86_64-microkernel-runner-artifact-consolidation: guard-scripts-executable x86_64-microkernel-validation-matrix-contract x86_64-microkernel-validation-failure-injection-contract x86_64-microkernel-runner-artifact-consolidation-contract" \
  "Makefile must expose the consolidation proof target"
require_literal "$makefile" "./tools/x86_64_microkernel_validation_matrix_run.sh --scenario validation-failure-injection" \
  "consolidation proof target must rerun the negative-fixture matrix scenario"

reject_regex "$runner" 'validation-failure-injection\).*summary\|failure injection log.*storage-service-counters' \
  "runner must not merge scenario-specific artifact labels"
reject_regex "$helper" 'scenario_command|scenario_marker|scenario_artifact_labels|validation-failure-injection|storage-service-counters' \
  "shared helper must not absorb scenario-specific matrix semantics"
reject_regex "$helper" 'TLS|HTTPS|curl[[:space:]]+-k|OpenSSL|certificate|private[[:space:]]+key' \
  "consolidation helper must not introduce TLS/HTTPS/certificate tooling"
reject_regex "$helper" 'generic[[:space:]]+socket|benchmark[[:space:]]+tuning|retun(e|ing)|calibration|nested[[:space:]]+spec' \
  "consolidation helper must not introduce broad harness, sockets, or benchmark tuning"

echo "x86_64 microkernel runner artifact consolidation contract guard passed."
