#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

runner="tools/x86_64_microkernel_qemu_negative_matrix.sh"
makefile="Makefile"
archived_spec="changes/__archived_changes_2026-05-31/x86_64-microkernel-qemu-negative-matrix/specs/qemu-negative-matrix/spec.md"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_file() {
  [[ -f "$1" ]] || fail "missing required file: $1"
}

require_literal() {
  local file="$1"
  local literal="$2"
  local note="$3"
  grep -Fq -- "$literal" "$file" || fail "$note"
}

reject_regex() {
  local file="$1"
  local regex="$2"
  local note="$3"
  local out err status
  out="/home/user/mnt/dataplane/tmp/qemu-negative-matrix-guard.$$"
  err="/home/user/mnt/dataplane/tmp/qemu-negative-matrix-guard-err.$$"
  mkdir -p /home/user/mnt/dataplane/tmp
  set +e
  rg -n -- "$regex" "$file" >"$out" 2>"$err"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    cat "$out"
    rm -f "$out" "$err"
    fail "$note"
  fi
  if [[ "$status" -ne 1 ]]; then
    cat "$err" >&2 || true
    rm -f "$out" "$err"
    fail "could not scan $file for forbidden regex: $regex"
  fi
  rm -f "$out" "$err"
}

for file in "$runner" "$makefile" "$archived_spec"; do
  require_file "$file"
done

for literal in \
  'mnt_root="/home/user/mnt/dataplane"' \
  'log_dir="$mnt_root/logs"' \
  'tmp_dir="$mnt_root/tmp"' \
  'DP_VALIDATION_FAILURE_LOG_ROOT="$log_dir"' \
  './tools/x86_64_microkernel_validation_failure_injection.sh >"$failure_run_log" 2>&1' \
  './tools/x86_64_microkernel_fat32_integrity_readonly.sh >"$fat32_run_log" 2>&1' \
  './tools/x86_64_microkernel_resource_budget_ledger.sh >"$positive_run_log" 2>&1' \
  'extract_label_path "$failure_run_log" "validation failure injection summary"' \
  'extract_label_path "$failure_run_log" "runner fail-closed expansion summary"' \
  'extract_label_path "$failure_run_log" "failure injection log"' \
  'extract_summary_path "$fat32_run_log" "summary"' \
  'extract_summary_path "$positive_run_log" "summary"' \
  'reject_duplicate_keys "$file"' \
  'qemu_negative_matrix_summary_status=pass' \
  'qemu_negative_matrix_fixture_set=parser-artifacts-fat32-images-positive-qemu' \
  'qemu_negative_matrix_missing_pcap_rejected=true' \
  'qemu_negative_matrix_missing_serial_marker_rejected=true' \
  'qemu_negative_matrix_missing_client_log_rejected=true' \
  'qemu_negative_matrix_stale_summary_rejected=true' \
  'qemu_negative_matrix_stale_or_missing_summary_evidence_rejected=true' \
  'qemu_negative_matrix_wrong_marker_rejected=true' \
  'qemu_negative_matrix_timeout_or_truncated_capture_rejected=true' \
  'qemu_negative_matrix_timeout_or_runner_failure_rejected=true' \
  'qemu_negative_matrix_bad_fat32_image_rejected=true' \
  'qemu_negative_matrix_positive_scenario_passed=true' \
  'qemu_negative_matrix_brittle_sleep_proof=false' \
  'qemu_negative_matrix_tls=false' \
  'qemu_negative_matrix_https=false' \
  'qemu_negative_matrix_benchmark_result=false'; do
  require_literal "$runner" "$literal" "QEMU negative matrix runner must preserve contract literal: $literal"
done

for literal in \
  'runner_fail_closed_expansion_missing_pcap_rejected' \
  'runner_fail_closed_expansion_missing_serial_marker_rejected' \
  'runner_fail_closed_expansion_missing_client_log_rejected' \
  'runner_fail_closed_expansion_stale_run_id_rejected' \
  'runner_fail_closed_expansion_wrong_mode_rejected' \
  'runner_fail_closed_expansion_wrong_scenario_name_rejected' \
  'runner_fail_closed_expansion_empty_capture_rejected' \
  'runner_fail_closed_expansion_truncated_capture_rejected' \
  'runner_fail_closed_expansion_duplicate_summary_key_rejected' \
  'runner_fail_closed_expansion_timeout_or_runner_failure_rejected' \
  'runner_fail_closed_expansion_command_parser_error_rejected' \
  'fat32_integrity_readonly_bad_boot_signature_rejected' \
  'fat32_integrity_readonly_bad_bytes_per_sector_rejected' \
  'fat32_integrity_readonly_bad_label_rejected' \
  'fat32_integrity_readonly_bad_fat_entry_rejected' \
  'fat32_integrity_readonly_oversized_hello_rejected' \
  'fat32_integrity_readonly_missing_root_entry_rejected' \
  'fat32_integrity_readonly_malformed_root_attr_rejected' \
  'fat32_integrity_readonly_bad_file_content_rejected' \
  'fat32_integrity_readonly_short_image_rejected' \
  'fat32_integrity_readonly_stale_or_missing_summary_evidence_rejected' \
  'resource_budget_ledger_summary_status'; do
  require_literal "$runner" "$literal" "QEMU negative matrix must verify upstream evidence key: $literal"
done

require_literal "$makefile" 'x86_64-microkernel-qemu-negative-matrix-contract:' \
  "Makefile must expose the QEMU negative matrix contract"
require_literal "$makefile" './tools/check_x86_64_microkernel_qemu_negative_matrix_contract.sh' \
  "Makefile must run the QEMU negative matrix contract guard"
require_literal "$makefile" 'x86_64-microkernel-qemu-negative-matrix:' \
  "Makefile must expose the QEMU negative matrix proof target"
require_literal "$makefile" './tools/x86_64_microkernel_qemu_negative_matrix.sh' \
  "Makefile proof target must run the focused QEMU negative matrix wrapper"

require_literal "$archived_spec" '### Requirement: Fail closed on named negative scenarios' \
  "active spec must keep the fail-closed requirement"
require_literal "$archived_spec" '### Requirement: Preserve explicit scenario labeling' \
  "active spec must keep explicit scenario labeling"
require_literal "$archived_spec" '### Requirement: Stay scoped to the existing non-TLS control path' \
  "active spec must keep the non-TLS boundary"
require_literal "$archived_spec" '### Requirement: Retain at least one positive control' \
  "active spec must keep the positive control"
reject_regex "$makefile" 'x86_64-microkernel-qemu-negative-matrix.*(strict-five|STRICT_FIVE_CALIBRATION|retune|calibration)' \
  "QEMU negative matrix target must not substitute benchmark calibration evidence"
reject_regex "$runner" 'qemu_negative_matrix_.*(tls=true|https=true|benchmark_result=true|brittle_sleep_proof=true)' \
  "QEMU negative matrix summary must not claim TLS, HTTPS, benchmark, or brittle sleep proof"
reject_regex "$runner" 'sleep [1-9][0-9]*' \
  "QEMU negative matrix wrapper must not use sleeps as evidence"
reject_regex "$runner" '/tmp/dataplane' \
  "QEMU negative matrix wrapper must keep artifacts under /home/user/mnt/dataplane"
reject_regex "$runner" 'recovery|restart|reset|TLS|HTTPS|generic socket|DNS|NTP|service discovery|production TCP/IP|hardware readiness|benchmark retuning|broad storage semantics' \
  "QEMU negative matrix wrapper must stay within the packet boundary"

echo "x86_64 microkernel QEMU negative matrix contract OK"
