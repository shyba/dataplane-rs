#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mnt_root="/home/user/mnt/dataplane"
log_dir="$mnt_root/logs"
tmp_dir="$mnt_root/tmp"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
run_log="$log_dir/x86_64-microkernel-service-route-table-$run_id.run.log"
summary="$log_dir/x86_64-microkernel-service-route-table-$run_id.summary"

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

validate_route_ledger() {
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
]
for marker in required:
    if marker not in serial:
        raise SystemExit(f"missing service route ledger marker: {marker}")
if "DPMK:SERVICE-ROUTE-TABLE-BEGIN" in serial:
    raise SystemExit("stale service route table marker unexpectedly present")
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

validate_route_ledger "$serial_log"

bad_serial="$tmp_dir/x86_64-microkernel-service-route-table-$run_id.bad.serial.log"
sed 's/DPMBOX:routes timer=1->1 cli_fs=2->3 fs_block=3->4 tcpip_net=6->5 http_fs=7->3 dhcp_net=8->5/DPMBOX:routes stale=1->9/' "$serial_log" >"$bad_serial"
if validate_route_ledger "$bad_serial" >/dev/null 2>&1; then
  fail "corrupt service route ledger was accepted"
fi

{
  echo "service_route_table_summary_status=pass"
  echo "service_route_table_run_id=$run_id"
  echo "service_route_table_model=service-mailbox-envelope-ledger"
  echo "service_route_table_run_log=$run_log"
  echo "service_route_table_serial_log=$serial_log"
  echo "service_route_table_qemu_log=$qemu_log"
  echo "service_route_table_client_log=$client_log"
  echo "service_route_table_pcap=$pcap"
  echo "service_route_table_pcap_bytes=$pcap_bytes"
  echo "service_route_table_route_rows=6"
  echo "service_route_table_task_rows=8"
  echo "service_route_table_request_rows=6"
  echo "service_route_table_shape_marker_ok=true"
  echo "service_route_table_tasks_marker_ok=true"
  echo "service_route_table_requests_marker_ok=true"
  echo "service_route_table_routes_marker_ok=true"
  echo "service_route_table_errors_marker_ok=true"
  echo "service_route_table_fault_destination_marker_ok=true"
  echo "service_route_table_corrupt_marker_rejected=true"
  echo "service_route_table_dynamic_discovery=false"
  echo "service_route_table_source_table_claim=false"
  echo "service_route_table_denied_route_runtime_claim=false"
  echo "service_route_table_stale_reply_runtime_claim=false"
  echo "service_route_table_restart=false"
  echo "service_route_table_replay=false"
  echo "service_route_table_tls=false"
  echo "service_route_table_https=false"
  echo "service_route_table_benchmark_result=false"
} >"$summary"

reject_duplicate_keys "$summary"

echo "x86_64 microkernel service route ledger proof passed."
echo "summary: $summary"
