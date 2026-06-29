#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
matrix_runner="tools/x86_64_microkernel_validation_matrix_run.sh"
matrix_guard="tools/check_x86_64_microkernel_validation_matrix_contract.sh"
makefile="Makefile"
fat32_guard="tools/check_x86_64_microkernel_fat32_contract.sh"
operator_guard="tools/check_x86_64_microkernel_cli_operator_contract.sh"
nontls_guard="tools/check_x86_64_microkernel_nontls_network_service_contract.sh"
service_ipc_guard="tools/check_x86_64_microkernel_service_ipc_audit_contract.sh"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_file() {
  local file="$1"
  [[ -f "$file" ]] || fail "required file missing: $file"
}

require_literal() {
  local file="$1"
  local needle="$2"
  local note="$3"
  rg -q --fixed-strings -- "$needle" "$file" || fail "$note"
}

reject_regex() {
  local file="$1"
  local regex="$2"
  local note="$3"
  local matches status
  set +e
  matches="$(rg -n -- "$regex" "$file")"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    printf '%s\n' "$matches"
    fail "$note"
  fi
  [[ "$status" -eq 1 ]] || fail "could not scan $file for forbidden regex: $regex"
}

for file in \
  "$main" \
  "$runner" \
  "$matrix_runner" \
  "$matrix_guard" \
  "$makefile" \
  "$fat32_guard" \
  "$operator_guard" \
  "$nontls_guard" \
  "$service_ipc_guard"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel Scheduler Fairness Load Contract Guard ==="

for marker in \
  'DPMK:FAIR-HTTP-OK' \
  'DPMK:FAIR-CLI-OK' \
  'DPMK:FAIR-FS-OK' \
  'DPMK:FAIR-FAULT-CONTAINED' \
  'DPMK:FAIR-TIMER-MAXGAP:' \
  'DPMK:FAIR-OK'; do
  require_literal "$main" "$marker" "guest must emit scheduler fairness marker $marker"
  require_literal "$runner" "$marker" "runner must require or parse scheduler fairness marker $marker"
done

require_literal "$main" 'fn run_integrated_fairness(&mut self)' \
  "guest must keep the integrated fairness workload"
require_literal "$main" 'serial::write_str("DPMK:FAIR-TIMER-MAXGAP:");' \
  "guest must emit a measurable fairness timer max-gap"

for literal in \
  'DP_MICROKERNEL_SCHEDULER_FAIRNESS_LOAD_PROOF' \
  '--scheduler-fairness-load-proof' \
  'DP_SCHEDULER_FAIRNESS_LOAD_PROOF' \
  'scheduler_fairness_summary=' \
  'scheduler_fairness_load=1' \
  'scheduler_fairness_load_mode=usernet-hostfwd-plus-serial-cli' \
  'scheduler_fairness_load_http_ok=true' \
  'scheduler_fairness_load_cli_ok=true' \
  'scheduler_fairness_load_fs_ok=true' \
  'scheduler_fairness_load_fault_contained=true' \
  'scheduler_fairness_load_timer_bound_ok=true' \
  'x86_64 microkernel scheduler fairness load proof passed.' \
  'fairness_bootstrap_count=8' \
  'fairness_curl_count=8' \
  'fairness_url="http://127.0.0.1:' \
  'fairness_curl_completed=' \
  'fairness_bootstrap_curl_completed=' \
  'fairness_expected_curl_gets=' \
  'fairness_timer_max_gap=' \
  'fairness_fault_contained=true' \
  'fairness_serial_evidence=true' \
  'fairness pcap: ' \
  'scheduler fairness summary: '; do
  require_literal "$runner" "$literal" "runner must expose scheduler fairness evidence literal: $literal"
done

for command in \
  '"tasks"' \
  '"fs ls /"' \
  '"fs cat /INDEX.HTM"' \
  '"task timer"' \
  '"task tcpip"' \
  '"queues"' \
  '"parity"' \
  '"shell"' \
  '"json status"' \
  '"restart"' \
  '"reset"' \
  '"raw memory"' \
  '"page table dump"' \
  '"mmio dump"' \
  '"debug"' \
  '"DPMK:CLI-OPERATOR-OK"'; do
  require_literal "$runner" "$command" "runner must drive concurrent CLI command $command"
done

require_literal "$runner" 'if [[ "$mode" == "--fairness-proof" || "$mode" == "--scheduler-fairness-load-proof" ]]; then' \
  "scheduler fairness load must reuse the existing fairness QEMU branch"
require_literal "$runner" 'if [[ "$mode" == "--scheduler-fairness-load-proof" ]]; then' \
  "scheduler fairness load must add only mode-specific evidence checks"
require_literal "$runner" 'b"DPMK:CLI-OPERATOR-OK"' \
  "scheduler fairness load must keep the operator completion marker in the parity step"
require_literal "$runner" 'b":parity"' \
  "scheduler fairness load must drive parity before the CLI completion gate"
require_literal "$runner" '("queues", [b"DPMK:FAIR-CLI-BEGIN:", b":queues", b"DPCLI:QUEUES timer=0 cli=0 fs=0 block=0 net=0 tcpip=0 http=0 dhcp=0", b"DPMK:CLI-OPERATOR-OK", b"DPMK:FAIR-CLI-END:"])' \
  "scheduler fairness load must require the operator completion marker in fairness queues"
