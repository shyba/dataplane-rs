#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
kernel="crates/dataplane-x86_64-microkernel-smoke/src/kernel.rs"
kernel_ledgers="crates/dataplane-x86_64-microkernel-smoke/src/kernel_ledgers.rs"
makefile="Makefile"
runner="tools/x86_64_microkernel_fat32_run.sh"
wrapper="tools/x86_64_microkernel_protocol_input_bounds_ledger.sh"
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
  grep -Fq -- "$literal" "$file" || fail "$note"
}

reject_regex() {
  local file="$1"
  local regex="$2"
  local note="$3"
  local out="/tmp/dataplane-protocol-bounds.$$"
  local err="/tmp/dataplane-protocol-bounds-err.$$"
  local status
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

for file in "$main" "$kernel" "$kernel_ledgers" "$makefile" "$runner" "$wrapper" "$plan" "$diary"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel Protocol Input Bounds Ledger Contract ==="

for literal in \
  'emit_protocol_input_bounds_ledger();'; do
  require_literal "$kernel" "$literal" "guest orchestration must call input-bound ledger: $literal"
done

for literal in \
  'fn emit_protocol_input_bounds_ledger()' \
  'DPMK:PROTOCOL-INPUT-BOUNDS-LEDGER' \
  'DPBOUNDS:cli line=' \
  'DPBOUNDS:http request_line=' \
  'DPBOUNDS:tcp rx=' \
  'DPBOUNDS:udp-control payload=' \
  'DPMK:PROTOCOL-INPUT-BOUNDS-LEDGER-OK' \
  'serial::write_decimal(CLI_LINE_BYTES as u32)' \
  'serial::write_decimal(CLI_COMMAND_COUNT as u32)' \
  'serial::write_decimal(HTTP_REQUEST_LINE_BYTES as u32)' \
  'serial::write_decimal(HTTP_HEADER_BYTES as u32)' \
  'serial::write_decimal(HTTP_INDEX_FILE_BYTES as u32)' \
  'serial::write_decimal(HTTP_LARGE_FILE_BYTES as u32)' \
  'serial::write_decimal(TCP_STREAM_RX_BYTES as u32)' \
  'serial::write_decimal(TCP_STREAM_SEGMENT_BYTES as u32)' \
  'serial::write_decimal(TCP_STREAM_TX_BYTES as u32)' \
  'serial::write_decimal(TCP_STREAM_SESSIONS as u32)' \
  'serial::write_decimal(TCP_CONTROL_OVERFLOW_BYTES as u32)' \
  'serial::write_decimal(CONTROL_PROTOCOL_V1_MAX_PAYLOAD_BYTES as u32)' \
  'serial::write_decimal(CONTROL_PROTOCOL_V1_VERSION as u32)' \
  'serial::write_decimal(CONTROL_PROTOCOL_V1_OPCODE_STATUS_ECHO as u32)' \
  'serial::write_decimal(NET_REPLY_PAYLOAD_BYTES as u32)'; do
  require_literal "$kernel_ledgers" "$literal" "guest source must expose input-bound literal: $literal"
done

require_literal "$makefile" 'x86_64-microkernel-protocol-input-bounds-ledger-contract:' \
  "Makefile must expose the protocol input bounds ledger contract"
require_literal "$makefile" './tools/check_x86_64_microkernel_protocol_input_bounds_ledger_contract.sh' \
  "Makefile must run the protocol input bounds ledger contract guard"
require_literal "$makefile" 'x86_64-microkernel-protocol-input-bounds-ledger:' \
  "Makefile must expose the protocol input bounds ledger proof target"
require_literal "$makefile" 'x86_64-microkernel-protocol-input-bounds-ledger: guard-scripts-executable x86_64-microkernel-protocol-input-bounds-ledger-contract' \
  "Makefile must defer the protocol input bounds ledger build to the wrapper"
require_literal "$makefile" './tools/x86_64_microkernel_protocol_input_bounds_ledger.sh' \
  "Makefile must run the protocol input bounds ledger wrapper"
for literal in \
  'mnt_root="/home/user/mnt/dataplane"' \
  'log_dir="$mnt_root/logs"' \
  'tmp_dir="$mnt_root/tmp"' \
  'export CARGO_TARGET_DIR="$mnt_root/target"' \
  'mkdir -p "$CARGO_TARGET_DIR"' \
  'make x86_64-microkernel-fat32-smoke-build' \
  'protocol_input_bounds_ledger_summary_status=pass' \
  'protocol_input_bounds_ledger_corrupt_marker_rejected=true' \
  'protocol_input_bounds_ledger_fuzzing_framework=false' \
  'protocol_input_bounds_ledger_dynamic_allocation=false' \
  'protocol_input_bounds_ledger_tls=false' \
  'protocol_input_bounds_ledger_https=false' \
  'protocol_input_bounds_ledger_benchmark_result=false' \
  'protocol_input_bounds_ledger_default_writable=false' \
  'summary: $summary'; do
  require_literal "$wrapper" "$literal" "wrapper must preserve evidence literal: $literal"
done
reject_regex "$wrapper" 'mv[[:space:]]+"\$repo_target"|ln -sfn "\$mnt_target" "\$repo_target"|target_backup|target_linked|cleanup\(\)' \
  "wrapper must not move or replace the repo target directory"
for literal in \
  'DPMK:PROTOCOL-INPUT-BOUNDS-LEDGER' \
  'DPBOUNDS:cli line=64 commands=24' \
  'DPBOUNDS:http request_line=96 headers=384 index_file=256 large_file=512' \
  'DPBOUNDS:tcp rx=472 segment=472 tx=1024 sessions=4 overflow_probe=473' \
  'DPBOUNDS:udp-control payload=16 version=0 opcode=0 reply=512' \
  'DPMK:PROTOCOL-INPUT-BOUNDS-LEDGER-OK'; do
  require_literal "$runner" "$literal" "QEMU runner must validate runtime input-bounds marker: $literal"
done
require_literal "$wrapper" 'validate_serial_markers "$serial_log"' \
  "QEMU runner must validate the real serial log with the marker helper"
require_literal "$wrapper" 'if validate_serial_markers "$corrupt_serial_log"; then' \
  "QEMU runner must fail closed on corrupted markers"
require_literal "$plan" 'x86_64-microkernel-protocol-input-bounds-ledger' \
  "plan must keep the protocol input bounds ledger packet"
require_literal "$diary" 'x86_64-microkernel-protocol-input-bounds-ledger' \
  "diary must record the protocol input bounds ledger packet"

reject_regex "$main" 'DPBOUNDS.*(Vec|String|Box|BTree|HashMap|format!|alloc::|std::)' \
  "protocol bounds ledger must not add heap-backed formatting or containers"
reject_regex "$main" 'DPMK:PROTOCOL-INPUT-BOUNDS.*(TLS|HTTPS|JSON|OpenAPI|dynamic|negotiat|unbounded|production)' \
  "protocol bounds ledger must not claim TLS, schema, dynamic, or production protocol behavior"
reject_regex "$makefile" 'x86_64-microkernel-protocol-input-bounds-ledger.*(strict-five|STRICT_FIVE_CALIBRATION|retune|calibration)' \
  "protocol bounds ledger target must not substitute benchmark calibration evidence"

echo "x86_64 microkernel protocol input bounds ledger contract OK"
