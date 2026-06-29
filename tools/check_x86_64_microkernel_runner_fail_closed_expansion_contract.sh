#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

plan="aidocs/055_microkernel_tls_deferred_robust_appliance_plan_2026-05-31.md"
pre_tls_plan="aidocs/051_microkernel_pre_tls_appliance_plan_2026-05-30.md"
runner="tools/x86_64_microkernel_validation_matrix_run.sh"
helper="tools/x86_64_microkernel_validation_matrix_lib.sh"
failure_runner="tools/x86_64_microkernel_validation_failure_injection.sh"
failure_guard="tools/check_x86_64_microkernel_validation_failure_injection_contract.sh"
consolidation_guard="tools/check_x86_64_microkernel_runner_artifact_consolidation_contract.sh"
makefile="Makefile"

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
  local status
  set +e
  rg -q --fixed-strings -- "$literal" "$path"
  status="$?"
  set -e
  if [[ "$status" -eq 0 ]]; then
    return 0
  fi
  [[ "$status" -eq 1 ]] || fail "rg failed while scanning $path for required literal: $literal"
  fail "$note"
}

reject_regex() {
  local path="$1"
  local regex="$2"
  local note="$3"
  local matches status
  set +e
  matches="$(rg -n --pcre2 -- "$regex" "$path")"
  status="$?"
  set -e
  if [[ "$status" -eq 0 ]]; then
    printf '%s\n' "$matches" >&2
    fail "$note"
  fi
  [[ "$status" -eq 1 ]] || fail "rg failed while scanning $path for forbidden regex: $regex"
}

for path in \
  "$plan" \
  "$pre_tls_plan" \
  "$runner" \
  "$helper" \
  "$failure_runner" \
  "$failure_guard" \
  "$consolidation_guard" \
  "$makefile"; do
  require_file "$path"
done

echo "=== x86_64 Microkernel Runner Fail-Closed Expansion Contract Guard ==="

require_literal "$plan" "x86_64-microkernel-runner-fail-closed-expansion" \
  "active plan must name the runner fail-closed expansion packet"
require_literal "$plan" "TLS is deferred" \
  "active plan must keep TLS deferred"
require_literal "$pre_tls_plan" "#### Queue 2: Validation Failure Injection" \
  "pre-TLS plan must keep validation failure injection as source context"

require_literal "$failure_guard" "missing_positive_marker_failed" \
  "validation failure injection guard must continue to own missing-marker fixtures"
require_literal "$failure_guard" "stale_duplicate_artifact_failed" \
  "validation failure injection guard must continue to own artifact fixture checks"
require_literal "$consolidation_guard" "dp_default_log_root()" \
  "runner artifact consolidation guard must continue to own helper extraction"

for literal in \
  "runner-fail-closed-expansion" \
  "runner-fail-closed-expansion) echo 'make x86_64-microkernel-runner-fail-closed-expansion' ;;" \
  "runner-fail-closed-expansion) echo 'validation-matrix-negative-proof' ;;" \
  "runner-fail-closed-expansion) echo 'x86_64 microkernel runner fail-closed expansion proof passed.' ;;" \
  "runner-fail-closed-expansion) echo 'runner fail-closed expansion summary|failure injection log' ;;" \
  'metadata_status=0' \
  'command="$(scenario_command "$scenario")" || metadata_status=$?' \
  'if [[ "$metadata_status" -ne 0 || -z "$command" ]]; then' \
  'if [[ "$metadata_status" -ne 0 || -z "$marker" ]]; then' \
  'if [[ "$metadata_status" -ne 0 || -z "$evidence_class" ]]; then' \
  'if [[ "$metadata_status" -ne 0 ]]; then' \
  'bash -lc "$command" >"$scenario_log" 2>&1 || status=$?' \
  'if [[ "$status" -ne 0 ]]; then' \
  'echo "result=fail" >>"$summary"' \
  'if ! dp_require_marker "$scenario_log" "$marker"; then' \
  'if ! artifact="$(extract_artifact "$scenario_log" "$label")"; then' \
  'IFS="$old_ifs"' \
  'validation_matrix_result=fail' \
  'if [[ "$failures" -ne 0 ]]; then'; do
  require_literal "$runner" "$literal" "matrix runner must fail closed on metadata, marker, status, and artifact errors: $literal"
done

for literal in \
  'runner_summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-runner-fail-closed-expansion" "$run_id" "summary")"' \
  "runner_fail_closed_expansion_summary_status=pass" \
  "runner_fail_closed_expansion_tls_deferred=true" \
  "missing_pcap_artifact_failed" \
  "missing_client_log_artifact_failed" \
  "missing_summary_field_failed" \
  "stale_summary_run_id_failed" \
  "wrong_mode_failed" \
  "wrong_scenario_name_failed" \
  "empty_capture_failed" \
  "truncated_capture_failed" \
  "duplicate_summary_key_failed" \
  "nonzero_pipeline_status_failed" \
  "command_parser_error_failed" \
  'dp_reject_duplicate_keys "$duplicate_summary"' \
  "runner_fail_closed_expansion_no_generic_harness_ok=true" \
  "x86_64 microkernel runner fail-closed expansion proof passed." \
  'dp_emit_artifact_line "runner fail-closed expansion summary" "$runner_summary"'; do
  require_literal "$failure_runner" "$literal" "failure runner must prove expanded fail-closed fixture: $literal"
done

for literal in \
  "x86_64-microkernel-runner-fail-closed-expansion-contract:" \
  "./tools/check_x86_64_microkernel_runner_fail_closed_expansion_contract.sh" \
  "x86_64-microkernel-runner-fail-closed-expansion: guard-scripts-executable x86_64-microkernel-validation-matrix-contract x86_64-microkernel-validation-failure-injection-contract x86_64-microkernel-runner-artifact-consolidation-contract x86_64-microkernel-runner-fail-closed-expansion-contract" \
  "./tools/x86_64_microkernel_validation_failure_injection.sh"; do
  require_literal "$makefile" "$literal" "Makefile must wire runner fail-closed expansion target: $literal"
done

reject_regex "$runner" '\|\|[[:space:]]*(true|:)' \
  "matrix runner must not swallow command failures with || true or || :"
reject_regex "$runner" 'grep[[:space:]]+-Fq[[:space:]]+"\$marker"|rg[[:space:]]+-Fq[[:space:]]+"\$marker"' \
  "matrix runner must use dp_require_marker instead of direct marker grep"
reject_regex "$runner" 'command="\$\(scenario_command "\$scenario"\)"[[:space:]]*$' \
  "matrix runner command metadata must capture lookup status"
require_literal "$helper" "dp_reject_duplicate_keys()" \
  "matrix helper must expose duplicate-key rejection"

for scanned in "$runner" "$failure_runner" "$makefile"; do
  reject_regex "$scanned" 'curl[[:space:]]+-k|https://|OpenSSL|openssl|certificate|private[[:space:]]+key|DPMK:TLS|HTTPS-' \
    "runner fail-closed expansion must not add TLS/HTTPS/certificate tooling in $scanned"
  reject_regex "$scanned" 'generic[[:space:]]+socket|benchmark[[:space:]]+tuning|retune[[:space:]]+benchmarks|retuning[[:space:]]+benchmarks|calibration' \
    "runner fail-closed expansion must not add sockets or benchmark tuning in $scanned"
done

echo "x86_64 microkernel runner fail-closed expansion contract guard passed."
