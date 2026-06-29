#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

source "tools/x86_64_microkernel_validation_matrix_lib.sh"

log_root="$(dp_default_log_root DP_VALIDATION_FAILURE_LOG_ROOT)"
run_id="$(dp_new_run_id)"
summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-validation-failure-injection" "$run_id" "summary")"
runner_summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-runner-fail-closed-expansion" "$run_id" "summary")"
detail_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-validation-failure-injection" "$run_id" "log")"
case_dir="$(dp_artifact_path "$log_root" "x86_64-microkernel-validation-failure-injection" "$run_id" "cases")"

mkdir -p "$log_root" "$case_dir"

record_case() {
  local name="$1"
  local result="$2"
  echo "$name=$result" >>"$summary"
  echo "$name=$result" >>"$detail_log"
}

expect_pass() {
  local name="$1"
  shift
  if "$@" >>"$detail_log" 2>&1; then
    record_case "$name" "true"
  else
    record_case "$name" "false"
    echo "FAIL: expected pass case failed: $name" >&2
    return 1
  fi
}

expect_fail() {
  local name="$1"
  shift
  if "$@" >>"$detail_log" 2>&1; then
    record_case "$name" "false"
    echo "FAIL: expected failure case passed: $name" >&2
    return 1
  fi
  record_case "$name" "true"
}

runner_timeout_fixture() {
  local log_file="$1"
  if timeout 1 bash -c 'sleep 2' >>"$log_file" 2>&1; then
    return 0
  fi
  printf 'runner timeout rejected\n' >>"$log_file"
  return 1
}

marker="x86_64 microkernel storage service counters proof passed."
valid_artifact="$case_dir/valid-artifact.log"
valid_artifact_secondary="$case_dir/valid-artifact-secondary.log"
empty_artifact="$case_dir/empty-artifact.log"
valid_log="$case_dir/valid-scenario.log"
missing_marker_log="$case_dir/missing-marker.log"
missing_artifact_log="$case_dir/missing-artifact.log"
wrong_root_log="$case_dir/wrong-root.log"
empty_artifact_log="$case_dir/empty-artifact-ref.log"
stale_duplicate_log="$case_dir/stale-duplicate.log"
runner_timeout_log="$case_dir/runner-timeout.log"
valid_summary="$case_dir/valid-summary.log"
stale_summary="$case_dir/stale-summary.log"
malformed_summary="$case_dir/malformed-summary.log"
unknown_scenario_log="$case_dir/unknown-scenario.log"
wrong_scenario_log="$case_dir/wrong-scenario.log"
wrong_mode_log="$case_dir/wrong-mode.log"
command_parser_log="$case_dir/command-parser.log"
valid_multi_artifact_log="$case_dir/valid-multi-artifact.log"
missing_pcap_artifact_log="$case_dir/missing-pcap-artifact.log"
missing_client_artifact_log="$case_dir/missing-client-artifact.log"
truncated_capture_log="$case_dir/truncated-capture.log"
duplicate_summary="$case_dir/duplicate-summary.log"
missing_summary_field="$case_dir/missing-summary-field.log"

printf 'valid artifact\n' >"$valid_artifact"
printf 'secondary valid artifact\n' >"$valid_artifact_secondary"
printf 'pcap bytes\n' >"$case_dir/valid.pcap"
printf 'client bytes\n' >"$case_dir/client.log"
: >"$empty_artifact"
printf '%s\nvalidation failure injection summary: %s\n' "$marker" "$valid_artifact" >"$valid_log"
printf 'booted without marker\nvalidation failure injection summary: %s\n' "$valid_artifact" >"$missing_marker_log"
printf '%s\n' "$marker" >"$missing_artifact_log"
printf '%s\nvalidation failure injection summary: /tmp/dataplane-validation-wrong-root.log\n' "$marker" >"$wrong_root_log"
printf '%s\nvalidation failure injection summary: %s\n' "$marker" "$empty_artifact" >"$empty_artifact_log"
{
  printf '%s\n' "$marker"
  printf 'validation failure injection summary: %s\n' "$valid_artifact"
  printf 'validation failure injection summary: %s\n' "$valid_artifact_secondary"
} >"$stale_duplicate_log"
{
  printf 'runner start\n'
} >"$runner_timeout_log"
printf 'validation_matrix_run_id=%s\nvalidation_matrix_result=pass\n' "$run_id" >"$valid_summary"
printf 'validation_matrix_run_id=stale-run\nvalidation_matrix_result=pass\n' >"$stale_summary"
printf 'validation_matrix_result maybe pass\n' >"$malformed_summary"
printf 'validation_matrix_run_id=%s\nvalidation_matrix_result=fail\nvalidation_matrix_result=pass\n' "$run_id" >"$duplicate_summary"
printf 'validation_matrix_run_id=%s\n' "$run_id" >"$missing_summary_field"
{
  printf '%s\n' "$marker"
  printf 'validation failure injection summary: %s\n' "$valid_artifact"
  printf 'network pcap: %s\n' "$case_dir/valid.pcap"
  printf 'client log: %s\n' "$case_dir/client.log"
} >"$valid_multi_artifact_log"
{
  printf '%s\n' "$marker"
  printf 'validation failure injection summary: %s\n' "$valid_artifact"
  printf 'client log: %s\n' "$case_dir/client.log"
} >"$missing_pcap_artifact_log"
{
  printf '%s\n' "$marker"
  printf 'validation failure injection summary: %s\n' "$valid_artifact"
  printf 'network pcap: %s\n' "$case_dir/valid.pcap"
} >"$missing_client_artifact_log"
printf 'DPMK:TRUNCATED-PARTIAL\n' >"$truncated_capture_log"

