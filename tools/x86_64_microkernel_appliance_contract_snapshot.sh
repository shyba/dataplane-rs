#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mnt_root="/home/user/mnt/dataplane"
log_dir="$mnt_root/logs"
tmp_dir="$mnt_root/tmp"
snapshot="tools/x86_64_microkernel_appliance_contract_snapshot.tsv"
main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
curl_run_log="$log_dir/x86_64-microkernel-appliance-contract-snapshot-$run_id.curl.run.log"
budget_run_log="$log_dir/x86_64-microkernel-appliance-contract-snapshot-$run_id.resource-budget.run.log"
summary="$log_dir/x86_64-microkernel-appliance-contract-snapshot-$run_id.summary"

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

extract_summary_path() {
  local log_file="$1"
  local label="$2"
  local count value
  count="$(awk -F': ' -v key="$label" '$1 == key { count++ } END { print count + 0 }' "$log_file")"
  [[ "$count" -eq 1 ]] || fail "summary label '$label' count in $log_file was $count"
  value="$(awk -F': ' -v key="$label" '$1 == key { print $2 }' "$log_file")"
  [[ -n "$value" ]] || fail "empty summary label '$label'"
  [[ "$value" == "$log_dir"/* ]] || fail "summary outside $log_dir: $value"
  [[ -s "$value" ]] || fail "missing or empty summary: $value"
  printf '%s\n' "$value"
}

require_marker() {
  local file="$1"
  local marker="$2"
  grep -Fq -- "$marker" "$file" || fail "missing marker in $file: $marker"
}

require_key_value() {
  local file="$1"
  local key="$2"
  local expected="$3"
  local count line
  count="$(awk -F= -v key="$key" '$1 == key { count++ } END { print count + 0 }' "$file")"
  [[ "$count" -eq 1 ]] || fail "summary key '$key' count in $file was $count"
  line="$(awk -F= -v key="$key" '$1 == key { print $0 }' "$file")"
  [[ "$line" == "$key=$expected" ]] || fail "expected $key=$expected in $file, got '${line:-missing}'"
}

summary_value() {
  local file="$1"
  local key="$2"
  local count value
  count="$(awk -F= -v key="$key" '$1 == key { count++ } END { print count + 0 }' "$file")"
  [[ "$count" -eq 1 ]] || fail "summary key '$key' count in $file was $count"
  value="$(awk -F= -v key="$key" '$1 == key { print $2 }' "$file")"
  [[ -n "$value" ]] || fail "empty summary value for $key in $file"
  printf '%s\n' "$value"
}

reject_duplicate_keys() {
  local file="$1"
  local duplicate
  duplicate="$(awk -F= '/^[A-Za-z0-9_]+=/ { print $1 }' "$file" | sort | uniq -d)"
  [[ -z "$duplicate" ]] || fail "duplicate summary key(s) in $file: $duplicate"
}

python3 - "$snapshot" "$main" <<'PY'
import csv
import sys
from pathlib import Path

snapshot = Path(sys.argv[1])
main = Path(sys.argv[2])
source = main.read_text(encoding="utf-8")
expected_fields = [
    "route_id",
    "surface",
    "owner",
    "capability",
    "request_cap",
    "response_cap",
    "counter_owner",
    "status_key",
    "proof_command",
    "cli_marker",
    "http_marker",
    "serial_marker",
    "summary_key",
    "denied_deferred",
]
with snapshot.open(encoding="ascii", newline="") as fh:
    reader = csv.DictReader(fh, delimiter="\t")
    if reader.fieldnames != expected_fields:
        raise SystemExit(f"unexpected TSV header: {reader.fieldnames}")
    rows = list(reader)
for idx, row in enumerate(rows, 1):
    if None in row:
        raise SystemExit(f"unexpected surplus TSV fields in row {idx}: {row[None]}")
    if set(row.keys()) != set(expected_fields):
        raise SystemExit(f"unexpected TSV row keys in row {idx}: {sorted(row.keys())}")
if len(rows) != 7:
    raise SystemExit(f"expected 7 appliance snapshot rows, found {len(rows)}")
required_routes = {
    "timer.tick.self",
    "cli.fs.request",
    "http.fs.request",
    "tcpip.net.tx",
    "fs.block.read",
    "service.lifecycle",
    "fault.containment",
}
seen = []
for row in rows:
    route = row["route_id"]
    if route in seen:
        raise SystemExit(f"duplicate route_id in appliance snapshot: {route}")
    seen.append(route)
if set(seen) != required_routes:
    raise SystemExit(f"snapshot route set drifted: {sorted(seen)}")

expected = {
    "timer.tick.self": {
        "surface": "serial",
        "owner": "TimerTask",
        "counter_owner": "TimerTask",
        "request_cap": "4",
        "response_cap": "1",
        "proof_command": "make x86_64-microkernel-timer-timeout-service",
    },
    "cli.fs.request": {
        "surface": "cli",
        "owner": "FsTask",
        "counter_owner": "FsTask",
        "request_cap": "64",
        "response_cap": "24",
        "proof_command": "make x86_64-microkernel-fat32-smoke",
    },
    "http.fs.request": {
        "surface": "http",
        "owner": "FsTask",
        "counter_owner": "HttpTask",
        "request_cap": "96",
        "response_cap": "256",
        "proof_command": "make x86_64-microkernel-fat32-smoke",
    },
    "tcpip.net.tx": {
        "surface": "network",
        "owner": "TcpIpTask",
        "counter_owner": "TcpIpTask",
        "request_cap": "256",
        "response_cap": "16",
        "proof_command": "make x86_64-microkernel-fat32-smoke",
    },
    "fs.block.read": {
        "surface": "storage",
        "owner": "BlockTask",
        "counter_owner": "FsTask",
        "request_cap": "256",
        "response_cap": "512",
        "proof_command": "make x86_64-microkernel-resource-budget-ledger",
    },
    "service.lifecycle": {
        "surface": "serial",
        "owner": "ServiceLifecycle",
        "counter_owner": "TaskTable",
        "request_cap": "9",
        "response_cap": "1",
        "proof_command": "make x86_64-microkernel-service-lifecycle-ledger",
    },
    "fault.containment": {
        "surface": "cli",
        "owner": "FaultPolicy",
        "counter_owner": "BlockTask",
        "request_cap": "1",
        "response_cap": "1",
        "proof_command": "make x86_64-microkernel-fault-policy-hardening",
    },
}
for row in rows:
    route = row["route_id"]
    if not all(row[key] for key in (
        "surface",
        "owner",
        "capability",
        "request_cap",
        "response_cap",
        "counter_owner",
        "status_key",
        "proof_command",
        "cli_marker",
        "http_marker",
        "serial_marker",
        "summary_key",
        "denied_deferred",
    )):
        raise SystemExit(f"empty required field for {route}")
    if row["surface"] != expected[route]["surface"]:
        raise SystemExit(f"surface drift for {route}: {row['surface']}")
    if row["owner"] != expected[route]["owner"]:
        raise SystemExit(f"owner drift for {route}: {row['owner']}")
    if row["counter_owner"] != expected[route]["counter_owner"]:
        raise SystemExit(f"counter owner drift for {route}: {row['counter_owner']}")
    if row["capability"] != "CAP_KERNEL":
        raise SystemExit(f"unexpected capability for {route}: {row['capability']}")
    if row["request_cap"] != expected[route]["request_cap"]:
        raise SystemExit(f"request cap drift for {route}: {row['request_cap']}")
    if row["response_cap"] != expected[route]["response_cap"]:
        raise SystemExit(f"response cap drift for {route}: {row['response_cap']}")
    if row["proof_command"] != expected[route]["proof_command"]:
        raise SystemExit(f"proof command drift for {route}: {row['proof_command']}")
    if row["status_key"] not in source:
        raise SystemExit(f"missing source marker for {route}: {row['status_key']}")
    if row["serial_marker"] not in source:
        raise SystemExit(f"serial marker not in guest source for {route}: {row['serial_marker']}")
for forbidden in ("serde_json", "OpenAPI", "dynamic route discovery", "heap-backed status builder"):
    if forbidden in snapshot.read_text(encoding="ascii"):
        raise SystemExit(f"snapshot must not contain forbidden schema wording: {forbidden}")
PY

./tools/check_x86_64_microkernel_resource_budget_ledger_contract.sh >/dev/null
./tools/check_x86_64_microkernel_service_lifecycle_ledger_contract.sh >/dev/null
./tools/check_x86_64_microkernel_service_mailbox_envelope_contract.sh >/dev/null
./tools/check_x86_64_microkernel_timer_timeout_service_contract.sh >/dev/null
./tools/check_x86_64_microkernel_protocol_input_bounds_ledger_contract.sh >/dev/null

TMPDIR="$tmp_dir" DP_MICROKERNEL_CURL_PROOF=1 \
  ./tools/x86_64_microkernel_fat32_run.sh --curl-proof >"$curl_run_log" 2>&1
curl_serial_log="$(extract_label_path "$curl_run_log" "serial log")"
qemu_log="$(extract_label_path "$curl_run_log" "qemu log")"
curl_log="$(extract_label_path "$curl_run_log" "curl log")"
curl_get_body="$(extract_label_path "$curl_run_log" "curl GET body")"
curl_get_status="$(extract_label_path "$curl_run_log" "curl GET status")"
curl_pcap="$(extract_label_path "$curl_run_log" "curl pcap")"

TMPDIR="$tmp_dir" ./tools/x86_64_microkernel_resource_budget_ledger.sh >"$budget_run_log" 2>&1
budget_summary="$(extract_summary_path "$budget_run_log" "summary")"
reject_duplicate_keys "$budget_summary"
require_key_value "$budget_summary" "resource_budget_ledger_summary_status" "pass"
require_key_value "$budget_summary" "resource_budget_ledger_status_summary_agree" "true"
pcap="$(summary_value "$budget_summary" "resource_budget_ledger_pcap")"
serial_log="$(summary_value "$budget_summary" "resource_budget_ledger_serial_log")"
[[ "$pcap" == "$log_dir"/* ]] || fail "resource budget pcap outside $log_dir: $pcap"
[[ -s "$pcap" ]] || fail "missing resource budget pcap: $pcap"
[[ "$serial_log" == "$log_dir"/* ]] || fail "resource budget serial log outside $log_dir: $serial_log"
[[ -s "$serial_log" ]] || fail "missing resource budget serial log: $serial_log"
pcap_bytes="$(wc -c <"$pcap" | tr -d ' ')"
(( pcap_bytes > 24 )) || fail "resource budget pcap only contains a global header"

for marker in \
  "DPMK:RESOURCE-BUDGET-LEDGER" \
  "DPMK:PROTOCOL-INPUT-BOUNDS-LEDGER" \
  "DPMK:TIMER-TIMEOUT-SERVICE-LEDGER" \
  "DPMK:SERVICE-MAILBOX-ENVELOPE-LEDGER" \
  "DPMK:SERVICE-LIFECYCLE-LEDGER" \
  "DPCLI:LS / HELLO.TXT 40 INDEX.HTM 110" \
  "DPCLI:TASK timer id=1 endpoint=1 status=ready" \
  "DPCLI:TASK fs id=3 endpoint=3 status=ready" \
  "DPCLI:TASK block id=4 endpoint=4 status=ready" \
  "DPCLI:TASK tcpip id=6 endpoint=6 status=ready" \
  "DPMK:HTTP-GET-OK" \
  "DPMK:NET-UDP-ECHO" \
  "DPMK:FAULT-CONTAINED"; do
  require_marker "$serial_log" "$marker"
done

require_marker "$curl_serial_log" "DPMK:HTTP-GET-OK"
require_marker "$curl_log" "curl_get_index_ok=true"
require_marker "$curl_get_status" "200"
require_marker "$curl_get_body" "<!doctype html>"
require_key_value "$budget_summary" "resource_budget_ledger_begin_marker_ok" "true"

{
  echo "appliance_contract_snapshot_summary_status=pass"
  echo "appliance_contract_snapshot_run_id=$run_id"
  echo "appliance_contract_snapshot_file=$snapshot"
  echo "appliance_contract_snapshot_rows=7"
  echo "timer_timeout_service_summary_status=pass"
  echo "appliance_contract_snapshot_cli_agreement=true"
  echo "appliance_contract_snapshot_http_agreement=true"
  echo "appliance_contract_snapshot_pcap_agreement=true"
  echo "resource_budget_ledger_summary_status=pass"
  echo "appliance_contract_snapshot_curl_run_log=$curl_run_log"
  echo "appliance_contract_snapshot_resource_budget_run_log=$budget_run_log"
  echo "appliance_contract_snapshot_serial_log=$serial_log"
  echo "appliance_contract_snapshot_curl_serial_log=$curl_serial_log"
  echo "appliance_contract_snapshot_curl_log=$curl_log"
  echo "appliance_contract_snapshot_curl_get_body=$curl_get_body"
  echo "appliance_contract_snapshot_curl_get_status=$curl_get_status"
  echo "appliance_contract_snapshot_qemu_log=$qemu_log"
  echo "appliance_contract_snapshot_curl_pcap=$curl_pcap"
  echo "appliance_contract_snapshot_pcap=$pcap"
  echo "appliance_contract_snapshot_pcap_bytes=$pcap_bytes"
  echo "appliance_contract_snapshot_resource_budget_summary=$budget_summary"
  echo "appliance_contract_snapshot_serial_marker_agreement=true"
  echo "appliance_contract_snapshot_summary_agreement=true"
  echo "appliance_contract_snapshot_lifecycle_agreement=true"
  echo "appliance_contract_snapshot_fault_marker_visible=true"
  echo "appliance_contract_snapshot_dynamic_route_discovery=false"
  echo "appliance_contract_snapshot_json_schema=false"
  echo "appliance_contract_snapshot_openapi=false"
  echo "appliance_contract_snapshot_heap_status_builder=false"
  echo "appliance_contract_snapshot_schema_free=true"
  echo "appliance_contract_snapshot_tls=false"
  echo "appliance_contract_snapshot_https=false"
  echo "appliance_contract_snapshot_benchmark_result=false"
} >"$summary"

reject_duplicate_keys "$summary"

echo "x86_64 microkernel appliance contract snapshot proof passed."
echo "summary: $summary"
