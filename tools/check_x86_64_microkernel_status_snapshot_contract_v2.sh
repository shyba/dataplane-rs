#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

runner="tools/x86_64_microkernel_fat32_run.sh"
matrix_runner="tools/x86_64_microkernel_validation_matrix_run.sh"
matrix_guard="tools/check_x86_64_microkernel_validation_matrix_contract.sh"
makefile="Makefile"
guest="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"

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
  local matches status
  set +e
  matches="$(rg -n -- "$regex" "$file")"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    printf '%s\n' "$matches" >&2
    fail "$note"
  fi
  [[ "$status" -eq 1 ]] || fail "could not scan $file for forbidden regex: $regex"
}

for file in "$runner" "$matrix_runner" "$matrix_guard" "$makefile" "$guest"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel Status Snapshot Contract V2 Guard ==="

for literal in \
  'status_snapshot_v2_summary="$log_dir/x86_64-microkernel-fat32-$run_id.status-snapshot-v2.summary"' \
  'DP_MICROKERNEL_STATUS_SNAPSHOT_V2_PROOF' \
  '--status-snapshot-v2-proof' \
  'DP_STATUS_SNAPSHOT_V2_PROOF' \
  'status_snapshot_v2 = os.environ["DP_STATUS_SNAPSHOT_V2_PROOF"] == "1"' \
  'status_snapshot_v2_checks = {' \
  'DPMK:STATUS-SNAPSHOT-V2-BEGIN' \
  'DPMK:STATUS-SNAPSHOT-V2-SOURCES:cli,http,status-service' \
  'DPMK:STATUS-SNAPSHOT-V2-BOUNDED-BYTES:' \
  'DPMK:STATUS-SNAPSHOT-V2-FIELDS:task-polls,queue-peaks,timer-ticks,filesystem,storage,http,network,faults,route-table,no-restart' \
  'DPMK:STATUS-SNAPSHOT-V2-CLI-HTTP-SAME-BODY-OK' \
  'DPMK:STATUS-SNAPSHOT-V2-INCLUDES-TASKS-OK' \
  'DPMK:STATUS-SNAPSHOT-V2-INCLUDES-COUNTERS-OK' \
  'DPMK:STATUS-SNAPSHOT-V2-INCLUDES-FAULTS-OK' \
  'DPMK:STATUS-SNAPSHOT-V2-FIELD-AGREEMENT-OK' \
  'DPMK:STATUS-SNAPSHOT-V2-ROUTE-EVIDENCE-OK' \
  'DPMK:STATUS-SNAPSHOT-V2-NO-RESTART-OK' \
  'DPMK:STATUS-SNAPSHOT-V2-OK' \
  'if status_snapshot_v2 and not all(status_snapshot_v2_checks.values()):' \
  'raise SystemExit("status snapshot v2 evidence missing")' \
  'status_snapshot_v2_summary_status=pass' \
  'status_snapshot_v2_fields_ok=true' \
  'status_snapshot_v2_field_agreement_ok=true' \
  'status_snapshot_v2_route_evidence_ok=true' \
  'grep '\''^status_snapshot_v2_'\'' "$client_log"' \
  'status_snapshot_v2_client_log=$client_log' \
  'status_snapshot_v2_serial_log=$serial_log' \
  'status_snapshot_v2_qemu_log=$qemu_log' \
  'status_snapshot_v2_network_pcap=$net_pcap' \
  'x86_64 microkernel status snapshot contract v2 proof passed.' \
  'status snapshot v2 summary: $status_snapshot_v2_summary'; do
  require_literal "$runner" "$literal" "runner must contain status snapshot v2 literal: $literal"
done