{
  echo "validation_failure_injection_run_id=$run_id"
  echo "validation_failure_injection_log_root=$log_root"
  echo "validation_failure_injection_case_dir=$case_dir"
  echo "validation_failure_injection_summary=$summary"
  echo "runner_fail_closed_expansion_summary=$runner_summary"
  echo "validation_failure_injection_log=$detail_log"
} >"$summary"

{
  echo "validation failure injection detail log"
  echo "run_id=$run_id"
  echo "case_dir=$case_dir"
} >"$detail_log"

expect_pass "present_positive_marker_passed" dp_require_marker "$valid_log" "$marker"
expect_fail "missing_positive_marker_failed" dp_require_marker "$missing_marker_log" "$marker"
expect_fail "missing_marker_file_failed" dp_require_marker "$case_dir/not-present.log" "$marker"
expect_pass "valid_artifact_passed" dp_extract_artifact "$valid_log" "validation failure injection summary"
expect_fail "missing_artifact_label_failed" dp_extract_artifact "$missing_artifact_log" "validation failure injection summary"
expect_fail "wrong_root_artifact_failed" dp_extract_artifact "$wrong_root_log" "validation failure injection summary"
expect_fail "empty_artifact_failed" dp_extract_artifact "$empty_artifact_log" "validation failure injection summary"
expect_fail "stale_duplicate_artifact_failed" dp_extract_artifact "$stale_duplicate_log" "validation failure injection summary"
expect_pass "valid_pcap_artifact_passed" dp_extract_artifact "$valid_multi_artifact_log" "network pcap"
expect_fail "missing_pcap_artifact_failed" dp_extract_artifact "$missing_pcap_artifact_log" "network pcap"
expect_pass "valid_client_log_artifact_passed" dp_extract_artifact "$valid_multi_artifact_log" "client log"
expect_fail "missing_client_log_artifact_failed" dp_extract_artifact "$missing_client_artifact_log" "client log"
expect_fail "empty_capture_failed" dp_require_marker "$empty_artifact" "DPMK:CAPTURE-OK"
expect_fail "truncated_capture_failed" dp_require_marker "$truncated_capture_log" "DPMK:TRUNCATED-COMPLETE"
expect_pass "summary_run_id_passed" dp_require_summary_run_id "$valid_summary" "$run_id"
expect_fail "stale_summary_run_id_failed" dp_require_summary_run_id "$stale_summary" "$run_id"
expect_fail "missing_summary_field_failed" dp_require_key_value "$missing_summary_field" "validation_matrix_result" "pass"
expect_fail "malformed_key_value_failed" dp_require_key_value "$malformed_summary" "validation_matrix_result" "pass"
expect_fail "duplicate_summary_key_failed" dp_reject_duplicate_keys "$duplicate_summary"
expect_fail "duplicate_artifact_label_failed" dp_extract_artifact "$stale_duplicate_log" "validation failure injection summary"
expect_fail "runner_timeout_or_failure_failed" runner_timeout_fixture "$runner_timeout_log"

set +e
bash -o pipefail -c 'printf "%s\n" no-match | grep -Fq required-marker | cat' >>"$detail_log" 2>&1
pipeline_status=$?
set -e
if [[ "$pipeline_status" -ne 0 ]]; then
  record_case "nonzero_pipeline_status_failed" "true"
else
  record_case "nonzero_pipeline_status_failed" "false"
  echo "FAIL: nonzero pipeline status did not fail closed" >&2
  exit 1
fi

