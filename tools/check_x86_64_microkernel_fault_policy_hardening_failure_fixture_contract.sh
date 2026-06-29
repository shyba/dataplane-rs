#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

fixture="tools/x86_64_microkernel_fault_policy_hardening_failure_fixture.sh"
makefile="Makefile"
plan="aidocs/055_microkernel_tls_deferred_robust_appliance_plan_2026-05-31.md"

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
  grep -Fq -- "$literal" "$file" || fail "$note; missing literal in $file: $literal"
}

reject_regex() {
  local file="$1"
  local regex="$2"
  local note="$3"
  local tmp_dir out err status
  tmp_dir="${TMPDIR:-/home/user/mnt/dataplane/tmp}"
  mkdir -p "$tmp_dir"
  out="$(mktemp "$tmp_dir/fault-policy-fixture-guard.XXXXXX")"
  err="$(mktemp "$tmp_dir/fault-policy-fixture-guard-err.XXXXXX")"
  set +e
  rg -n -- "$regex" "$file" >"$out" 2>"$err"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    cat "$out" >&2
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

echo "=== x86_64 Microkernel Fault Policy Hardening Failure Fixture Contract ==="

for file in "$fixture" "$makefile" "$plan"; do
  require_file "$file"
done

for literal in \
  'mnt_root="/home/user/mnt/dataplane"' \
  'expect_pass()' \
  'expect_fail()' \
  'require_post_fault_cli_status_order()' \
  'case_valid_minimal()' \
  'case_missing_contained_marker()' \
  'case_missing_post_fault_denied_marker()' \
  'case_pre_fault_only_cli_status()' \
  'case_floating_hardcoded_status()' \
  'case_duplicate_summary_key()' \
  'case_wrong_fault_count()' \
  'case_stale_artifact()' \
  'case_restart_claim()' \
  'case_replay_claim()' \
  'case_cleanup_claim()' \
  'case_pcap_global_header_only()' \
  'expect_pass "valid_minimal_fixture" case_valid_minimal' \
  'expect_fail "missing_contained_marker_failed" case_missing_contained_marker' \
  'expect_fail "missing_post_fault_denied_marker_failed" case_missing_post_fault_denied_marker' \
  'expect_fail "pre_fault_only_cli_status_failed" case_pre_fault_only_cli_status' \
  'expect_fail "floating_hardcoded_status_failed" case_floating_hardcoded_status' \
  'expect_fail "duplicate_summary_key_failed" case_duplicate_summary_key' \
  'expect_fail "wrong_fault_count_failed" case_wrong_fault_count' \
  'expect_fail "stale_artifact_failed" case_stale_artifact' \
  'expect_fail "restart_claim_failed" case_restart_claim' \
  'expect_fail "replay_claim_failed" case_replay_claim' \
  'expect_fail "cleanup_claim_failed" case_cleanup_claim' \
  'expect_fail "pcap_global_header_only_failed" case_pcap_global_header_only' \
  'fault_policy_failure_fixture_summary_status=pass'; do
  require_literal "$fixture" "$literal" "fixture must preserve literal"
done

for literal in \
  'DPFAULT:post-fault-denied task=TASK_BLOCK id=4 endpoint=4 region=block_task_region right=block-write generation=1 status=faulted faults=1 route=denied enqueued=0 restart=0 replay=0 recovery=0' \
  'DPMK:POST-FAULT-DENIED-OK' \
  'DPMK:FAULT-RECORDED' \
  'DPMK:POST-FAULT-CLI-BEGIN' \
  'DPCLI:TASK block id=4 endpoint=4 status=faulted faults=1' \
  'DPMK:POST-FAULT-STATUS-BEGIN' \
  'DPCLI:PARITY DPSTATUS routes=t1>1,c2>3,b3>4,n6>5,h7>3,d8>5 service_caps=h2,t3,ls4,cat5,st6,neg7,tt8,tf9,tb10,ttc11,q12,p13 storage_mode=ro network_counters=a1,i1,u16,200=3,404=1,405=1,413=2,500=1 timer_status=ok,cli,fs,blk,net,tcpip,http,dhcp fault_status=tb:faulted,f1,r0,p0,c0 generation_id=1' \
  'DPMK:POST-FAULT-CLI-STATUS-OK' \
  'fault_policy_cli_task_block_visible=true' \
  'fault_policy_status_route_visible=true' \
  'fault_policy_post_fault_denied_visible=true' \
  'fault_policy_post_fault_denied_operation=block-write' \
  'fault_policy_post_fault_denied_route=denied' \
  'fault_policy_post_fault_denied_enqueued=0'; do
  require_literal "$fixture" "$literal" "fixture must require post-fault denied evidence"
done

require_literal "$makefile" 'x86_64-microkernel-fault-policy-hardening-failure-fixture-contract:' \
  "Makefile must expose fault-policy failure fixture contract"
require_literal "$makefile" './tools/check_x86_64_microkernel_fault_policy_hardening_failure_fixture_contract.sh' \
  "Makefile must run fault-policy failure fixture guard"
require_literal "$makefile" 'x86_64-microkernel-fault-policy-hardening-failure-fixture:' \
  "Makefile must expose fault-policy failure fixture target"
require_literal "$plan" 'x86_64-microkernel-fault-policy-hardening-failure-fixture' \
  "plan must record the failure fixture packet"

reject_regex "$fixture" 'qemu-system|x86_64_microkernel_fat32_run|curl[[:space:]]+-k|openssl|rustls|embedded-tls|webpki|aws-lc-rs|STRICT_FIVE_CALIBRATION|run_strict_five|strict-five|retune[[:space:]]|calibration' \
  "failure fixture must stay parser-only and avoid QEMU, TLS tooling, or benchmark retuning"
reject_regex "$fixture" 'raw_memory_dump=true|page_table_dump=true|debugger_shell=true|default_writable=true|benchmark_result=true' \
  "failure fixture must not claim raw-memory, page-table, debugger, writable, or benchmark evidence"

tmp_dir="${TMPDIR:-/home/user/mnt/dataplane/tmp}"
mkdir -p "$tmp_dir"
run_stdout="$(mktemp "$tmp_dir/fault-policy-fixture-contract.XXXXXX.out")"
run_stderr="$(mktemp "$tmp_dir/fault-policy-fixture-contract.XXXXXX.err")"

set +e
./tools/x86_64_microkernel_fault_policy_hardening_failure_fixture.sh >"$run_stdout" 2>"$run_stderr"
status=$?
set -e
if [[ "$status" -ne 0 ]]; then
  cat "$run_stdout" >&2 || true
  cat "$run_stderr" >&2 || true
  rm -f "$run_stdout" "$run_stderr"
  fail "fault-policy failure fixture path did not complete cleanly"
fi

summary_path="$(awk -F': ' '/^summary: / { print $2; exit }' "$run_stdout")"
[[ -n "$summary_path" ]] || fail "fault-policy failure fixture did not report a summary path"
[[ -s "$summary_path" ]] || fail "missing or empty fault-policy failure fixture summary: $summary_path"

case_dir="$(dirname "$summary_path")/$(basename "$summary_path" .summary).cases"
[[ -d "$case_dir" ]] || fail "missing fault-policy failure fixture case dir: $case_dir"

for literal in \
  'fault_policy_failure_fixture_summary_status=pass' \
  'valid_minimal_fixture=passed' \
  'missing_contained_marker_failed=failed' \
  'missing_post_fault_denied_marker_failed=failed' \
  'pre_fault_only_cli_status_failed=failed' \
  'floating_hardcoded_status_failed=failed' \
  'duplicate_summary_key_failed=failed' \
  'wrong_fault_count_failed=failed' \
  'stale_artifact_failed=failed' \
  'restart_claim_failed=failed' \
  'replay_claim_failed=failed' \
  'cleanup_claim_failed=failed' \
  'pcap_global_header_only_failed=failed'; do
  require_literal "$summary_path" "$literal" "fault-policy failure fixture summary must preserve fresh pass/fail matrix"
done

for case_name in \
  missing_contained_marker_failed \
  missing_post_fault_denied_marker_failed \
  pre_fault_only_cli_status_failed \
  floating_hardcoded_status_failed \
  duplicate_summary_key_failed \
  wrong_fault_count_failed \
  stale_artifact_failed \
  restart_claim_failed \
  replay_claim_failed \
  cleanup_claim_failed \
  pcap_global_header_only_failed; do
  [[ -f "$case_dir/$case_name.out" ]] || fail "missing case stdout for $case_name"
  [[ -f "$case_dir/$case_name.err" ]] || fail "missing case stderr for $case_name"
done

rm -f "$run_stdout" "$run_stderr"

echo "x86_64 microkernel fault policy hardening failure fixture contract OK"
