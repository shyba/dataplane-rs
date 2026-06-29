#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fixture="tools/x86_64_microkernel_readonly_appliance_load_fairness_failure_fixture.sh"
makefile="Makefile"
plan="aidocs/055_microkernel_tls_deferred_robust_appliance_plan_2026-05-31.md"
diary="aidocs/050_microkernel_robust_design_diary_2026-05-30.md"

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
  local out="/tmp/dataplane-readonly-load-fairness-fixture.$$"
  local err="/tmp/dataplane-readonly-load-fairness-fixture-err.$$"
  local status
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

for file in "$fixture" "$makefile" "$plan" "$diary"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel Readonly Load Fairness Failure Fixture Contract ==="

for literal in \
  'mnt_root="/home/user/mnt/dataplane"' \
  'expect_pass()' \
  'expect_fail()' \
  'require_summary_file()' \
  'require_contains()' \
  'case_duplicate_artifact_label()' \
  'case_missing_artifact_label()' \
  'case_empty_artifact_path()' \
  'case_artifact_outside_log_dir()' \
  'case_stale_artifact()' \
  'case_missing_summary_artifact()' \
  'case_empty_summary_artifact()' \
  'case_out_of_tree_summary_artifact()' \
  'case_stale_summary_artifact()' \
  'case_duplicate_summary_key()' \
  'case_wrong_summary_value()' \
  'case_wrong_counter_value()' \
  'case_missing_lifecycle_marker()' \
  'case_missing_load_fairness_marker()' \
  'case_pcap_global_header_only()' \
  'expect_pass "valid_minimal_fixture" case_valid_minimal' \
  'expect_fail "missing_summary_artifact_failed" case_missing_summary_artifact' \
  'expect_fail "empty_summary_artifact_failed" case_empty_summary_artifact' \
  'expect_fail "out_of_tree_summary_artifact_failed" case_out_of_tree_summary_artifact' \
  'expect_fail "stale_summary_artifact_failed" case_stale_summary_artifact' \
  'expect_fail "duplicate_artifact_label_failed" case_duplicate_artifact_label' \
  'expect_fail "missing_artifact_label_failed" case_missing_artifact_label' \
  'expect_fail "empty_artifact_path_failed" case_empty_artifact_path' \
  'expect_fail "artifact_outside_log_dir_failed" case_artifact_outside_log_dir' \
  'expect_fail "stale_artifact_failed" case_stale_artifact' \
  'expect_fail "duplicate_summary_key_failed" case_duplicate_summary_key' \
  'expect_fail "wrong_summary_value_failed" case_wrong_summary_value' \
  'expect_fail "wrong_counter_value_failed" case_wrong_counter_value' \
  'expect_fail "missing_lifecycle_marker_failed" case_missing_lifecycle_marker' \
  'expect_fail "missing_load_fairness_marker_failed" case_missing_load_fairness_marker' \
  'expect_fail "pcap_global_header_only_failed" case_pcap_global_header_only' \
  'readonly_load_fairness_failure_fixture_summary_status=pass'; do
  require_literal "$fixture" "$literal" "fixture must preserve literal: $literal"
done

require_literal "$makefile" 'x86_64-microkernel-readonly-appliance-load-fairness-failure-fixture-contract:' \
  "Makefile must expose read-only load fairness failure fixture contract"
require_literal "$makefile" './tools/check_x86_64_microkernel_readonly_appliance_load_fairness_failure_fixture_contract.sh' \
  "Makefile must run read-only load fairness failure fixture guard"
require_literal "$makefile" 'x86_64-microkernel-readonly-appliance-load-fairness-failure-fixture:' \
  "Makefile must expose read-only load fairness failure fixture target"
require_literal "$makefile" './tools/x86_64_microkernel_readonly_appliance_load_fairness_failure_fixture.sh' \
  "Makefile must run read-only load fairness failure fixture"
require_literal "$plan" 'x86_64-microkernel-readonly-appliance-load-fairness-failure-fixture' \
  "plan must record the failure fixture packet"
require_literal "$diary" 'x86_64-microkernel-readonly-appliance-load-fairness-failure-fixture' \
  "diary must record the failure fixture packet"

reject_regex "$fixture" 'qemu-system|x86_64_microkernel_fat32_run|x86_64_microkernel_readonly_appliance_release|curl[[:space:]]+-k|openssl|rustls|embedded-tls|webpki|ring::|aws-lc-rs|STRICT_FIVE_CALIBRATION|run_strict_five|strict-five|retune[[:space:]]|calibration' \
  "failure fixture must stay parser-only and avoid QEMU, TLS tooling, or benchmark retuning"
reject_regex "$fixture" 'default_writable=true|write_feature_used=true|restart=true|replay=true|benchmark_result=true' \
  "failure fixture must not claim writable storage, restart, replay, or benchmark evidence"

echo "x86_64 microkernel read-only load fairness failure fixture contract OK"
