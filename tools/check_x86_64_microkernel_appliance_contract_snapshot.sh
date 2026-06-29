#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

snapshot="tools/x86_64_microkernel_appliance_contract_snapshot.tsv"
runner="tools/x86_64_microkernel_appliance_contract_snapshot.sh"
makefile="Makefile"
src_dir="crates/dataplane-x86_64-microkernel-smoke/src"
main="$src_dir/main.rs"
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
  rg -q --fixed-strings -- "$literal" "$file" || fail "$note"
}

reject_regex() {
  local file="$1"
  local regex="$2"
  local note="$3"
  local out="/home/user/mnt/dataplane/tmp/appliance-snapshot-guard.$$"
  local err="/home/user/mnt/dataplane/tmp/appliance-snapshot-guard-err.$$"
  local status
  mkdir -p /home/user/mnt/dataplane/tmp
  set +e
  rg -n -- "$regex" "$file" >"$out" 2>"$err"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    cat "$out"
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

for file in "$snapshot" "$runner" "$makefile" "$main" "$plan" "$diary"; do
  require_file "$file"
done

require_literal "$snapshot" $'route_id\tsurface\towner\tcapability\trequest_cap\tresponse_cap\tcounter_owner\tstatus_key\tproof_command\tcli_marker\thttp_marker\tserial_marker\tsummary_key\tdenied_deferred' \
  "snapshot must keep the expected TSV header"
require_literal "$snapshot" $'negative timeout delivery deferred; restart/replay/recovery denied' \
  "snapshot must track denied and deferred surfaces"

for route in \
  'timer.tick.self' \
  'cli.fs.request' \
  'http.fs.request' \
  'tcpip.net.tx' \
  'fs.block.read' \
  'service.lifecycle' \
  'fault.containment'; do
  require_literal "$snapshot" "$route" "snapshot must include route row: $route"
done

for literal in \
  'make x86_64-microkernel-timer-timeout-service' \
  'make x86_64-microkernel-fat32-smoke' \
  'make x86_64-microkernel-resource-budget-ledger' \
  'make x86_64-microkernel-service-lifecycle-ledger' \
  'make x86_64-microkernel-fault-policy-hardening'; do
  require_literal "$snapshot" "$literal" "snapshot must keep fresh proof wiring: $literal"
done

for literal in \
  'mnt_root="/home/user/mnt/dataplane"' \
  'snapshot="tools/x86_64_microkernel_appliance_contract_snapshot.tsv"' \
  'DP_MICROKERNEL_CURL_PROOF=1' \
  './tools/x86_64_microkernel_fat32_run.sh --curl-proof >"$curl_run_log" 2>&1' \
  './tools/x86_64_microkernel_resource_budget_ledger.sh >"$budget_run_log" 2>&1' \
  './tools/check_x86_64_microkernel_resource_budget_ledger_contract.sh >/dev/null' \
  './tools/check_x86_64_microkernel_service_lifecycle_ledger_contract.sh >/dev/null' \
  './tools/check_x86_64_microkernel_service_mailbox_envelope_contract.sh >/dev/null' \
  './tools/check_x86_64_microkernel_timer_timeout_service_contract.sh >/dev/null' \
  './tools/check_x86_64_microkernel_protocol_input_bounds_ledger_contract.sh >/dev/null' \
  'appliance_contract_snapshot_summary_status=pass' \
  'summary_value "$budget_summary" "resource_budget_ledger_pcap"' \
  'extract_label_path "$curl_run_log" "curl GET body"' \
  'extract_label_path "$curl_run_log" "curl GET status"' \
  'extract_label_path "$curl_run_log" "curl pcap"' \
  'appliance_contract_snapshot_rows=7' \
  'appliance_contract_snapshot_cli_agreement=true' \
  'appliance_contract_snapshot_http_agreement=true' \
  'appliance_contract_snapshot_serial_marker_agreement=true' \
  'appliance_contract_snapshot_summary_agreement=true' \
  'appliance_contract_snapshot_pcap_agreement=true' \
  'appliance_contract_snapshot_lifecycle_agreement=true' \
  'appliance_contract_snapshot_fault_marker_visible=true' \
  'appliance_contract_snapshot_dynamic_route_discovery=false' \
  'appliance_contract_snapshot_json_schema=false' \
  'appliance_contract_snapshot_schema_free=true' \
  'appliance_contract_snapshot_openapi=false' \
  'appliance_contract_snapshot_heap_status_builder=false' \
  'appliance_contract_snapshot_tls=false' \
  'appliance_contract_snapshot_https=false' \
  'appliance_contract_snapshot_benchmark_result=false'; do
  require_literal "$runner" "$literal" "runner must preserve appliance snapshot literal: $literal"
done

for literal in \
  'DPMK:RESOURCE-BUDGET-LEDGER' \
  'DPMK:PROTOCOL-INPUT-BOUNDS-LEDGER' \
  'DPMK:TIMER-TIMEOUT-SERVICE-LEDGER' \
  'DPMK:SERVICE-LIFECYCLE-LEDGER' \
  'DPCLI:LS / HELLO.TXT 40 INDEX.HTM 110' \
  'DPCLI:TASK timer id=1 endpoint=1 status=ready' \
  'DPCLI:TASK fs id=3 endpoint=3 status=ready' \
  'DPCLI:TASK block id=4 endpoint=4 status=ready' \
  'DPCLI:TASK tcpip id=6 endpoint=6 status=ready' \
  'DPMK:HTTP-GET-OK' \
  'DPMK:NET-UDP-ECHO' \
  'DPMK:FAULT-CONTAINED'; do
  require_literal "$runner" "$literal" "runner must cross-check marker: $literal"
done

for literal in \
  'DPMK:RESOURCE-BUDGET-LEDGER' \
  'DPMK:PROTOCOL-INPUT-BOUNDS-LEDGER' \
  'DPMK:TIMER-TIMEOUT-SERVICE-LEDGER' \
  'DPMK:SERVICE-LIFECYCLE-LEDGER' \
  'DPMK:HTTP-GET-OK'; do
  require_literal "$src_dir" "$literal" "guest must still contain marker: $literal"
done

require_literal "$makefile" 'x86_64-microkernel-appliance-contract-snapshot-contract:' \
  "Makefile must expose appliance contract snapshot guard"
require_literal "$makefile" './tools/check_x86_64_microkernel_appliance_contract_snapshot.sh' \
  "Makefile must run appliance snapshot guard"
require_literal "$makefile" 'x86_64-microkernel-appliance-contract-snapshot:' \
  "Makefile must expose appliance contract snapshot proof"
require_literal "$makefile" './tools/x86_64_microkernel_appliance_contract_snapshot.sh' \
  "Makefile must run appliance snapshot wrapper"
require_literal "$plan" 'x86_64-microkernel-appliance-contract-snapshot' \
  "plan must name appliance contract snapshot packet"
require_literal "$diary" 'x86_64-microkernel-appliance-contract-snapshot' \
  "diary must record appliance contract snapshot packet"

reject_regex "$snapshot" 'serde_json|OpenAPI|dynamic route discovery|heap-backed status builder|https://|TLS|certificate|benchmark' \
  "snapshot must stay schema-free, TLS-free, and benchmark-free"
reject_regex "$runner" 'appliance_contract_snapshot_.*(tls=true|https=true|benchmark_result=true|json_schema=true|openapi=true|dynamic_route_discovery=true|heap_status_builder=true)' \
  "snapshot summary must not overclaim schema, dynamic discovery, heap, TLS, HTTPS, or benchmark evidence"
reject_regex "$makefile" 'x86_64-microkernel-appliance-contract-snapshot.*(strict-five|STRICT_FIVE_CALIBRATION|retune|calibration)' \
  "appliance snapshot target must not substitute benchmark calibration evidence"

python3 - "$snapshot" "$src_dir" <<'PY'
import csv
import sys
from pathlib import Path

snapshot = Path(sys.argv[1])
src_dir = Path(sys.argv[2])
source = "\n".join(path.read_text(encoding="utf-8") for path in sorted(src_dir.rglob("*.rs")))
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
        "status_keys": {"DPTIMER:owner"},
        "markers": {"DPTIMER:owner", "DPMK:TIMER-TIMEOUT-SERVICE-LEDGER"},
    },
    "cli.fs.request": {
        "surface": "cli",
        "owner": "FsTask",
        "counter_owner": "FsTask",
        "request_cap": "64",
        "response_cap": "24",
        "proof_command": "make x86_64-microkernel-fat32-smoke",
        "status_keys": {"DPCLI:LS"},
        "markers": {"DPCLI:LS", "DPMK:FS-LS-ROOT-OK"},
    },
    "http.fs.request": {
        "surface": "http",
        "owner": "FsTask",
        "counter_owner": "HttpTask",
        "request_cap": "96",
        "response_cap": "256",
        "proof_command": "make x86_64-microkernel-fat32-smoke",
        "status_keys": {"DPMK:HTTP-GET-OK"},
        "markers": {"DPMK:HTTP-GET-OK", "/INDEX.HTM"},
    },
    "tcpip.net.tx": {
        "surface": "network",
        "owner": "TcpIpTask",
        "counter_owner": "TcpIpTask",
        "request_cap": "256",
        "response_cap": "16",
        "proof_command": "make x86_64-microkernel-fat32-smoke",
        "status_keys": {"DPMK:NET-UDP-ECHO"},
        "markers": {"DPMK:NET-UDP-ECHO", "task tcpip"},
    },
    "fs.block.read": {
        "surface": "storage",
        "owner": "BlockTask",
        "counter_owner": "FsTask",
        "request_cap": "256",
        "response_cap": "512",
        "proof_command": "make x86_64-microkernel-resource-budget-ledger",
        "status_keys": {"DPMK:BLK-SECTOR0-OK"},
        "markers": {"DPMK:BLK-SECTOR0-OK", "task block"},
    },
    "service.lifecycle": {
        "surface": "serial",
        "owner": "ServiceLifecycle",
        "counter_owner": "TaskTable",
        "request_cap": "9",
        "response_cap": "1",
        "proof_command": "make x86_64-microkernel-service-lifecycle-ledger",
        "status_keys": {"DPLIFE:ready"},
        "markers": {"DPMK:SERVICE-LIFECYCLE-LEDGER", "DPLIFE:ready"},
    },
    "fault.containment": {
        "surface": "cli",
        "owner": "FaultPolicy",
        "counter_owner": "BlockTask",
        "request_cap": "1",
        "response_cap": "1",
        "proof_command": "make x86_64-microkernel-fault-policy-hardening",
        "status_keys": {"DPMK:FAULT-CONTAINED"},
        "markers": {"DPMK:FAULT-CONTAINED", "task block"},
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
    if row["status_key"] not in expected[route]["status_keys"]:
        raise SystemExit(f"unexpected status key for {route}: {row['status_key']}")
    if row["status_key"] not in source:
        raise SystemExit(f"missing source marker for {route}: {row['status_key']}")
    if row["serial_marker"] not in source:
        raise SystemExit(f"serial marker not in guest source for {route}: {row['serial_marker']}")
    for marker in expected[route]["markers"]:
        if marker not in source:
            raise SystemExit(f"missing source marker for {route}: {marker}")

for forbidden in ("serde_json", "OpenAPI", "dynamic route discovery", "heap-backed status builder"):
    if forbidden in snapshot.read_text(encoding="ascii"):
        raise SystemExit(f"snapshot must not contain forbidden schema wording: {forbidden}")
PY

echo "x86_64 microkernel appliance contract snapshot guard OK"
