#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

plan="aidocs/051_microkernel_pre_tls_appliance_plan_2026-05-30.md"
frontier="aidocs/050_microkernel_robust_design_frontier_2026-05-30.md"
runner="tools/x86_64_microkernel_validation_matrix_run.sh"
helper="tools/x86_64_microkernel_validation_matrix_lib.sh"
failure_runner="tools/x86_64_microkernel_validation_failure_injection.sh"
matrix_guard="tools/check_x86_64_microkernel_validation_matrix_contract.sh"
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

echo "=== x86_64 Microkernel Validation Failure Injection Contract Guard ==="

for path in "$plan" "$frontier" "$runner" "$helper" "$failure_runner" "$matrix_guard" "$makefile"; do
  require_file "$path"
done

require_literal "$plan" "#### Queue 2: Validation Failure Injection" \
  "pre-TLS plan must name the active validation failure injection queue"
require_literal "$plan" "do not require QEMU for fixture-only parser failures" \
  "pre-TLS plan must keep parser failure injection fixture-based"
require_literal "$frontier" "validation failure injection" \
  "frontier must mention validation failure injection"

require_literal "$helper" "dp_extract_artifact()" \
  "matrix helper must own artifact extraction"
require_literal "$helper" 'case "$candidate" in' \
  "artifact extraction must parse labels as fixed shell prefixes"
require_literal "$helper" '"$label":\ /*) line="$candidate" ;;' \
  "artifact extraction must avoid regex interpolation for labels"
require_literal "$helper" 'path="${line#"$label": }"' \
  "artifact extraction must strip the exact fixed label prefix"
require_literal "$helper" 'if [[ "$path" != /home/user/mnt/dataplane/* ]]; then' \
  "artifact extraction must reject wrong-root paths"
require_literal "$helper" 'if [[ ! -s "$path" ]]; then' \
  "artifact extraction must reject missing or empty artifacts"
require_literal "$helper" "dp_require_marker()" \
  "matrix helper must own marker checks"
require_literal "$helper" "dp_require_key_value()" \
  "matrix helper must expose key/value summary checks"
require_literal "$helper" '"$key"=*) line="$candidate" ;;' \
  "key/value extraction must avoid regex interpolation for keys"
require_literal "$helper" "dp_require_summary_run_id()" \
  "matrix helper must expose stale summary checks"

require_literal "$runner" "source \"tools/x86_64_microkernel_validation_matrix_lib.sh\"" \
  "matrix runner must source the shared fail-closed helper"
require_literal "$runner" "validation-failure-injection" \
  "matrix runner must include the validation failure injection scenario"
require_literal "$runner" "validation-failure-injection) echo 'make x86_64-microkernel-validation-failure-injection' ;;" \
  "matrix runner must dispatch validation failure injection through Make"
require_literal "$runner" "validation-failure-injection) echo 'validation-matrix-negative-proof' ;;" \
  "matrix runner must classify failure injection as negative matrix proof"
require_literal "$runner" "validation-failure-injection) echo 'x86_64 microkernel validation failure injection proof passed.' ;;" \
  "matrix runner must require the failure injection positive marker"
require_literal "$runner" "validation-failure-injection) echo 'validation failure injection summary|failure injection log' ;;" \
  "matrix runner must require failure injection artifacts"
require_literal "$runner" 'metadata_status=0' \
  "matrix runner must check scenario metadata command status"
require_literal "$runner" 'if [[ "$metadata_status" -ne 0 || -z "$marker" ]]; then' \
  "matrix runner must reject empty marker metadata"
require_literal "$runner" 'if [[ "$metadata_status" -ne 0 || -z "$evidence_class" ]]; then' \
  "matrix runner must reject empty evidence class metadata"
require_literal "$runner" 'if [[ "$metadata_status" -ne 0 ]]; then' \
  "matrix runner must reject artifact label metadata failures"
require_literal "$runner" 'dp_require_marker "$scenario_log" "$marker"' \
  "matrix runner must use the helper marker check"

require_literal "$helper" "dp_default_log_root()" \
  "matrix helper must own default log-root resolution"
require_literal "$helper" "dp_new_run_id()" \
  "matrix helper must own run-id generation"
