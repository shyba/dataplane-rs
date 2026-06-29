#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mnt_root="/home/user/mnt/dataplane"
log_dir="$mnt_root/logs"
tmp_dir="$mnt_root/tmp"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
run_log="$log_dir/x86_64-microkernel-protocol-input-bounds-ledger-$run_id.run.log"
summary="$log_dir/x86_64-microkernel-protocol-input-bounds-ledger-$run_id.summary"

mkdir -p "$log_dir" "$tmp_dir"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_file() {
  local path="$1"
  [[ -f "$path" ]] || fail "missing required file: $path"
}

require_marker() {
  local file="$1"
  local marker="$2"
  grep -Fq -- "$marker" "$file" || fail "missing marker in $file: $marker"
}

validate_serial_markers() {
  local file="$1"
  for marker in \
    'DPMK:PROTOCOL-INPUT-BOUNDS-LEDGER' \
    'DPBOUNDS:cli line=64 commands=15' \
    'DPBOUNDS:http request_line=96 headers=384 index_file=256 large_file=512' \
    'DPBOUNDS:tcp rx=472 segment=472 tx=1024 sessions=4 overflow_probe=473' \
    'DPBOUNDS:udp-control payload=16 version=0 opcode=0 reply=512' \
    'DPMK:PROTOCOL-INPUT-BOUNDS-LEDGER-OK'; do
    if ! grep -Fq -- "$marker" "$file"; then
      return 1
    fi
  done
  return 0
}

extract_artifact() {
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

check_duplicate_keys() {
  local file="$1"
  local duplicate
  duplicate="$(awk -F= '/^[A-Za-z0-9_]+=/ { print $1 }' "$file" | sort | uniq -d)"
  [[ -z "$duplicate" ]] || fail "duplicate summary key(s) in $file: $duplicate"
}

require_file "tools/x86_64_microkernel_fat32_run.sh"

export CARGO_TARGET_DIR="$mnt_root/target"
mkdir -p "$CARGO_TARGET_DIR"

make x86_64-microkernel-fat32-smoke-build

TMPDIR="$tmp_dir" CARGO_TARGET_DIR="$CARGO_TARGET_DIR" ./tools/x86_64_microkernel_fat32_run.sh >"$run_log" 2>&1

serial_log="$(extract_artifact "$run_log" "serial log")"
client_log="$(extract_artifact "$run_log" "client log")"
qemu_log="$(extract_artifact "$run_log" "qemu log")"
host_log="$(extract_artifact "$run_log" "network host log")"
pcap_log="$(extract_artifact "$run_log" "network pcap")"

if ! validate_serial_markers "$serial_log"; then
  fail "real serial log did not satisfy required protocol markers"
fi

corrupt_serial_log="$log_dir/x86_64-microkernel-protocol-input-bounds-ledger-$run_id.corrupt.serial.log"
cp "$serial_log" "$corrupt_serial_log"
perl -0pi -e 's/DPMK:PROTOCOL-INPUT-BOUNDS-LEDGER-OK/DPMK:PROTOCOL-INPUT-BOUNDS-LEDGER-BAD/g' "$corrupt_serial_log"

if validate_serial_markers "$corrupt_serial_log"; then
  fail "corrupt marker negative failed: validator accepted corrupted serial"
fi

if grep -Fq 'DPMK:PROTOCOL-INPUT-BOUNDS-LEDGER-BAD' "$serial_log"; then
  fail "unexpected corrupt marker in real serial log"
fi

serial_bytes="$(wc -c <"$serial_log" | tr -d ' ')"
client_bytes="$(wc -c <"$client_log" | tr -d ' ')"
qemu_bytes="$(wc -c <"$qemu_log" | tr -d ' ')"
host_bytes="$(wc -c <"$host_log" | tr -d ' ')"
pcap_bytes="$(wc -c <"$pcap_log" | tr -d ' ')"

{
  echo "protocol_input_bounds_ledger_summary_status=pass"
  echo "protocol_input_bounds_ledger_run_id=$run_id"
  echo "protocol_input_bounds_ledger_run_log=$run_log"
  echo "protocol_input_bounds_ledger_serial_log=$serial_log"
  echo "protocol_input_bounds_ledger_client_log=$client_log"
  echo "protocol_input_bounds_ledger_qemu_log=$qemu_log"
  echo "protocol_input_bounds_ledger_host_log=$host_log"
  echo "protocol_input_bounds_ledger_pcap=$pcap_log"
  echo "protocol_input_bounds_ledger_serial_bytes=$serial_bytes"
  echo "protocol_input_bounds_ledger_client_bytes=$client_bytes"
  echo "protocol_input_bounds_ledger_qemu_bytes=$qemu_bytes"
  echo "protocol_input_bounds_ledger_host_bytes=$host_bytes"
  echo "protocol_input_bounds_ledger_pcap_bytes=$pcap_bytes"
  echo "protocol_input_bounds_ledger_serial_marker_ok=true"
  echo "protocol_input_bounds_ledger_corrupt_marker_rejected=true"
  echo "protocol_input_bounds_ledger_fuzzing_framework=false"
  echo "protocol_input_bounds_ledger_dynamic_allocation=false"
  echo "protocol_input_bounds_ledger_tls=false"
  echo "protocol_input_bounds_ledger_https=false"
  echo "protocol_input_bounds_ledger_benchmark_result=false"
  echo "protocol_input_bounds_ledger_default_writable=false"
} >"$summary"

check_duplicate_keys "$summary"

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
    "protocol_input_bounds_ledger_summary_status": "pass",
    "protocol_input_bounds_ledger_serial_marker_ok": "true",
    "protocol_input_bounds_ledger_corrupt_marker_rejected": "true",
    "protocol_input_bounds_ledger_fuzzing_framework": "false",
    "protocol_input_bounds_ledger_dynamic_allocation": "false",
    "protocol_input_bounds_ledger_tls": "false",
    "protocol_input_bounds_ledger_https": "false",
    "protocol_input_bounds_ledger_benchmark_result": "false",
    "protocol_input_bounds_ledger_default_writable": "false",
}
for key, expected in required.items():
    if values.get(key) != expected:
        raise SystemExit(f"missing {key}={expected}")
PY

echo "x86_64 microkernel protocol input bounds ledger proof passed."
echo "summary: $summary"
