#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mnt_root="/home/user/mnt/dataplane"
log_dir="$mnt_root/logs"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
case_dir="$log_dir/x86_64-microkernel-readonly-load-fairness-failure-$run_id.cases"
fresh_marker="$case_dir/fresh"
summary="$log_dir/x86_64-microkernel-readonly-load-fairness-failure-$run_id.summary"

mkdir -p "$case_dir"
: >"$fresh_marker"
touch -d "2 seconds ago" "$fresh_marker"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

extract_artifact() {
  local log="$1"
  local label="$2"
  local count value
  count="$(awk -F': ' -v key="$label" '$1 == key { count++ } END { print count + 0 }' "$log")"
  [[ "$count" -eq 1 ]] || fail "artifact label '$label' count in $log was $count"
  value="$(awk -F': ' -v key="$label" '$1 == key { print $2 }' "$log")"
  [[ -n "$value" ]] || fail "empty artifact label '$label' in $log"
  printf '%s\n' "$value"
}

require_file() {
  local path="$1"
  [[ -s "$path" ]] || fail "missing or empty artifact: $path"
  [[ "$path" == "$log_dir"/* ]] || fail "artifact outside $log_dir: $path"
  [[ "$path" -nt "$fresh_marker" ]] || fail "stale artifact: $path"
}

require_summary_file() {
  local path="$1"
  [[ -s "$path" ]] || fail "missing or empty summary: $path"
  [[ "$path" == "$log_dir"/* ]] || fail "summary outside $log_dir: $path"
  [[ "$path" -nt "$fresh_marker" ]] || fail "stale summary: $path"
}

require_contains() {
  local path="$1"
  local marker="$2"
  grep -Fq -- "$marker" "$path" || fail "artifact missing marker: $marker"
}

require_summary_key() {
  local path="$1"
  local key="$2"
  local expected="$3"
  local count
  count="$(awk -F= -v key="$key" '$1 == key { count++ } END { print count + 0 }' "$path")"
  [[ "$count" -eq 1 ]] || fail "summary key '$key' count in $path was $count"
  grep -Fq -- "$key=$expected" "$path" || fail "summary $path missing $key=$expected"
}

require_pcap_payload() {
  local path="$1"
  require_file "$path"
  local bytes
  bytes="$(wc -c <"$path" | tr -d ' ')"
  (( bytes > 24 )) || fail "pcap contains only global header: $path"
}

write_bytes() {
  local path="$1"
  local bytes="$2"
  dd if=/dev/zero of="$path" bs=1 count="$bytes" status=none
}

case_valid_minimal() {
  local log="$case_dir/valid.log"
  local artifact="$case_dir/valid-artifact"
  local pcap="$case_dir/valid.pcap"
  local child_summary="$case_dir/valid.summary"
  local serial="$case_dir/valid.serial"
  echo "ok" >"$artifact"
  write_bytes "$pcap" 64
  {
    echo "DPMK:SERVICE-LIFECYCLE-LEDGER-OK"
    echo "DPMK:FAIR-OK"
  } >"$serial"
  {
    echo "summary: $child_summary"
    echo "pcap: $pcap"
    echo "serial: $serial"
  } >"$log"
  echo "summary_status=pass" >"$child_summary"
  require_file "$(extract_artifact "$log" "summary")"
  require_file "$artifact"
  require_pcap_payload "$pcap"
  require_file "$serial"
  require_contains "$serial" "DPMK:SERVICE-LIFECYCLE-LEDGER-OK"
  require_contains "$serial" "DPMK:FAIR-OK"
  require_summary_key "$child_summary" "summary_status" "pass"
}

case_missing_summary_artifact() {
  local log="$case_dir/missing-summary.log"
  {
    echo "summary: $log_dir/x86_64-microkernel-readonly-load-fairness-failure-missing.summary"
    echo "pcap: $case_dir/missing-summary.pcap"
  } >"$log"
  require_summary_file "$(extract_artifact "$log" "summary")"
}

case_empty_summary_artifact() {
  local log="$case_dir/empty-summary.log"
  local child_summary="$log_dir/x86_64-microkernel-readonly-load-fairness-failure-empty.summary"
  local pcap="$case_dir/empty-summary.pcap"
  : >"$child_summary"
  write_bytes "$pcap" 64
  {
    echo "summary: $child_summary"
    echo "pcap: $pcap"
  } >"$log"
  require_summary_file "$(extract_artifact "$log" "summary")"
}

case_out_of_tree_summary_artifact() {
  local log="$case_dir/out-of-tree-summary.log"
  local child_summary="/tmp/dataplane-readonly-load-fairness-outside-summary-$$"
  local pcap="$case_dir/out-of-tree-summary.pcap"
  echo "summary_status=pass" >"$child_summary"
  write_bytes "$pcap" 64
  {
    echo "summary: $child_summary"
    echo "pcap: $pcap"
  } >"$log"
  require_summary_file "$(extract_artifact "$log" "summary")"
}

case_stale_summary_artifact() {
  local log="$case_dir/stale-summary.log"
  local child_summary="$log_dir/x86_64-microkernel-readonly-load-fairness-failure-stale.summary"
  local pcap="$case_dir/stale-summary.pcap"
  echo "summary_status=pass" >"$child_summary"
  touch -d "2 hours ago" "$child_summary"
  write_bytes "$pcap" 64
  {
    echo "summary: $child_summary"
    echo "pcap: $pcap"
  } >"$log"
  require_summary_file "$(extract_artifact "$log" "summary")"
}

case_duplicate_artifact_label() {
  local log="$case_dir/duplicate-artifact-label.log"
  {
    echo "summary: $case_dir/a.summary"
    echo "summary: $case_dir/b.summary"
  } >"$log"
  extract_artifact "$log" "summary" >/dev/null
}

case_missing_artifact_label() {
  local log="$case_dir/missing-artifact-label.log"
  echo "other: $case_dir/other.summary" >"$log"
  extract_artifact "$log" "summary" >/dev/null
}

case_empty_artifact_path() {
  local artifact="$case_dir/empty-artifact"
  : >"$artifact"
  require_file "$artifact"
}

case_artifact_outside_log_dir() {
  local artifact="/tmp/dataplane-readonly-load-fairness-outside-$$"
  echo "outside" >"$artifact"
  require_file "$artifact"
}

case_stale_artifact() {
  local artifact="$case_dir/stale-artifact"
  echo "stale" >"$artifact"
  touch -d "2 hours ago" "$artifact"
  require_file "$artifact"
}

case_duplicate_summary_key() {
  local child_summary="$case_dir/duplicate-key.summary"
  {
    echo "summary_status=pass"
    echo "summary_status=pass"
  } >"$child_summary"
  require_summary_key "$child_summary" "summary_status" "pass"
}

case_wrong_summary_value() {
  local child_summary="$case_dir/wrong-value.summary"
  echo "summary_status=fail" >"$child_summary"
  require_summary_key "$child_summary" "summary_status" "pass"
}

case_wrong_counter_value() {
  local child_summary="$case_dir/wrong-counter.summary"
  {
    echo "readonly_load_fairness_release_fairness_curl_completed=7"
    echo "readonly_load_fairness_release_timer_max_gap=7"
  } >"$child_summary"
  require_summary_key "$child_summary" "readonly_load_fairness_release_fairness_curl_completed" "8"
}

case_missing_lifecycle_marker() {
  local serial="$case_dir/missing-lifecycle.serial"
  {
    echo "DPMK:FAIR-OK"
  } >"$serial"
  require_contains "$serial" "DPMK:SERVICE-LIFECYCLE-LEDGER-OK"
}

case_missing_load_fairness_marker() {
  local serial="$case_dir/missing-fairness.serial"
  {
    echo "DPMK:SERVICE-LIFECYCLE-LEDGER-OK"
  } >"$serial"
  require_contains "$serial" "DPMK:FAIR-OK"
}

case_pcap_global_header_only() {
  local pcap="$case_dir/global-header-only.pcap"
  write_bytes "$pcap" 24
  require_pcap_payload "$pcap"
}

expect_pass() {
  local name="$1"
  shift
  ( "$@" ) || fail "$name should have passed"
  echo "${name}=passed" >>"$summary"
}

expect_fail() {
  local name="$1"
  shift
  set +e
  ( "$@" ) >"$case_dir/$name.out" 2>"$case_dir/$name.err"
  local status=$?
  set -e
  [[ "$status" -ne 0 ]] || fail "$name should have failed"
  echo "${name}=failed" >>"$summary"
}

{
  echo "readonly_load_fairness_failure_fixture_summary_status=pass"
  echo "readonly_load_fairness_failure_fixture_run_id=$run_id"
} >"$summary"

expect_pass "valid_minimal_fixture" case_valid_minimal
expect_fail "missing_summary_artifact_failed" case_missing_summary_artifact
expect_fail "empty_summary_artifact_failed" case_empty_summary_artifact
expect_fail "out_of_tree_summary_artifact_failed" case_out_of_tree_summary_artifact
expect_fail "stale_summary_artifact_failed" case_stale_summary_artifact
expect_fail "duplicate_artifact_label_failed" case_duplicate_artifact_label
expect_fail "missing_artifact_label_failed" case_missing_artifact_label
expect_fail "empty_artifact_path_failed" case_empty_artifact_path
expect_fail "artifact_outside_log_dir_failed" case_artifact_outside_log_dir
expect_fail "stale_artifact_failed" case_stale_artifact
expect_fail "duplicate_summary_key_failed" case_duplicate_summary_key
expect_fail "wrong_summary_value_failed" case_wrong_summary_value
expect_fail "wrong_counter_value_failed" case_wrong_counter_value
expect_fail "missing_lifecycle_marker_failed" case_missing_lifecycle_marker
expect_fail "missing_load_fairness_marker_failed" case_missing_load_fairness_marker
expect_fail "pcap_global_header_only_failed" case_pcap_global_header_only

require_summary_key "$summary" "readonly_load_fairness_failure_fixture_summary_status" "pass"
for negative_key in \
  missing_summary_artifact_failed \
  empty_summary_artifact_failed \
  out_of_tree_summary_artifact_failed \
  stale_summary_artifact_failed \
  duplicate_artifact_label_failed \
  missing_artifact_label_failed \
  empty_artifact_path_failed \
  artifact_outside_log_dir_failed \
  stale_artifact_failed \
  duplicate_summary_key_failed \
  wrong_summary_value_failed \
  wrong_counter_value_failed \
  missing_lifecycle_marker_failed \
  missing_load_fairness_marker_failed \
  pcap_global_header_only_failed; do
  require_summary_key "$summary" "$negative_key" "failed"
done

echo "x86_64 microkernel read-only appliance load fairness failure fixture passed."
echo "summary: $summary"