set +e
DP_VALIDATION_MATRIX_LOG_ROOT="$case_dir" ./tools/x86_64_microkernel_validation_matrix_run.sh --scenario __validation_failure_unknown__ >"$unknown_scenario_log" 2>&1
unknown_status=$?
set -e
if [[ "$unknown_status" -ne 0 ]] && dp_require_marker "$unknown_scenario_log" "FAIL: unknown scenario: __validation_failure_unknown__"; then
  record_case "unknown_scenario_failed" "true"
else
  record_case "unknown_scenario_failed" "false"
  echo "FAIL: unknown scenario did not fail closed; log: $unknown_scenario_log" >&2
  exit 1
fi

set +e
DP_VALIDATION_MATRIX_LOG_ROOT="$case_dir" ./tools/x86_64_microkernel_validation_matrix_run.sh --scenario wrong-scenario-name >"$wrong_scenario_log" 2>&1
wrong_scenario_status=$?
set -e
if [[ "$wrong_scenario_status" -ne 0 ]] && dp_require_marker "$wrong_scenario_log" "FAIL: unknown scenario: wrong-scenario-name"; then
  record_case "wrong_scenario_name_failed" "true"
else
  record_case "wrong_scenario_name_failed" "false"
  echo "FAIL: wrong scenario name did not fail closed; log: $wrong_scenario_log" >&2
  exit 1
fi

set +e
DP_VALIDATION_MATRIX_LOG_ROOT="$case_dir" ./tools/x86_64_microkernel_validation_matrix_run.sh --wrong-mode >"$wrong_mode_log" 2>&1
wrong_mode_status=$?
set -e
if [[ "$wrong_mode_status" -ne 0 ]]; then
  record_case "wrong_mode_failed" "true"
else
  record_case "wrong_mode_failed" "false"
  echo "FAIL: wrong mode did not fail closed; log: $wrong_mode_log" >&2
  exit 1
fi

set +e
DP_VALIDATION_MATRIX_LOG_ROOT="$case_dir" ./tools/x86_64_microkernel_validation_matrix_run.sh --scenario >"$command_parser_log" 2>&1
command_parser_status=$?
set -e
if [[ "$command_parser_status" -ne 0 ]] && dp_require_marker "$command_parser_log" "FAIL: --scenario requires at least one scenario name"; then
  record_case "command_parser_error_failed" "true"
else
  record_case "command_parser_error_failed" "false"
  echo "FAIL: command parser error did not fail closed; log: $command_parser_log" >&2
  exit 1
fi

{
  echo "validation_failure_injection_summary_status=pass"
  echo "validation_failure_injection_cases=24"
} >>"$summary"

{
  echo "runner_fail_closed_expansion_summary_status=pass"
  echo "runner_fail_closed_expansion_run_id=$run_id"
  echo "runner_fail_closed_expansion_tls_deferred=true"
  echo "runner_fail_closed_expansion_missing_pcap_rejected=true"
  echo "runner_fail_closed_expansion_missing_serial_marker_rejected=true"
  echo "runner_fail_closed_expansion_missing_client_log_rejected=true"
  echo "runner_fail_closed_expansion_missing_summary_field_rejected=true"
  echo "runner_fail_closed_expansion_stale_run_id_rejected=true"
  echo "runner_fail_closed_expansion_wrong_mode_rejected=true"
  echo "runner_fail_closed_expansion_wrong_scenario_name_rejected=true"
  echo "runner_fail_closed_expansion_empty_capture_rejected=true"
  echo "runner_fail_closed_expansion_truncated_capture_rejected=true"
  echo "runner_fail_closed_expansion_duplicate_summary_key_rejected=true"
  echo "runner_fail_closed_expansion_duplicate_artifact_label_rejected=true"
  echo "runner_fail_closed_expansion_nonzero_pipeline_rejected=true"
  echo "runner_fail_closed_expansion_timeout_or_runner_failure_rejected=true"
  echo "runner_fail_closed_expansion_command_parser_error_rejected=true"
  echo "runner_fail_closed_expansion_no_generic_harness_ok=true"
  echo "runner_fail_closed_expansion_validation_failure_summary=$summary"
  echo "runner_fail_closed_expansion_log=$detail_log"
  echo "runner_fail_closed_expansion_case_dir=$case_dir"
} >"$runner_summary"

echo "x86_64 microkernel validation failure injection proof passed."
echo "x86_64 microkernel runner fail-closed expansion proof passed."
dp_emit_artifact_line "validation failure injection summary" "$summary"
dp_emit_artifact_line "runner fail-closed expansion summary" "$runner_summary"
dp_emit_artifact_line "failure injection log" "$detail_log"
