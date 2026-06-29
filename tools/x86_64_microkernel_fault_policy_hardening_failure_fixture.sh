#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mnt_root="/home/user/mnt/dataplane"
log_dir="$mnt_root/logs"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
case_dir="$log_dir/x86_64-microkernel-fault-policy-failure-$run_id.cases"
fresh_marker="$case_dir/fresh"
summary="$log_dir/x86_64-microkernel-fault-policy-failure-$run_id.summary"

mkdir -p "$case_dir"
: >"$fresh_marker"
touch -d "2 seconds ago" "$fresh_marker"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_file() {
  local path="$1"
  [[ -s "$path" ]] || fail "missing or empty artifact: $path"
  [[ "$path" == "$log_dir"/* ]] || fail "artifact outside $log_dir: $path"
  [[ "$path" -nt "$fresh_marker" ]] || fail "stale artifact: $path"
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

require_serial_marker() {
  local path="$1"
  local marker="$2"
  grep -Fq -- "$marker" "$path" || fail "serial artifact missing marker: $marker"
}

require_post_fault_cli_status_order() {
  local path="$1"
  awk '
    $0 == "DPMK:FAULT-RECORDED" { recorded = NR }
    $0 == "DPMK:POST-FAULT-CLI-BEGIN" && recorded { cli_begin = NR }
    index($0, "DPCLI:TASK block id=4 endpoint=4 status=faulted faults=1") == 1 && cli_begin { cli = NR }
    $0 == "DPMK:POST-FAULT-STATUS-BEGIN" && cli { status_begin = NR }
    index($0, "DPCLI:PARITY DPSTATUS ") == 1 && index($0, "fault_status=tb:faulted,f1,r0,p0,c0 generation_id=1") && status_begin { status = NR }
    $0 == "DPMK:POST-FAULT-CLI-STATUS-OK" && status { ok = NR }
    index($0, "DPFAULT:post-fault-denied ") == 1 && ok { denied = NR }
    END { exit(recorded && cli_begin && cli && status_begin && status && ok && denied ? 0 : 1) }
  ' "$path" || fail "serial artifact missing ordered post-fault CLI/status evidence"
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

write_valid_serial() {
  local path="$1"
  {
    echo "DPMK:FAULT-RECORDED"
    echo "DPMK:POST-FAULT-CLI-BEGIN"
    echo "DPCLI:TASK block id=4 endpoint=4 status=faulted faults=1 mailbox=0 enqueued=1 dequeued=1"
    echo "DPMK:POST-FAULT-STATUS-BEGIN"
    echo "DPCLI:PARITY DPSTATUS routes=t1>1,c2>3,b3>4,n6>5,h7>3,d8>5 service_caps=h2,t3,ls4,cat5,st6,neg7,tt8,tf9,tb10,ttc11,q12,p13 storage_mode=ro network_counters=a1,i1,u16,200=3,404=1,405=1,413=2,500=1 timer_status=ok,cli,fs,blk,net,tcpip,http,dhcp fault_status=tb:faulted,f1,r0,p0,c0 generation_id=1"
    echo "DPMK:POST-FAULT-CLI-STATUS-OK"
    echo "DPFAULT:post-fault-denied task=TASK_BLOCK id=4 endpoint=4 region=block_task_region right=block-write generation=1 status=faulted faults=1 route=denied enqueued=0 restart=0 replay=0 recovery=0"
    echo "DPMK:POST-FAULT-DENIED-OK"
    echo "DPLIFE:task block id=4 endpoint=4 status=faulted faults=1 restart=0 replay=0 cleanup_guarantee=0"
    echo "DPMK:SERVICE-LIFECYCLE-FAULT-OK"
    echo "DPMK:FAULT-CONTAINED"
  } >"$path"
}

write_valid_summary() {
  local path="$1"
  {
    echo "fault_policy_summary_status=pass"
    echo "fault_policy_contained_ok=true"
    echo "fault_policy_task_faulted_ok=true"
    echo "fault_policy_counter_ok=true"
    echo "fault_policy_cli_task_block_visible=true"
    echo "fault_policy_status_route_visible=true"
    echo "fault_policy_faults=1"
    echo "fault_policy_post_fault_denied_visible=true"
    echo "fault_policy_post_fault_denied_operation=block-write"
    echo "fault_policy_post_fault_denied_route=denied"
    echo "fault_policy_post_fault_denied_enqueued=0"
    echo "fault_policy_no_restart_ok=true"
    echo "fault_policy_no_replay_ok=true"
    echo "fault_policy_cleanup_not_claimed_ok=true"
    echo "fault_policy_restart=false"
    echo "fault_policy_replay=false"
    echo "fault_policy_cleanup_guarantee=false"
    echo "fault_policy_raw_memory_dump=false"
    echo "fault_policy_page_table_dump=false"
  } >"$path"
}

validate_fault_policy_fixture() {
  local child_summary="$1"
  local serial="$2"
  local pcap="$3"
  require_file "$child_summary"
  require_file "$serial"
  require_pcap_payload "$pcap"
  require_serial_marker "$serial" "DPMK:FAULT-CONTAINED"
  require_serial_marker "$serial" "DPMK:FAULT-RECORDED"
  require_serial_marker "$serial" "DPMK:POST-FAULT-CLI-BEGIN"
  require_serial_marker "$serial" "DPCLI:TASK block id=4 endpoint=4 status=faulted faults=1"
  require_serial_marker "$serial" "DPMK:POST-FAULT-STATUS-BEGIN"
  require_serial_marker "$serial" "DPCLI:PARITY DPSTATUS routes=t1>1,c2>3,b3>4,n6>5,h7>3,d8>5 service_caps=h2,t3,ls4,cat5,st6,neg7,tt8,tf9,tb10,ttc11,q12,p13 storage_mode=ro network_counters=a1,i1,u16,200=3,404=1,405=1,413=2,500=1 timer_status=ok,cli,fs,blk,net,tcpip,http,dhcp fault_status=tb:faulted,f1,r0,p0,c0 generation_id=1"
  require_serial_marker "$serial" "DPMK:POST-FAULT-CLI-STATUS-OK"
  require_post_fault_cli_status_order "$serial"
  require_serial_marker "$serial" "DPMK:POST-FAULT-DENIED-OK"
  require_serial_marker "$serial" "DPFAULT:post-fault-denied task=TASK_BLOCK id=4 endpoint=4 region=block_task_region right=block-write generation=1 status=faulted faults=1 route=denied enqueued=0 restart=0 replay=0 recovery=0"
  require_serial_marker "$serial" "DPMK:SERVICE-LIFECYCLE-FAULT-OK"
  require_serial_marker "$serial" "DPLIFE:task block id=4 endpoint=4 status=faulted faults=1 restart=0 replay=0 cleanup_guarantee=0"
  require_summary_key "$child_summary" "fault_policy_summary_status" "pass"
  require_summary_key "$child_summary" "fault_policy_contained_ok" "true"
  require_summary_key "$child_summary" "fault_policy_task_faulted_ok" "true"
  require_summary_key "$child_summary" "fault_policy_counter_ok" "true"
  require_summary_key "$child_summary" "fault_policy_cli_task_block_visible" "true"
  require_summary_key "$child_summary" "fault_policy_status_route_visible" "true"
  require_summary_key "$child_summary" "fault_policy_faults" "1"
  require_summary_key "$child_summary" "fault_policy_post_fault_denied_visible" "true"
  require_summary_key "$child_summary" "fault_policy_post_fault_denied_operation" "block-write"
  require_summary_key "$child_summary" "fault_policy_post_fault_denied_route" "denied"
  require_summary_key "$child_summary" "fault_policy_post_fault_denied_enqueued" "0"
  require_summary_key "$child_summary" "fault_policy_restart" "false"
  require_summary_key "$child_summary" "fault_policy_replay" "false"
  require_summary_key "$child_summary" "fault_policy_cleanup_guarantee" "false"
  require_summary_key "$child_summary" "fault_policy_raw_memory_dump" "false"
  require_summary_key "$child_summary" "fault_policy_page_table_dump" "false"
}

case_valid_minimal() {
  local child_summary="$case_dir/valid.summary"
  local serial="$case_dir/valid.serial"
  local pcap="$case_dir/valid.pcap"
  write_valid_summary "$child_summary"
  write_valid_serial "$serial"
  write_bytes "$pcap" 64
  validate_fault_policy_fixture "$child_summary" "$serial" "$pcap"
}

case_missing_contained_marker() {
  local child_summary="$case_dir/missing-marker.summary"
  local serial="$case_dir/missing-marker.serial"
  local pcap="$case_dir/missing-marker.pcap"
  write_valid_summary "$child_summary"
  {
    echo "DPLIFE:task block id=4 endpoint=4 status=faulted faults=1 restart=0 replay=0 cleanup_guarantee=0"
    echo "DPMK:SERVICE-LIFECYCLE-FAULT-OK"
  } >"$serial"
  write_bytes "$pcap" 64
  validate_fault_policy_fixture "$child_summary" "$serial" "$pcap"
}

case_missing_post_fault_denied_marker() {
  local child_summary="$case_dir/missing-post-fault-denied.summary"
  local serial="$case_dir/missing-post-fault-denied.serial"
  local pcap="$case_dir/missing-post-fault-denied.pcap"
  write_valid_summary "$child_summary"
  {
    echo "DPLIFE:task block id=4 endpoint=4 status=faulted faults=1 restart=0 replay=0 cleanup_guarantee=0"
    echo "DPMK:SERVICE-LIFECYCLE-FAULT-OK"
    echo "DPMK:FAULT-CONTAINED"
  } >"$serial"
  write_bytes "$pcap" 64
  validate_fault_policy_fixture "$child_summary" "$serial" "$pcap"
}

case_pre_fault_only_cli_status() {
  local child_summary="$case_dir/pre-fault-only.summary"
  local serial="$case_dir/pre-fault-only.serial"
  local pcap="$case_dir/pre-fault-only.pcap"
  write_valid_summary "$child_summary"
  {
    echo "DPMK:POST-FAULT-CLI-BEGIN"
    echo "DPCLI:TASK block id=4 endpoint=4 status=ready faults=0 mailbox=0 enqueued=1 dequeued=1"
    echo "DPMK:POST-FAULT-STATUS-BEGIN"
    echo "DPCLI:PARITY DPSTATUS routes=t1>1,c2>3,b3>4,n6>5,h7>3,d8>5 service_caps=h2,t3,ls4,cat5,st6,neg7,tt8,tf9,tb10,ttc11,q12,p13 storage_mode=ro network_counters=a1,i1,u16,200=3,404=1,405=1,413=2,500=1 timer_status=ok,cli,fs,blk,net,tcpip,http,dhcp fault_status=tb:ready,f0,r0,p0,c0 generation_id=1"
    echo "DPMK:POST-FAULT-CLI-STATUS-OK"
    echo "DPFAULT:post-fault-denied task=TASK_BLOCK id=4 endpoint=4 region=block_task_region right=block-write generation=1 status=faulted faults=1 route=denied enqueued=0 restart=0 replay=0 recovery=0"
    echo "DPMK:POST-FAULT-DENIED-OK"
    echo "DPLIFE:task block id=4 endpoint=4 status=faulted faults=1 restart=0 replay=0 cleanup_guarantee=0"
    echo "DPMK:SERVICE-LIFECYCLE-FAULT-OK"
    echo "DPMK:FAULT-CONTAINED"
  } >"$serial"
  write_bytes "$pcap" 64
  validate_fault_policy_fixture "$child_summary" "$serial" "$pcap"
}

case_floating_hardcoded_status() {
  local child_summary="$case_dir/floating-hardcoded-status.summary"
  local serial="$case_dir/floating-hardcoded-status.serial"
  local pcap="$case_dir/floating-hardcoded-status.pcap"
  write_valid_summary "$child_summary"
  {
    echo "DPMK:FAULT-RECORDED"
    echo "DPCLI:PARITY DPSTATUS routes=t1>1,c2>3,b3>4,n6>5,h7>3,d8>5 service_caps=h2,t3,ls4,cat5,st6,neg7,tt8,tf9,tb10,ttc11,q12,p13 storage_mode=ro network_counters=a1,i1,u16,200=3,404=1,405=1,413=2,500=1 timer_status=ok,cli,fs,blk,net,tcpip,http,dhcp fault_status=tb:faulted,f1,r0,p0,c0 generation_id=1"
    echo "DPMK:POST-FAULT-CLI-BEGIN"
    echo "DPCLI:TASK block id=4 endpoint=4 status=faulted faults=1 mailbox=0 enqueued=1 dequeued=1"
    echo "DPMK:POST-FAULT-STATUS-BEGIN"
    echo "DPMK:POST-FAULT-CLI-STATUS-OK"
    echo "DPFAULT:post-fault-denied task=TASK_BLOCK id=4 endpoint=4 region=block_task_region right=block-write generation=1 status=faulted faults=1 route=denied enqueued=0 restart=0 replay=0 recovery=0"
    echo "DPMK:POST-FAULT-DENIED-OK"
    echo "DPLIFE:task block id=4 endpoint=4 status=faulted faults=1 restart=0 replay=0 cleanup_guarantee=0"
    echo "DPMK:SERVICE-LIFECYCLE-FAULT-OK"
    echo "DPMK:FAULT-CONTAINED"
  } >"$serial"
  write_bytes "$pcap" 64
  validate_fault_policy_fixture "$child_summary" "$serial" "$pcap"
}

case_duplicate_summary_key() {
  local child_summary="$case_dir/duplicate-key.summary"
  local serial="$case_dir/duplicate-key.serial"
  local pcap="$case_dir/duplicate-key.pcap"
  write_valid_summary "$child_summary"
  echo "fault_policy_summary_status=pass" >>"$child_summary"
  write_valid_serial "$serial"
  write_bytes "$pcap" 64
  validate_fault_policy_fixture "$child_summary" "$serial" "$pcap"
}

case_wrong_fault_count() {
  local child_summary="$case_dir/wrong-fault-count.summary"
  local serial="$case_dir/wrong-fault-count.serial"
  local pcap="$case_dir/wrong-fault-count.pcap"
  write_valid_summary "$child_summary"
  sed -i 's/fault_policy_faults=1/fault_policy_faults=2/' "$child_summary"
  write_valid_serial "$serial"
  write_bytes "$pcap" 64
  validate_fault_policy_fixture "$child_summary" "$serial" "$pcap"
}

case_stale_artifact() {
  local child_summary="$case_dir/stale.summary"
  local serial="$case_dir/stale.serial"
  local pcap="$case_dir/stale.pcap"
  write_valid_summary "$child_summary"
  write_valid_serial "$serial"
  write_bytes "$pcap" 64
  touch -d "2 hours ago" "$serial"
  validate_fault_policy_fixture "$child_summary" "$serial" "$pcap"
}

case_restart_claim() {
  local child_summary="$case_dir/restart.summary"
  local serial="$case_dir/restart.serial"
  local pcap="$case_dir/restart.pcap"
  write_valid_summary "$child_summary"
  sed -i 's/fault_policy_restart=false/fault_policy_restart=true/' "$child_summary"
  write_valid_serial "$serial"
  write_bytes "$pcap" 64
  validate_fault_policy_fixture "$child_summary" "$serial" "$pcap"
}

case_replay_claim() {
  local child_summary="$case_dir/replay.summary"
  local serial="$case_dir/replay.serial"
  local pcap="$case_dir/replay.pcap"
  write_valid_summary "$child_summary"
  sed -i 's/fault_policy_replay=false/fault_policy_replay=true/' "$child_summary"
  write_valid_serial "$serial"
  write_bytes "$pcap" 64
  validate_fault_policy_fixture "$child_summary" "$serial" "$pcap"
}

case_cleanup_claim() {
  local child_summary="$case_dir/cleanup.summary"
  local serial="$case_dir/cleanup.serial"
  local pcap="$case_dir/cleanup.pcap"
  write_valid_summary "$child_summary"
  sed -i 's/fault_policy_cleanup_guarantee=false/fault_policy_cleanup_guarantee=true/' "$child_summary"
  write_valid_serial "$serial"
  write_bytes "$pcap" 64
  validate_fault_policy_fixture "$child_summary" "$serial" "$pcap"
}

case_pcap_global_header_only() {
  local child_summary="$case_dir/global-header.summary"
  local serial="$case_dir/global-header.serial"
  local pcap="$case_dir/global-header.pcap"
  write_valid_summary "$child_summary"
  write_valid_serial "$serial"
  write_bytes "$pcap" 24
  validate_fault_policy_fixture "$child_summary" "$serial" "$pcap"
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
  echo "fault_policy_failure_fixture_summary_status=pass"
  echo "fault_policy_failure_fixture_run_id=$run_id"
} >"$summary"

expect_pass "valid_minimal_fixture" case_valid_minimal
expect_fail "missing_contained_marker_failed" case_missing_contained_marker
expect_fail "missing_post_fault_denied_marker_failed" case_missing_post_fault_denied_marker
expect_fail "pre_fault_only_cli_status_failed" case_pre_fault_only_cli_status
expect_fail "floating_hardcoded_status_failed" case_floating_hardcoded_status
expect_fail "duplicate_summary_key_failed" case_duplicate_summary_key
expect_fail "wrong_fault_count_failed" case_wrong_fault_count
expect_fail "stale_artifact_failed" case_stale_artifact
expect_fail "restart_claim_failed" case_restart_claim
expect_fail "replay_claim_failed" case_replay_claim
expect_fail "cleanup_claim_failed" case_cleanup_claim
expect_fail "pcap_global_header_only_failed" case_pcap_global_header_only

require_summary_key "$summary" "fault_policy_failure_fixture_summary_status" "pass"
require_summary_key "$summary" "valid_minimal_fixture" "passed"
require_summary_key "$summary" "missing_contained_marker_failed" "failed"
require_summary_key "$summary" "missing_post_fault_denied_marker_failed" "failed"
require_summary_key "$summary" "pre_fault_only_cli_status_failed" "failed"
require_summary_key "$summary" "floating_hardcoded_status_failed" "failed"
require_summary_key "$summary" "duplicate_summary_key_failed" "failed"
require_summary_key "$summary" "wrong_fault_count_failed" "failed"
require_summary_key "$summary" "stale_artifact_failed" "failed"
require_summary_key "$summary" "restart_claim_failed" "failed"
require_summary_key "$summary" "replay_claim_failed" "failed"
require_summary_key "$summary" "cleanup_claim_failed" "failed"
require_summary_key "$summary" "pcap_global_header_only_failed" "failed"

echo "x86_64 microkernel fault policy hardening failure fixture passed."
echo "summary: $summary"
