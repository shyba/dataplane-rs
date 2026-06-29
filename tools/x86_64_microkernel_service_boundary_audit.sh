#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mnt_root="/home/user/mnt/dataplane"
log_dir="$mnt_root/logs"
tmp_dir="$mnt_root/tmp"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
run_log="$log_dir/x86_64-microkernel-service-boundary-audit-$run_id.run.log"
summary="$log_dir/x86_64-microkernel-service-boundary-audit-$run_id.summary"

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

validate_boundary_ledger() {
  local serial_log="$1"
  python3 - "$serial_log" <<'PY'
import sys
from pathlib import Path

serial = Path(sys.argv[1]).read_text(encoding="ascii")
required = [
    "DPMK:SERVICE-MAILBOX-ENVELOPE-LEDGER",
    "DPMBOX:shape fields=from,to,request,capability,body inline_bytes=32 mailbox_cap=4 heap=0 dyn_dispatch=0 generic_actor=0",
    "DPMBOX:tasks timer=1 cli=2 fs=3 block=4 net=5 tcpip=6 http=7 dhcp=8",
    "DPMBOX:routes timer=1->1 cli_fs=2->3 fs_block=3->4 tcpip_net=6->5 http_fs=7->3 dhcp_net=8->5",
    "DPMBOX:errors queue_full=MailboxError::Full empty=MailboxError::Empty send=mailbox-send recv=mailbox-recv timeout=deferred stale_reply=deferred denied_route=deferred",
    "DPMBOX:fault-destination task=4 status=faulted restart=0 replay=0",
    "DPMK:SERVICE-MAILBOX-ENVELOPE-OK",
    "DPMK:SERVICE-LIFECYCLE-LEDGER",
    "DPLIFE:ready timer=ready cli=ready fs=ready block=ready net=ready tcpip=ready http=ready dhcp=ready",
    "DPLIFE:fault-path task=4 status=faulted faults=1 marker=DPMK:FAULT-CONTAINED",
    "DPLIFE:deferred stopped=not-exercised degraded=not-defined restart=0 replay=0 cleanup_guarantee=0",
    "DPMK:SERVICE-LIFECYCLE-LEDGER-OK",
    "DPLIFE:task block id=4 endpoint=4 status=faulted faults=1 restart=0 replay=0 cleanup_guarantee=0",
    "DPMK:SERVICE-LIFECYCLE-FAULT-OK",
    "DPMK:IPC-OK",
    "DPMK:HTTP-POLICY-200",
    "DPMK:HTTP-POLICY-404",
    "DPMK:HTTP-POLICY-405",
    "DPMK:HTTP-POLICY-413",
    "DPMK:HTTP-POLICY-500",
    "DPMK:HTTP-STATUS-200:2",
    "DPMK:HTTP-STATUS-404:1",
    "DPMK:HTTP-STATUS-405:1",
    "DPMK:HTTP-STATUS-413:2",
    "DPMK:HTTP-STATUS-500:1",
    "DPMK:HTTP-PARSER-MALFORMED:1",
    "DPMK:HTTP-PARSER-LINE-TOO-LONG:1",
    "DPMK:HTTP-POLICY-OK",
]
for marker in required:
    if marker not in serial:
        raise SystemExit(f"missing service boundary marker: {marker}")
stale_markers = (
    "DPMK:STATUS-ROUTE-UNIFICATION-SOURCE:" + "SERVICE_" + "ROUTE_TABLE",
    "DPSTATUS:ROUTE" + "-TABLE",
    "DPMK:SERVICE" + "-ROUTE-TABLE-BEGIN",
)
for stale in stale_markers:
    if stale in serial:
        raise SystemExit(f"stale service boundary marker unexpectedly present: {stale}")
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

validate_boundary_ledger "$serial_log"

bad_serial="$tmp_dir/x86_64-microkernel-service-boundary-audit-$run_id.bad.serial.log"
sed 's/DPMK:HTTP-POLICY-OK/DPMK:HTTP-POLICY-STALE/' "$serial_log" >"$bad_serial"
if validate_boundary_ledger "$bad_serial" >/dev/null 2>&1; then
  fail "corrupt service boundary ledger was accepted"
fi

{
  echo "service_boundary_audit_summary_status=pass"
  echo "service_boundary_audit_run_id=$run_id"
  echo "service_boundary_audit_model=mailbox-route-ipc-lifecycle-http-ledger"
  echo "service_boundary_audit_run_log=$run_log"
  echo "service_boundary_audit_serial_log=$serial_log"
  echo "service_boundary_audit_qemu_log=$qemu_log"
  echo "service_boundary_audit_client_log=$client_log"
  echo "service_boundary_audit_pcap=$pcap"
  echo "service_boundary_audit_pcap_bytes=$pcap_bytes"
  echo "service_boundary_audit_mailbox_ledger_ok=true"
  echo "service_boundary_audit_route_ledger_ok=true"
  echo "service_boundary_audit_ipc_ledger_ok=true"
  echo "service_boundary_audit_lifecycle_ledger_ok=true"
  echo "service_boundary_audit_http_policy_ok=true"
  echo "service_boundary_audit_http_status_rows=5"
  echo "service_boundary_audit_http_parser_rows=2"
  echo "service_boundary_audit_fault_boundary_ok=true"
  echo "service_boundary_audit_corrupt_marker_rejected=true"
  echo "service_boundary_audit_source_table_claim=false"
  echo "service_boundary_audit_status_route_unification_claim=false"
  echo "service_boundary_audit_denied_route_runtime_claim=false"
  echo "service_boundary_audit_stale_reply_runtime_claim=false"
  echo "service_boundary_audit_restart=false"
  echo "service_boundary_audit_replay=false"
  echo "service_boundary_audit_tls=false"
  echo "service_boundary_audit_https=false"
  echo "service_boundary_audit_benchmark_result=false"
} >"$summary"

reject_duplicate_keys "$summary"

echo "x86_64 microkernel service boundary audit proof passed."
echo "summary: $summary"
