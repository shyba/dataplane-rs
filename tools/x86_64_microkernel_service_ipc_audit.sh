#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mnt_root="/home/user/mnt/dataplane"
log_dir="$mnt_root/logs"
tmp_dir="$mnt_root/tmp"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
run_log="$log_dir/x86_64-microkernel-service-ipc-audit-$run_id.run.log"
summary="$log_dir/x86_64-microkernel-service-ipc-audit-$run_id.summary"

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

validate_ipc_ledger() {
  local serial_log="$1"
  python3 - "$serial_log" <<'PY'
import sys
from pathlib import Path

serial = Path(sys.argv[1]).read_text(encoding="ascii")
required = [
    "DPMK:SERVICE-MAILBOX-ENVELOPE-LEDGER",
    "DPMBOX:shape fields=from,to,request,capability,body inline_bytes=32 mailbox_cap=4 heap=0 dyn_dispatch=0 generic_actor=0",
    "DPMBOX:tasks timer=1 cli=2 fs=3 block=4 net=5 tcpip=6 http=7 dhcp=8",
    "DPMBOX:requests timer_tick=1 cli_fs=2 fs_block=3 tcpip_net=4 http_fs=5 dhcp_net=6",
    "DPMBOX:routes timer=1->1 cli_fs=2->3 fs_block=3->4 tcpip_net=6->5 http_fs=7->3 dhcp_net=8->5",
    "DPMBOX:errors queue_full=MailboxError::Full empty=MailboxError::Empty send=mailbox-send recv=mailbox-recv timeout=deferred stale_reply=deferred denied_route=deferred",
    "DPMBOX:fault-destination task=4 status=faulted restart=0 replay=0",
    "DPMK:SERVICE-MAILBOX-ENVELOPE-OK",
    "DPMK:IPC-OK",
]
for marker in required:
    if marker not in serial:
        raise SystemExit(f"missing service IPC audit marker: {marker}")
for stale in (
    "DPMK:IPC-DENIED-ROUTE-OK",
    "DPMK:IPC-STALE-REPLY-OK",
    "DPMK:IPC-RECOVERY-AFTER-FAILURE-OK",
    "DPMK:SERVICE-IPC-AUDIT-OK",
    "DPMK:SERVICE-ROUTE-TABLE-BEGIN",
):
    if stale in serial:
        raise SystemExit(f"stale service IPC proof marker unexpectedly present: {stale}")
PY
}

reject_duplicate_keys() {
  local file="$1"
  local duplicate
  duplicate="$(awk -F= '/^[A-Za-z0-9_]+=/ { print $1 }' "$file" | sort | uniq -d)"
  [[ -z "$duplicate" ]] || fail "duplicate summary key(s) in $file: $duplicate"
}

TMPDIR="$tmp_dir" ./tools/x86_64_microkernel_fat32_run.sh >"$run_log" 2>&1
serial_log="$(extract_label_path "$run_log" "serial log")"
qemu_log="$(extract_label_path "$run_log" "qemu log")"
client_log="$(extract_label_path "$run_log" "client log")"
pcap="$(extract_label_path "$run_log" "network pcap")"
pcap_bytes="$(wc -c <"$pcap" | tr -d ' ')"
(( pcap_bytes > 24 )) || fail "network pcap only contains a global header"

validate_ipc_ledger "$serial_log"

bad_serial="$tmp_dir/x86_64-microkernel-service-ipc-audit-$run_id.bad.serial.log"
sed 's/DPMK:IPC-OK/DPMK:IPC-STALE/' "$serial_log" >"$bad_serial"
if validate_ipc_ledger "$bad_serial" >/dev/null 2>&1; then
  fail "corrupt service IPC ledger was accepted"
fi

{
  echo "service_ipc_audit_summary_status=pass"
  echo "service_ipc_audit_run_id=$run_id"
  echo "service_ipc_audit_model=service-mailbox-envelope-ledger"
  echo "service_ipc_audit_run_log=$run_log"
  echo "service_ipc_audit_serial_log=$serial_log"
  echo "service_ipc_audit_qemu_log=$qemu_log"
  echo "service_ipc_audit_client_log=$client_log"
  echo "service_ipc_audit_pcap=$pcap"
  echo "service_ipc_audit_pcap_bytes=$pcap_bytes"
  echo "service_ipc_audit_ipc_marker_ok=true"
  echo "service_ipc_audit_mailbox_shape_ok=true"
  echo "service_ipc_audit_route_ledger_ok=true"
  echo "service_ipc_audit_error_ledger_ok=true"
  echo "service_ipc_audit_corrupt_marker_rejected=true"
  echo "service_ipc_audit_send_error_name=mailbox-send"
  echo "service_ipc_audit_recv_error_name=mailbox-recv"
  echo "service_ipc_audit_queue_full_policy=MailboxError::Full"
  echo "service_ipc_audit_timeout_runtime_claim=false"
  echo "service_ipc_audit_stale_reply_runtime_claim=false"
  echo "service_ipc_audit_denied_route_runtime_claim=false"
  echo "service_ipc_audit_recovery_runtime_claim=false"
  echo "service_ipc_audit_source_table_claim=false"
  echo "service_ipc_audit_generic_ipc=false"
  echo "service_ipc_audit_heap_mailbox=false"
  echo "service_ipc_audit_actor_framework=false"
  echo "service_ipc_audit_restart=false"
  echo "service_ipc_audit_replay=false"
  echo "service_ipc_audit_tls=false"
  echo "service_ipc_audit_https=false"
  echo "service_ipc_audit_benchmark_result=false"
  echo "service_ipc_audit_hardware_readiness=false"
  echo "service_ipc_audit_debugger=false"
  echo "service_ipc_audit_raw_memory=false"
  echo "service_ipc_audit_mmio=false"
  echo "service_ipc_audit_page_table=false"
} >"$summary"

reject_duplicate_keys "$summary"

echo "x86_64 microkernel service IPC ledger proof passed."
echo "summary: $summary"