require_literal "$helper" "dp_artifact_path()" \
  "matrix helper must own artifact path construction"
require_literal "$helper" "dp_emit_artifact_line()" \
  "matrix helper must own labeled artifact output"
require_literal "$failure_runner" 'log_root="$(dp_default_log_root DP_VALIDATION_FAILURE_LOG_ROOT)"' \
  "failure injection runner must write under the large log root by default"
require_literal "$failure_runner" 'run_id="$(dp_new_run_id)"' \
  "failure injection runner must use shared run-id generation"
require_literal "$failure_runner" 'summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-validation-failure-injection" "$run_id" "summary")"' \
  "failure injection runner must write a summary artifact"
require_literal "$failure_runner" 'detail_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-validation-failure-injection" "$run_id" "log")"' \
  "failure injection runner must write a detail log artifact"
require_literal "$failure_runner" "missing_positive_marker_failed" \
  "failure injection must prove missing marker failure"
require_literal "$failure_runner" "missing_artifact_label_failed" \
  "failure injection must prove missing artifact label failure"
require_literal "$failure_runner" "wrong_root_artifact_failed" \
  "failure injection must prove wrong-root artifact failure"
require_literal "$failure_runner" "empty_artifact_failed" \
  "failure injection must prove empty artifact failure"
require_literal "$failure_runner" "stale_duplicate_artifact_failed" \
  "failure injection must prove stale final duplicate artifact failure"
require_literal "$failure_runner" "stale_summary_run_id_failed" \
  "failure injection must prove stale summary run id failure"
require_literal "$failure_runner" "malformed_key_value_failed" \
  "failure injection must prove malformed key/value failure"
require_literal "$failure_runner" "unknown_scenario_failed" \
  "failure injection must prove unknown scenario failure"
require_literal "$failure_runner" 'DP_VALIDATION_MATRIX_LOG_ROOT="$case_dir" ./tools/x86_64_microkernel_validation_matrix_run.sh --scenario __validation_failure_unknown__' \
  "failure injection must exercise the real matrix unknown-scenario path"
require_literal "$failure_runner" "x86_64 microkernel validation failure injection proof passed." \
  "failure injection runner must emit the matrix positive marker"
require_literal "$failure_runner" 'dp_emit_artifact_line "validation failure injection summary" "$summary"' \
  "failure injection runner must print the summary artifact label"
require_literal "$failure_runner" 'dp_emit_artifact_line "failure injection log" "$detail_log"' \
  "failure injection runner must print the detail artifact label"

require_literal "$makefile" "x86_64-microkernel-validation-failure-injection-contract:" \
  "Makefile must expose the validation failure injection contract target"
require_literal "$makefile" "x86_64-microkernel-validation-failure-injection: guard-scripts-executable x86_64-microkernel-validation-matrix-contract x86_64-microkernel-validation-failure-injection-contract" \
  "Makefile must expose the validation failure injection proof target"
require_literal "$makefile" "./tools/x86_64_microkernel_validation_failure_injection.sh" \
  "Makefile must run the validation failure injection proof runner"

require_literal "$matrix_guard" "tools/x86_64_microkernel_validation_matrix_lib.sh" \
  "matrix guard must know about the shared helper"
require_literal "$matrix_guard" "validation-failure-injection" \
  "matrix guard must require the failure injection scenario"
require_literal "$matrix_guard" 'dp_require_marker "$scenario_log" "$marker"' \
  "matrix guard must require helper-based marker checks"

reject_regex "$failure_runner" 'curl[[:space:]]+-k|OpenSSL|certificate|private[[:space:]]+key|TLS|HTTPS' \
  "failure injection packet must not add TLS/HTTPS/certificate tooling"
reject_regex "$failure_runner" 'generic[[:space:]]+socket|benchmark[[:space:]]+tuning|retun(e|ing)|calibration' \
  "failure injection packet must not add sockets or benchmark tuning"
reject_regex "$runner" 'grep[[:space:]]+-Fq[[:space:]]+"\$marker"[[:space:]]+"\$scenario_log"' \
  "matrix runner must not bypass the helper marker check"

echo "x86_64 microkernel validation failure injection contract guard passed."