for literal in \
  'struct OperatorStatusSnapshot' \
  'task_polls: u32' \
  'queue_peak_bound: u32' \
  'timer_ticks: u32' \
  'route_count: u32' \
  'route_queue_capacity: u32' \
  'fn validate_status_snapshot_v2' \
  'DPMK:STATUS-SNAPSHOT-V2-SOURCES:cli,http,status-service' \
  'DPMK:STATUS-SNAPSHOT-V2-BOUNDED-BYTES:' \
  'DPMK:STATUS-SNAPSHOT-V2-FIELDS:task-polls,queue-peaks,timer-ticks,filesystem,storage,http,network,faults,route-table,no-restart' \
  'DPMK:STATUS-SNAPSHOT-V2-CLI-HTTP-SAME-BODY-OK' \
  'DPMK:STATUS-SNAPSHOT-V2-FIELD-AGREEMENT-OK' \
  'DPMK:STATUS-SNAPSHOT-V2-ROUTE-EVIDENCE-OK' \
  'DPMK:STATUS-SNAPSHOT-V2-NO-RESTART-OK' \
  'DPMK:STATUS-SNAPSHOT-V2-OK' \
  'task_polls=' \
  'timer_ticks=' \
  'DPSTATUS:QUEUES total=' \
  'peak_bound=4' \
  'DPSTATUS:ROUTE-TABLE routes=13' \
  'no_restart=1'; do
  require_literal "$guest" "$literal" "guest must contain status snapshot v2 literal: $literal"
done

for literal in \
  'status-snapshot-contract-v2' \
  "status-snapshot-contract-v2) echo 'make x86_64-microkernel-status-snapshot-contract-v2' ;;" \
  "status-snapshot-contract-v2) echo 'x86_64 microkernel status snapshot contract v2 proof passed.' ;;" \
  "status-snapshot-contract-v2) echo 'status snapshot v2 summary|client log|serial log|qemu log|network pcap' ;;"; do
  require_literal "$matrix_runner" "$literal" "validation matrix must contain status snapshot v2 literal: $literal"
done

for literal in \
  'x86_64-microkernel-status-snapshot-contract-v2-contract:' \
  './tools/check_x86_64_microkernel_status_snapshot_contract_v2.sh' \
  'x86_64-microkernel-status-snapshot-contract-v2: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-status-snapshot-consistency-contract x86_64-microkernel-status-snapshot-contract-v2-contract' \
  'DP_MICROKERNEL_STATUS_SNAPSHOT_V2_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --status-snapshot-v2-proof'; do
  require_literal "$makefile" "$literal" "Makefile must contain status snapshot v2 literal: $literal"
done

for literal in \
  'x86_64-microkernel-status-snapshot-contract-v2-contract:' \
  "status-snapshot-contract-v2) echo 'make x86_64-microkernel-status-snapshot-contract-v2' ;;" \
  'status_snapshot_v2_summary_status=pass'; do
  require_literal "$matrix_guard" "$literal" "validation-matrix guard must cover status snapshot v2 literal: $literal"
done

reject_regex "$runner" 'status[-_ ]snapshot[-_ ]v2.*(curl[[:space:]]+-k|https://|openssl|rustls|embedded-tls|webpki|aws-lc-rs|STRICT_FIVE_CALIBRATION|benchmark[[:space:]]+tuning|retun(e|ing))' \
  "status snapshot v2 runner must not reopen deferred network security work or retune benchmarks"
reject_regex "$runner" 'status[-_ ]snapshot[-_ ]v2.*(DPCLI:(RESTART|RESET|SHELL)|DPMK:(RESTART|RESET-COMMAND)|debugger|raw[[:space:]]+memory|MMIO[[:space:]]+dump|page-table[[:space:]]+dump|script[[:space:]]+engine|serde_json|GenericSocket|SocketApi|heap-backed[[:space:]]+mailbox|hardware-ready|hardware readiness)' \
  "status snapshot v2 runner must not add restart/reset/debug/raw/hardware or generic IPC surfaces"
reject_regex "$matrix_runner" 'status[-_ ]snapshot[-_ ]contract[-_ ]v2.*(curl[[:space:]]+-k|https://|openssl|rustls|embedded-tls|webpki|aws-lc-rs|STRICT_FIVE_CALIBRATION|benchmark[[:space:]]+tuning|retun(e|ing)|hardware-ready|hardware readiness)' \
  "status snapshot v2 matrix wiring must not reopen deferred or hardware-readiness work"

echo "x86_64 microkernel status snapshot contract v2 guard OK"
