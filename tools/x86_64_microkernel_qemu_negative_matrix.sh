#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mnt_root="/home/user/mnt/dataplane"
log_dir="$mnt_root/logs"
tmp_dir="$mnt_root/tmp"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
summary="$log_dir/x86_64-microkernel-qemu-negative-matrix-$run_id.summary"
failure_run_log="$log_dir/x86_64-microkernel-qemu-negative-matrix-$run_id.failure-injection.run.log"
fat32_run_log="$log_dir/x86_64-microkernel-qemu-negative-matrix-$run_id.fat32-integrity.run.log"
positive_run_log="$log_dir/x86_64-microkernel-qemu-negative-matrix-$run_id.positive.run.log"

mkdir -p "$log_dir" "$tmp_dir"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

extract_label_path() {
  local log_file="$1"
  local label="$2"
  local count value
  count="$(awk -F': ' -v key="$label" '$1 == key { count++ } END { print count + 0 }' "$log_file")"
  [[ "$count" -eq 1 ]] || fail "artifact label '$label' count in $log_file was $count"
  value="$(awk -F': ' -v key="$label" '$1 == key { print $2 }' "$log_file")"
  [[ -n "$value" ]] || fail "empty artifact label '$label'"
  [[ "$value" == "$log_dir"/* ]] || fail "artifact outside $log_dir: $value"
  [[ -s "$value" ]] || fail "missing or empty artifact: $value"
  printf '%s\n' "$value"
}

extract_summary_path() {
  local log_file="$1"
  local prefix="$2"
  local count value
  count="$(awk -F': ' -v key="$prefix" '$1 == key { count++ } END { print count + 0 }' "$log_file")"
  [[ "$count" -eq 1 ]] || fail "summary label '$prefix' count in $log_file was $count"
  value="$(awk -F': ' -v key="$prefix" '$1 == key { print $2 }' "$log_file")"
  [[ -n "$value" ]] || fail "empty summary label '$prefix'"
  [[ "$value" == "$log_dir"/* ]] || fail "summary outside $log_dir: $value"
  [[ -s "$value" ]] || fail "missing or empty summary: $value"
  printf '%s\n' "$value"
}

require_key_value() {
  local file="$1"
  local key="$2"
  local expected="$3"
  local line count
  count="$(awk -F= -v key="$key" '$1 == key { count++ } END { print count + 0 }' "$file")"
  [[ "$count" -eq 1 ]] || fail "summary key '$key' count in $file was $count"
  line="$(awk -F= -v key="$key" '$1 == key { print $0 }' "$file")"
  [[ "$line" == "$key=$expected" ]] || fail "expected $key=$expected in $file, got '${line:-missing}'"
}

reject_duplicate_keys() {
  local file="$1"
  local duplicate
  duplicate="$(awk -F= '/^[A-Za-z0-9_]+=/ { print $1 }' "$file" | sort | uniq -d)"
  [[ -z "$duplicate" ]] || fail "duplicate summary key(s) in $file: $duplicate"
}

check_summary_set() {
  local file="$1"
  reject_duplicate_keys "$file"
  [[ -s "$file" ]] || fail "summary is empty: $file"
}

DP_VALIDATION_FAILURE_LOG_ROOT="$log_dir" \
  DP_VALIDATION_MATRIX_LOG_ROOT="$log_dir" \
  TMPDIR="$tmp_dir" \
  ./tools/x86_64_microkernel_validation_failure_injection.sh >"$failure_run_log" 2>&1

failure_summary="$(extract_label_path "$failure_run_log" "validation failure injection summary")"
runner_summary="$(extract_label_path "$failure_run_log" "runner fail-closed expansion summary")"
failure_detail_log="$(extract_label_path "$failure_run_log" "failure injection log")"

TMPDIR="$tmp_dir" ./tools/x86_64_microkernel_fat32_integrity_readonly.sh >"$fat32_run_log" 2>&1
fat32_summary="$(extract_summary_path "$fat32_run_log" "summary")"

TMPDIR="$tmp_dir" ./tools/x86_64_microkernel_resource_budget_ledger.sh >"$positive_run_log" 2>&1
positive_summary="$(extract_summary_path "$positive_run_log" "summary")"

for file in "$failure_summary" "$runner_summary" "$fat32_summary" "$positive_summary"; do
  check_summary_set "$file"
done

require_key_value "$failure_summary" "validation_failure_injection_summary_status" "pass"
require_key_value "$failure_summary" "missing_positive_marker_failed" "true"
require_key_value "$failure_summary" "missing_pcap_artifact_failed" "true"
require_key_value "$failure_summary" "missing_client_log_artifact_failed" "true"
require_key_value "$failure_summary" "truncated_capture_failed" "true"
require_key_value "$failure_summary" "stale_summary_run_id_failed" "true"
require_key_value "$failure_summary" "duplicate_summary_key_failed" "true"
require_key_value "$failure_summary" "nonzero_pipeline_status_failed" "true"

require_key_value "$runner_summary" "runner_fail_closed_expansion_summary_status" "pass"
require_key_value "$runner_summary" "runner_fail_closed_expansion_missing_pcap_rejected" "true"
require_key_value "$runner_summary" "runner_fail_closed_expansion_missing_serial_marker_rejected" "true"
require_key_value "$runner_summary" "runner_fail_closed_expansion_missing_client_log_rejected" "true"
require_key_value "$runner_summary" "runner_fail_closed_expansion_stale_run_id_rejected" "true"
require_key_value "$runner_summary" "runner_fail_closed_expansion_wrong_mode_rejected" "true"
require_key_value "$runner_summary" "runner_fail_closed_expansion_wrong_scenario_name_rejected" "true"
require_key_value "$runner_summary" "runner_fail_closed_expansion_empty_capture_rejected" "true"
require_key_value "$runner_summary" "runner_fail_closed_expansion_truncated_capture_rejected" "true"
require_key_value "$runner_summary" "runner_fail_closed_expansion_duplicate_summary_key_rejected" "true"
require_key_value "$runner_summary" "runner_fail_closed_expansion_nonzero_pipeline_rejected" "true"
require_key_value "$runner_summary" "runner_fail_closed_expansion_timeout_or_runner_failure_rejected" "true"
require_key_value "$runner_summary" "runner_fail_closed_expansion_command_parser_error_rejected" "true"

require_key_value "$fat32_summary" "fat32_integrity_readonly_summary_status" "pass"
require_key_value "$fat32_summary" "fat32_integrity_readonly_bad_boot_signature_rejected" "true"
require_key_value "$fat32_summary" "fat32_integrity_readonly_bad_bytes_per_sector_rejected" "true"
require_key_value "$fat32_summary" "fat32_integrity_readonly_bad_label_rejected" "true"
require_key_value "$fat32_summary" "fat32_integrity_readonly_bad_fat_entry_rejected" "true"
require_key_value "$fat32_summary" "fat32_integrity_readonly_oversized_hello_rejected" "true"
require_key_value "$fat32_summary" "fat32_integrity_readonly_missing_root_entry_rejected" "true"
require_key_value "$fat32_summary" "fat32_integrity_readonly_malformed_root_attr_rejected" "true"
require_key_value "$fat32_summary" "fat32_integrity_readonly_bad_file_content_rejected" "true"
require_key_value "$fat32_summary" "fat32_integrity_readonly_short_image_rejected" "true"
require_key_value "$fat32_summary" "fat32_integrity_readonly_stale_or_missing_summary_evidence_rejected" "true"

require_key_value "$positive_summary" "resource_budget_ledger_summary_status" "pass"
require_key_value "$positive_summary" "resource_budget_ledger_begin_marker_ok" "true"
require_key_value "$positive_summary" "resource_budget_ledger_ok_marker_ok" "true"

{
  echo "qemu_negative_matrix_summary_status=pass"
  echo "qemu_negative_matrix_run_id=$run_id"
  echo "qemu_negative_matrix_log_root=$log_dir"
  echo "qemu_negative_matrix_failure_run_log=$failure_run_log"
  echo "qemu_negative_matrix_fat32_run_log=$fat32_run_log"
  echo "qemu_negative_matrix_positive_run_log=$positive_run_log"
  echo "qemu_negative_matrix_validation_failure_summary=$failure_summary"
  echo "qemu_negative_matrix_runner_fail_closed_summary=$runner_summary"
  echo "qemu_negative_matrix_failure_detail_log=$failure_detail_log"
  echo "qemu_negative_matrix_fat32_integrity_summary=$fat32_summary"
  echo "qemu_negative_matrix_positive_summary=$positive_summary"
  echo "qemu_negative_matrix_fixture_set=parser-artifacts-fat32-images-positive-qemu"
  echo "qemu_negative_matrix_missing_pcap_rejected=true"
  echo "qemu_negative_matrix_missing_serial_marker_rejected=true"
  echo "qemu_negative_matrix_missing_client_log_rejected=true"
  echo "qemu_negative_matrix_stale_summary_rejected=true"
  echo "qemu_negative_matrix_stale_or_missing_summary_evidence_rejected=true"
  echo "qemu_negative_matrix_wrong_marker_rejected=true"
  echo "qemu_negative_matrix_timeout_or_truncated_capture_rejected=true"
  echo "qemu_negative_matrix_timeout_or_runner_failure_rejected=true"
  echo "qemu_negative_matrix_bad_fat32_image_rejected=true"
  echo "qemu_negative_matrix_positive_scenario_passed=true"
  echo "qemu_negative_matrix_brittle_sleep_proof=false"
  echo "qemu_negative_matrix_tls=false"
  echo "qemu_negative_matrix_https=false"
  echo "qemu_negative_matrix_benchmark_result=false"
} >"$summary"

check_summary_set "$summary"

echo "x86_64 microkernel QEMU negative matrix proof passed."
echo "summary: $summary"
