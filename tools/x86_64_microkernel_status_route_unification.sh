#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mnt_root="/home/user/mnt/dataplane"
log_dir="$mnt_root/logs"
tmp_dir="$mnt_root/tmp"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
run_log="$log_dir/x86_64-microkernel-status-route-unification-$run_id.run.log"
summary="$log_dir/x86_64-microkernel-status-route-unification-$run_id.summary"

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

validate_route_compatibility() {
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
    "DPMK:SERVICE-MAILBOX-ENVELOPE-OK",
    "DPMK:IPC-OK",
]
for marker in required:
    if marker not in serial:
        raise SystemExit(f"missing status route compatibility marker: {marker}")
stale_markers = (
    "DPMK:STATUS" + "-ROUTE-UNIFICATION-SOURCE:" + "SERVICE_" + "ROUTE_TABLE",
    "DPSTATUS:ROUTE" + "-TABLE",
    "DPMK:STATUS" + "-ROUTE-UNIFICATION-OK",
)
for stale in stale_markers:
    if stale in serial:
        raise SystemExit(f"stale status route marker unexpectedly present: {stale}")
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

validate_route_compatibility "$serial_log"

bad_serial="$tmp_dir/x86_64-microkernel-status-route-unification-$run_id.bad.serial.log"
sed 's/DPMBOX:routes timer=1->1 cli_fs=2->3 fs_block=3->4 tcpip_net=6->5 http_fs=7->3 dhcp_net=8->5/DPMBOX:routes stale=1->9/' "$serial_log" >"$bad_serial"
if validate_route_compatibility "$bad_serial" >/dev/null 2>&1; then
  fail "corrupt status route compatibility ledger was accepted"
fi

{
  echo "status_route_unification_summary_status=pass"
  echo "status_route_unification_run_id=$run_id"
  echo "status_route_unification_model=deferred-mailbox-route-ledger-compatibility"
  echo "status_route_unification_run_log=$run_log"
  echo "status_route_unification_serial_log=$serial_log"
  echo "status_route_unification_qemu_log=$qemu_log"
  echo "status_route_unification_client_log=$client_log"
  echo "status_route_unification_pcap=$pcap"
  echo "status_route_unification_pcap_bytes=$pcap_bytes"
  echo "status_route_unification_route_ledger_ok=true"
  echo "status_route_unification_ipc_marker_ok=true"
  echo "status_route_unification_corrupt_marker_rejected=true"
  echo "status_route_unification_status_surface_present=false"
  echo "status_route_unification_cli_http_same_body_claim=false"
  echo "status_route_unification_source_table_claim=false"
  echo "status_route_unification_route_probe_claim=false"
  echo "status_route_unification_runner_mode_claim=false"
  echo "status_route_unification_restart=false"
  echo "status_route_unification_replay=false"
  echo "status_route_unification_tls=false"
  echo "status_route_unification_https=false"
  echo "status_route_unification_benchmark_result=false"
} >"$summary"

reject_duplicate_keys "$summary"

echo "x86_64 microkernel status route compatibility proof passed."
echo "summary: $summary"
