#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
kernel="crates/dataplane-x86_64-microkernel-smoke/src/kernel.rs"
kernel_text="crates/dataplane-x86_64-microkernel-smoke/src/kernel_text.rs"
services="crates/dataplane-x86_64-microkernel-smoke/src/services.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
fixture="tools/x86_64_microkernel_fault_policy_hardening_failure_fixture.sh"
fixture_guard="tools/check_x86_64_microkernel_fault_policy_hardening_failure_fixture_contract.sh"
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
  grep -Fq -- "$literal" "$file" || fail "$note; missing literal in $file: $literal"
}

reject_regex() {
  local file="$1"
  local regex="$2"
  local note="$3"
  local tmp_dir out err status
  tmp_dir="${TMPDIR:-/home/user/mnt/dataplane/tmp}"
  mkdir -p "$tmp_dir"
  out="$(mktemp "$tmp_dir/fault-policy-guard.XXXXXX")"
  err="$(mktemp "$tmp_dir/fault-policy-guard-err.XXXXXX")"
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

echo "=== x86_64 Microkernel Fault Policy Hardening Contract ==="

for file in "$main" "$kernel" "$kernel_text" "$services" "$runner" "$fixture" "$fixture_guard" "$makefile" "$plan" "$diary"; do
  require_file "$file"
done

for literal in \
  "pub(crate) fn append_fault_status_body(" \
  "cli::task_status_name(slot.status).as_bytes()" \
  "fn write_fault_status_serial(&self) -> Result<(), &'static str>" \
  "serial::write_str(cli::task_status_name(slot.status))" \
  "serial::write_decimal(slot.counters.faults)"; do
  require_literal "$kernel" "$literal" "guest must preserve current contained-fault evidence"
done

for literal in \
  "fn record_block_fault(&mut self) -> Result<(), &'static str>" \
  "record_fault(TASK_BLOCK)" \
  "TaskStatus::Faulted" \
  "slot.counters.faults != 1" \
  "DPMK:FAULT-RECORDED" \
  "fn emit_post_fault_cli_status_evidence(&self) -> Result<(), &'static str>" \
  "DPMK:POST-FAULT-CLI-BEGIN" \
  "DPMK:POST-FAULT-STATUS-BEGIN" \
  "DPMK:POST-FAULT-CLI-STATUS-OK" \
  "fn emit_post_fault_denied_operation_evidence(&mut self) -> Result<(), &'static str>" \
  "DPFAULT:post-fault-denied task=TASK_BLOCK id=" \
  " route=denied enqueued=0 restart=0 replay=0 recovery=0" \
  "DPMK:POST-FAULT-DENIED-OK" \
  "fn emit_post_fault_lifecycle_evidence(&self) -> Result<(), &'static str>" \
  "DPLIFE:task block id=" \
  " status=faulted faults=" \
  " restart=0 replay=0 cleanup_guarantee=0" \
  "DPMK:SERVICE-LIFECYCLE-FAULT-OK" \
  "DPMK:FAULT-CONTAINED"; do
  require_literal "$services" "$literal" "guest must preserve current contained-fault evidence"
done

require_literal "$kernel" "append_decimal_bytes(out, len, slot.counters.faults)" \
  "guest must preserve current contained-fault evidence"
require_literal "$kernel_text" "pub(crate) fn append_decimal_bytes(" \
  "text helper owner must define decimal byte appending"

for literal in \
  'DP_MICROKERNEL_FAULT_POLICY_HARDENING_PROOF' \
  '--fault-policy-hardening-proof' \
  'fault_policy_hardening_summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "fault-policy-hardening.summary")"' \
  'fault_policy_summary_status=pass' \
  'fault_policy_contained_ok=true' \
  'fault_policy_task=TASK_BLOCK' \
  'fault_policy_region=block_task_region' \
  'fault_policy_denied_right=block-write' \
  'fault_policy_generation=1' \
  'fault_policy_status=faulted' \
  'fault_policy_faults=1' \
  'fault_policy_counter_ok=true' \
  'fault_policy_task_faulted_ok=true' \
  'fault_policy_cli_task_block_visible=$fault_policy_cli_task_block_visible' \
  'fault_policy_status_route_visible=$fault_policy_status_route_visible' \
  'fault_policy_post_fault_denied_visible=true' \
  'fault_policy_post_fault_denied_operation=block-write' \
  'fault_policy_post_fault_denied_route=denied' \
  'fault_policy_post_fault_denied_enqueued=0' \
  'fault_policy_lifecycle_visible=true' \
  'fault_policy_no_restart_ok=true' \
  'fault_policy_no_replay_ok=true' \
  'fault_policy_cleanup_not_claimed_ok=true' \
  'fault_policy_safe_action=preserve-artifacts-stop-vm-clean-rerun' \
  'fault_policy_restart=false' \
  'fault_policy_reset=false' \
  'fault_policy_replay=false' \
  'fault_policy_cleanup_guarantee=false' \
  'fault_policy_raw_memory_dump=false' \
  'fault_policy_page_table_dump=false' \
  'fault_policy_debugger_shell=false' \
  'fault_policy_tls=false' \
  'fault_policy_default_writable=false' \
  'fault_policy_benchmark_result=false' \
  'FAIL: fault policy missing post-fault bounded status-route fault visibility' \
  'FAIL: fault policy missing post-fault CLI-visible block task status' \
  'FAIL: fault policy missing post-fault denied operation evidence' \
  'line == "DPMK:FAULT-RECORDED"' \
  'line.startswith(expected_task)' \
  'line.startswith("DPCLI:PARITY DPSTATUS ") and expected_fault in line' \
  'fault_status=tb:faulted,f1,r0,p0,c0 generation_id=1' \
  'duplicate summary key:' \
  'fault policy hardening summary: $fault_policy_hardening_summary'; do
  require_literal "$runner" "$literal" "runner must preserve fault-policy runtime proof literal"
done

for literal in \
  'x86_64-microkernel-fault-policy-hardening-contract:' \
  './tools/check_x86_64_microkernel_fault_policy_hardening_contract.sh' \
  'x86_64-microkernel-fault-policy-hardening-failure-fixture-contract:' \
  './tools/check_x86_64_microkernel_fault_policy_hardening_failure_fixture_contract.sh' \
  'x86_64-microkernel-fault-policy-hardening-failure-fixture:' \
  'x86_64-microkernel-fault-policy-hardening:'; do
  require_literal "$makefile" "$literal" "Makefile must expose focused fault-policy packet"
done

require_literal "$fixture" 'fault_policy_failure_fixture_summary_status=pass' \
  "failure fixture must provide parser-negative evidence"
require_literal "$fixture_guard" 'x86_64_microkernel_fault_policy_hardening_failure_fixture.sh' \
  "failure fixture guard must cover the fixture"
require_literal "$plan" 'x86_64-microkernel-fault-policy-hardening' \
  "plan must route the fault-policy hardening packet"
require_literal "$plan" 'Current packet: implemented as a truthful QEMU runtime proof' \
  "plan must record the narrow implemented packet"

tmp_dir="${TMPDIR:-/home/user/mnt/dataplane/tmp}"
mkdir -p "$tmp_dir"
run_stdout="$(mktemp "$tmp_dir/fault-policy-hardening-contract.XXXXXX.out")"
run_stderr="$(mktemp "$tmp_dir/fault-policy-hardening-contract.XXXXXX.err")"

set +e
DP_MICROKERNEL_FAULT_POLICY_HARDENING_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --fault-policy-hardening-proof >"$run_stdout" 2>"$run_stderr"
status=$?
set -e
if [[ "$status" -ne 0 ]]; then
  cat "$run_stdout" >&2 || true
  cat "$run_stderr" >&2 || true
  rm -f "$run_stdout" "$run_stderr"
  fail "fault-policy hardening proof path did not complete cleanly"
fi

summary_path="$(awk -F': ' '/^fault policy hardening summary: / { print $2; exit }' "$run_stdout")"
[[ -n "$summary_path" ]] || fail "fault-policy hardening proof did not report a summary path"
[[ -s "$summary_path" ]] || fail "missing or empty fault-policy hardening summary: $summary_path"

serial_log="$(grep -m1 '^fault_policy_serial_log=' "$summary_path" | cut -d= -f2-)"
pcap_path="$(grep -m1 '^fault_policy_pcap=' "$summary_path" | cut -d= -f2-)"
[[ -s "$serial_log" ]] || fail "missing or empty fault-policy serial log: $serial_log"
[[ -s "$pcap_path" ]] || fail "missing or empty fault-policy pcap: $pcap_path"

for literal in \
  'fault_policy_summary_status=pass' \
  'fault_policy_contained_ok=true' \
  'fault_policy_task=TASK_BLOCK' \
  'fault_policy_region=block_task_region' \
  'fault_policy_denied_right=block-write' \
  'fault_policy_generation=1' \
  'fault_policy_status=faulted' \
  'fault_policy_faults=1' \
  'fault_policy_counter_ok=true' \
  'fault_policy_task_faulted_ok=true' \
  'fault_policy_cli_task_block_visible=true' \
  'fault_policy_status_route_visible=true' \
  'fault_policy_post_fault_denied_visible=true' \
  'fault_policy_post_fault_denied_operation=block-write' \
  'fault_policy_post_fault_denied_route=denied' \
  'fault_policy_post_fault_denied_enqueued=0' \
  'fault_policy_lifecycle_visible=true' \
  'fault_policy_no_restart_ok=true' \
  'fault_policy_no_replay_ok=true' \
  'fault_policy_cleanup_not_claimed_ok=true' \
  'fault_policy_safe_action=preserve-artifacts-stop-vm-clean-rerun' \
  'fault_policy_restart=false' \
  'fault_policy_reset=false' \
  'fault_policy_replay=false' \
  'fault_policy_cleanup_guarantee=false' \
  'fault_policy_raw_memory_dump=false' \
  'fault_policy_page_table_dump=false' \
  'fault_policy_debugger_shell=false' \
  'fault_policy_tls=false' \
  'fault_policy_default_writable=false' \
  'fault_policy_benchmark_result=false'; do
  require_literal "$summary_path" "$literal" "fault-policy hardening summary must preserve fresh evidence"
done

grep -Fq -- "DPMK:FAULT-CONTAINED" "$serial_log" || fail "fault-policy serial log missing contained-fault marker"
grep -Fq -- "DPMK:FAULT-RECORDED" "$serial_log" || fail "fault-policy serial log missing fault-recorded marker"
grep -Fq -- "DPMK:POST-FAULT-CLI-BEGIN" "$serial_log" || fail "fault-policy serial log missing post-fault CLI begin marker"
grep -Fq -- "DPCLI:TASK block id=4 endpoint=4 status=faulted faults=1" "$serial_log" || fail "fault-policy serial log missing post-fault CLI task evidence"
grep -Fq -- "DPMK:POST-FAULT-STATUS-BEGIN" "$serial_log" || fail "fault-policy serial log missing post-fault status begin marker"
grep -Fq -- "DPCLI:PARITY DPSTATUS routes=t1>1,c2>3,b3>4,n6>5,h7>3,d8>5 service_caps=h2,t3,ls4,cat5,st6,neg7,tt8,tf9,tb10,ttc11,q12,p13 storage_mode=ro network_counters=a1,i1,u16,200=3,404=1,405=1,413=2,500=1 timer_status=ok,cli,fs,blk,net,tcpip,http,dhcp fault_status=tb:faulted,f1,r0,p0,c0 generation_id=1" "$serial_log" || fail "fault-policy serial log missing post-fault status-route evidence"
grep -Fq -- "DPMK:POST-FAULT-CLI-STATUS-OK" "$serial_log" || fail "fault-policy serial log missing post-fault CLI/status marker"
grep -Fq -- "DPMK:POST-FAULT-DENIED-OK" "$serial_log" || fail "fault-policy serial log missing post-fault denied marker"
grep -Fq -- "DPFAULT:post-fault-denied task=TASK_BLOCK id=4 endpoint=4 region=block_task_region right=block-write generation=1 status=faulted faults=1 route=denied enqueued=0 restart=0 replay=0 recovery=0" "$serial_log" || fail "fault-policy serial log missing post-fault denied evidence"
grep -Fq -- "DPMK:SERVICE-LIFECYCLE-FAULT-OK" "$serial_log" || fail "fault-policy serial log missing lifecycle marker"
grep -Fq -- "DPLIFE:task block id=4 endpoint=4 status=faulted faults=1 restart=0 replay=0 cleanup_guarantee=0" "$serial_log" || fail "fault-policy serial log missing lifecycle evidence"

bytes="$(wc -c <"$pcap_path" | tr -d ' ')"
(( bytes > 24 )) || fail "fault-policy pcap contains only a global header: $pcap_path"

rm -f "$run_stdout" "$run_stderr"

for file in "$runner" "$makefile" "$plan" "$diary"; do
  reject_regex "$file" 'fault[-_ ]?policy.*(restart[[:space:]]*=true|reset[[:space:]]*=true|replay[[:space:]]*=true|cleanup_guarantee[[:space:]]*=true)' \
    "fault-policy packet must not claim restart/reset/replay/cleanup support in $file"
  reject_regex "$file" 'fault[-_ ]?policy.*(raw memory|raw_memory_dump=true|page table dump|page_table_dump=true|debugger shell|debugger_shell=true)' \
    "fault-policy packet must not expose raw memory, page tables, or debugger shell in $file"
  reject_regex "$file" 'fault[-_ ]?policy.*(https://|curl[[:space:]]+-k|openssl|rustls|embedded-tls|webpki|aws-lc-rs|certificate|entropy)' \
    "fault-policy packet must not reopen TLS or crypto work in $file"
  reject_regex "$file" 'fault[-_ ]?policy.*(STRICT_FIVE_CALIBRATION|run_strict_five|strict-five|benchmark tuning|retune)' \
    "fault-policy packet must not tune or substitute benchmark evidence in $file"
done

echo "x86_64 microkernel fault policy hardening contract OK"
