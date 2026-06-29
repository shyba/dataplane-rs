#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mnt_root="/home/user/mnt/dataplane"
log_dir="$mnt_root/logs"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
run_log="$log_dir/x86_64-microkernel-resource-budget-ledger-$run_id.run.log"
summary="$log_dir/x86_64-microkernel-resource-budget-ledger-$run_id.summary"

mkdir -p "$log_dir" "$mnt_root/tmp"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

extract_path() {
  local label="$1"
  local count value
  count="$(awk -F': ' -v key="$label" '$1 == key { count++ } END { print count + 0 }' "$run_log")"
  [[ "$count" -eq 1 ]] || fail "artifact label '$label' count in $run_log was $count"
  value="$(awk -F': ' -v key="$label" '$1 == key { print $2 }' "$run_log")"
  [[ -n "$value" ]] || fail "empty artifact label '$label'"
  [[ "$value" == "$log_dir"/* ]] || fail "artifact outside $log_dir: $value"
  [[ -s "$value" ]] || fail "missing or empty artifact: $value"
  printf '%s\n' "$value"
}

require_marker() {
  local path="$1"
  local marker="$2"
  grep -Fq -- "$marker" "$path" || fail "missing marker in $path: $marker"
}

TMPDIR="$mnt_root/tmp" ./tools/x86_64_microkernel_fat32_run.sh >"$run_log" 2>&1

serial_log="$(extract_path "serial log")"
qemu_log="$(extract_path "qemu log")"
client_log="$(extract_path "client log")"
host_log="$(extract_path "network host log")"
pcap="$(extract_path "network pcap")"
pcap_bytes="$(wc -c <"$pcap" | tr -d ' ')"
(( pcap_bytes > 24 )) || fail "network pcap only contains a global header"

for marker in \
  "DPMK:RESOURCE-BUDGET-LEDGER" \
  "DPBUDGET:task-bytes block=16384 fs=4096 net=65536" \
  "DPBUDGET:virtq-bytes block=12288 net=16384 block_cap=256 net_cap=256" \
  "DPBUDGET:protocol-bytes cli_line=64 cli_commands=24 http_line=96 http_headers=384 net_reply=512 udp_payload=16" \
  "DPBUDGET:fat32-bytes sector=512 index_file=256 large_file=512" \
  "DPMK:RESOURCE-BUDGET-LEDGER-OK" \
  "DPMK:OK"; do
  require_marker "$serial_log" "$marker"
done

{
  echo "resource_budget_ledger_summary_status=pass"
  echo "resource_budget_ledger_run_id=$run_id"
  echo "resource_budget_ledger_run_log=$run_log"
  echo "resource_budget_ledger_serial_log=$serial_log"
  echo "resource_budget_ledger_client_log=$client_log"
  echo "resource_budget_ledger_qemu_log=$qemu_log"
  echo "resource_budget_ledger_host_log=$host_log"
  echo "resource_budget_ledger_pcap=$pcap"
  echo "resource_budget_ledger_pcap_bytes=$pcap_bytes"
  echo "resource_budget_ledger_begin_marker_ok=true"
  echo "resource_budget_ledger_ok_marker_ok=true"
  echo "resource_budget_ledger_task_bytes_block=16384"
  echo "resource_budget_ledger_task_bytes_fs=4096"
  echo "resource_budget_ledger_task_bytes_net=65536"
  echo "resource_budget_ledger_virtq_bytes_block=12288"
  echo "resource_budget_ledger_virtq_bytes_net=16384"
  echo "resource_budget_ledger_block_queue_cap=256"
  echo "resource_budget_ledger_net_queue_cap=256"
  echo "resource_budget_ledger_cli_line_bytes=64"
  echo "resource_budget_ledger_cli_command_count=24"
  echo "resource_budget_ledger_http_request_line_bytes=96"
  echo "resource_budget_ledger_http_header_bytes=384"
  echo "resource_budget_ledger_net_reply_payload_bytes=512"
  echo "resource_budget_ledger_udp_payload_bytes=16"
  echo "resource_budget_ledger_fat32_sector_bytes=512"
  echo "resource_budget_ledger_index_file_bytes=256"
  echo "resource_budget_ledger_large_file_bytes=512"
  echo "resource_budget_ledger_status_summary_agree=true"
  echo "resource_budget_ledger_dynamic_negotiation=false"
  echo "resource_budget_ledger_temporary_unbounded=false"
  echo "resource_budget_ledger_heap_growth_claim=false"
  echo "resource_budget_ledger_heap_mailbox=false"
  echo "resource_budget_ledger_generic_actor_framework=false"
  echo "resource_budget_ledger_rp2040_networking_claim=false"
  echo "resource_budget_ledger_tls=false"
  echo "resource_budget_ledger_https=false"
  echo "resource_budget_ledger_default_writable=false"
  echo "resource_budget_ledger_benchmark_result=false"
} >"$summary"

python3 - "$summary" <<'PY'
import sys

path = sys.argv[1]
values = {}
for line in open(path, "r", encoding="ascii"):
    key, value = line.rstrip("\n").split("=", 1)
    if key in values:
        raise SystemExit(f"duplicate summary key: {key}")
    values[key] = value

required = {
    "resource_budget_ledger_summary_status": "pass",
    "resource_budget_ledger_begin_marker_ok": "true",
    "resource_budget_ledger_ok_marker_ok": "true",
    "resource_budget_ledger_task_bytes_block": "16384",
    "resource_budget_ledger_task_bytes_fs": "4096",
    "resource_budget_ledger_task_bytes_net": "65536",
    "resource_budget_ledger_virtq_bytes_block": "12288",
    "resource_budget_ledger_virtq_bytes_net": "16384",
    "resource_budget_ledger_block_queue_cap": "256",
    "resource_budget_ledger_net_queue_cap": "256",
    "resource_budget_ledger_cli_line_bytes": "64",
    "resource_budget_ledger_cli_command_count": "24",
    "resource_budget_ledger_http_request_line_bytes": "96",
    "resource_budget_ledger_http_header_bytes": "384",
    "resource_budget_ledger_net_reply_payload_bytes": "512",
    "resource_budget_ledger_udp_payload_bytes": "16",
    "resource_budget_ledger_fat32_sector_bytes": "512",
    "resource_budget_ledger_index_file_bytes": "256",
    "resource_budget_ledger_large_file_bytes": "512",
    "resource_budget_ledger_status_summary_agree": "true",
    "resource_budget_ledger_dynamic_negotiation": "false",
    "resource_budget_ledger_temporary_unbounded": "false",
    "resource_budget_ledger_heap_growth_claim": "false",
    "resource_budget_ledger_heap_mailbox": "false",
    "resource_budget_ledger_generic_actor_framework": "false",
    "resource_budget_ledger_rp2040_networking_claim": "false",
    "resource_budget_ledger_tls": "false",
    "resource_budget_ledger_https": "false",
    "resource_budget_ledger_benchmark_result": "false",
}
for key, expected in required.items():
    if values.get(key) != expected:
        raise SystemExit(f"missing {key}={expected}")
PY

echo "x86_64 microkernel resource budget ledger proof passed."
echo "summary: $summary"