require_literal "$runner" '("parity",' \
  "scheduler fairness load must keep the fairness parity tuple"
require_literal "$runner" 'b"DPMK:FAIR-CLI-BEGIN:",' \
  "scheduler fairness load must start fairness parity with the fairness begin marker"
require_literal "$runner" 'b":parity",' \
  "scheduler fairness load must drive the parity probe"
require_literal "$runner" 'b"DPCLI:PARITY DPSTATUS routes=t1>1,c2>3,b3>4,n6>5,h7>3,d8>5 service_caps=h2,t3,ls4,cat5,st6,neg7,tt8,tf9,tb10,ttc11,q12,p13 storage_mode=ro network_counters=a1,i1,u16,200=3,404=1,405=1,413=2,500=1 timer_status=ok,cli,fs,blk,net,tcpip,http,dhcp fault_status=tb:ready,f0,r0,p0,c0 generation_id=1"' \
  "scheduler fairness load must use the current ready-state fairness parity payload"
require_literal "$runner" 'b"DPMK:FAIR-CLI-END:"])' \
  "scheduler fairness load must close the fairness parity tuple"
reject_regex "$runner" 'DPMK:FAIR-CLI-BEGIN:.*:parity.*CLI-OPERATOR-OK' \
  "scheduler fairness load must not require the operator completion marker in fairness parity"
require_literal "$runner" 'b"DPMK:CLI-COMMANDS-OK"' \
  "scheduler fairness load must keep the CLI completion gate"
require_literal "$runner" 'b"DPMK:FAIR-CLI-INPUT-READY"' \
  "scheduler fairness load must wait for the fairness input readiness gate"
for literal in \
  'b":shell"' \
  'b":json status"' \
  'b":restart"' \
  'b":reset"' \
  'b":raw memory"' \
  'b":page table dump"' \
  'b":mmio dump"' \
  'b":debug"'; do
  require_literal "$runner" "$literal" "scheduler fairness load must include the current forbidden/debug CLI probe $literal"
done
require_literal "$runner" 'if [[ "$fairness_pcap_hex" != *"474554202f494e4445582e48544d20485454502f312e30"* ]]; then' \
  "runner must prove host-visible GET /INDEX.HTM packets in pcap"
require_literal "$runner" 'if [[ "$fairness_pcap_hex" != *"3c21646f63747970652068746d6c3e"* ]]; then' \
  "runner must prove host-visible INDEX.HTM response body bytes in pcap"

require_literal "$makefile" 'x86_64-microkernel-scheduler-fairness-load-contract:' \
  "Makefile must expose the scheduler fairness load contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_scheduler_fairness_load_contract.sh' \
  "Makefile scheduler fairness contract must run this guard"
require_literal "$makefile" 'x86_64-microkernel-scheduler-fairness-load: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-nontls-network-service-contract x86_64-microkernel-service-ipc-audit-contract x86_64-microkernel-scheduler-fairness-load-contract' \
  "Makefile scheduler fairness load target must preserve neighboring contract coverage"
require_literal "$makefile" 'DP_MICROKERNEL_SCHEDULER_FAIRNESS_LOAD_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --scheduler-fairness-load-proof' \
  "Makefile scheduler fairness load target must run the focused proof mode"

require_literal "$matrix_runner" 'scheduler-fairness-load' \
  "validation matrix must know the scheduler fairness load scenario"
require_literal "$matrix_runner" "scheduler-fairness-load) echo 'make x86_64-microkernel-scheduler-fairness-load' ;;" \
  "validation matrix must dispatch scheduler fairness through Make"
require_literal "$matrix_runner" "scheduler-fairness-load) echo 'x86_64 microkernel scheduler fairness load proof passed.' ;;" \
  "validation matrix must require the scheduler fairness success marker"
require_literal "$matrix_runner" "scheduler-fairness-load) echo 'scheduler fairness summary|fairness log|fairness client log|fairness curl log|fairness pcap|serial log|qemu log' ;;" \
  "validation matrix must require scheduler fairness artifacts"
require_literal "$matrix_guard" 'scheduler-fairness-load' \
  "validation matrix guard must cover scheduler fairness load"

for scanned in "$runner" "$matrix_runner" "$makefile"; do
  reject_regex "$scanned" 'https://|curl[[:space:]]+-k|openssl|rustls|embedded-tls|webpki|ring::|aws-lc-rs' \
    "scheduler fairness load must not introduce TLS/HTTPS drift in $scanned"
  reject_regex "$scanned" 'STRICT_FIVE_CALIBRATION|retune[[:space:]]+benchmarks|retuning[[:space:]]+benchmarks|benchmark[[:space:]]+tuning|calibration' \
    "scheduler fairness load must not tune benchmark-only parameters in $scanned"
done

require_literal "$runner" 'scheduler_fairness_summary="$log_dir/x86_64-microkernel-fat32-$run_id.scheduler-fairness-load.summary"' \
  "scheduler fairness summary must stay under the configured log directory"

echo "x86_64 microkernel scheduler fairness load contract OK"
