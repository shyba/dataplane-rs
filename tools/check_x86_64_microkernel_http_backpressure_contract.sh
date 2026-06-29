#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

spec="changes/__archived_changes_2026-05-31/x86_64-microkernel-http-backpressure/specs/http-backpressure/spec.md"
tasks="changes/__archived_changes_2026-05-31/x86_64-microkernel-http-backpressure/tasks.md"
diary="aidocs/050_microkernel_robust_design_diary_2026-05-30.md"
guest="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
matrix_runner="tools/x86_64_microkernel_validation_matrix_run.sh"
makefile="Makefile"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_file() {
  local path="$1"
  [[ -f "$path" ]] || fail "missing required file: $path"
}

require_literal() {
  local path="$1"
  local literal="$2"
  local message="$3"
  if ! rg -Fq -- "$literal" "$path"; then
    fail "$message"
  fi
}

reject_regex() {
  local path="$1"
  local regex="$2"
  local message="$3"
  local tmp status
  tmp="$(mktemp)"
  set +e
  rg -n -- "$regex" "$path" >"$tmp" 2>&1
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    cat "$tmp" >&2
    rm -f "$tmp"
    fail "$message"
  fi
  if [[ "$status" -ne 1 ]]; then
    cat "$tmp" >&2
    rm -f "$tmp"
    fail "rg failed while scanning $path for forbidden regex: $regex"
  fi
  rm -f "$tmp"
}

echo "=== x86_64 Microkernel HTTP Backpressure Contract Guard ==="

for path in "$spec" "$tasks" "$diary" "$guest" "$runner" "$matrix_runner" "$makefile"; do
  require_file "$path"
done

require_literal "$spec" "### Requirement: Keep the HTTP path bounded" \
  "spec must define the bounded-path requirement"
require_literal "$spec" "### Requirement: Separate HTTP policy from stream segmentation" \
  "spec must define the policy/transport split requirement"
require_literal "$spec" "### Requirement: Preserve explicit source-real evidence" \
  "spec must define the source-real evidence requirement"
require_literal "$spec" "### Requirement: Reject unsupported transport scope" \
  "spec must reject unsupported transport scope"
require_literal "$spec" "### Requirement: Preserve narrow storage claims" \
  "spec must keep storage claims narrow"

require_literal "$guest" 'http_backpressure_segments >= 2' \
  "guest must wait for multiple backpressure segments before closing"
require_literal "$guest" 'DPMK:HTTP-BACKPRESSURE-SPLIT:' \
  "guest must emit a split-response marker"
require_literal "$guest" 'DPMK:HTTP-BACKPRESSURE-FAT32-OK' \
  "guest must emit FAT32 evidence"
require_literal "$guest" 'DPMK:HTTP-BACKPRESSURE-CLI-OK' \
  "guest must emit CLI progress evidence"
require_literal "$guest" 'DPMK:HTTP-BACKPRESSURE-TIMER-MAXGAP:' \
  "guest must emit timer progress evidence"
require_literal "$guest" 'DPMK:HTTP-BACKPRESSURE-FAULT-CONTAINED' \
  "guest must emit fault-containment evidence"
require_literal "$guest" 'DPMK:HTTP-BACKPRESSURE-OK' \
  "guest must emit the positive completion marker"
require_literal "$guest" 'self.dispatch_cli_command(b"tasks")?' \
  "guest must keep CLI work explicit"
require_literal "$guest" 'self.run_timer_tick()?' \
  "guest must keep timer work explicit"
require_literal "$guest" 'prove_fault_containment()' \
  "guest must keep fault containment explicit"

require_literal "$runner" 'if [[ "$mode" == "--http-backpressure-proof" ]]; then' \
  "runner must expose the HTTP backpressure proof mode"
require_literal "$runner" 'http_backpressure_min_body_bytes="${DP_MICROKERNEL_HTTP_BACKPRESSURE_MIN_BODY:-480}"' \
  "runner must keep a bounded minimum body size"
require_literal "$runner" 'http_backpressure_split_response_ok=true' \
  "runner must record split-response success"
require_literal "$runner" 'grep -q "DPMK:HTTP-BACKPRESSURE-SPLIT:" "$serial_log"' \
  "runner must require the split-response serial marker"
require_literal "$runner" 'grep -q "DPMK:HTTP-BACKPRESSURE-FAT32-OK" "$serial_log"' \
  "runner must require FAT32 evidence"
require_literal "$runner" 'grep -q "DPMK:HTTP-BACKPRESSURE-CLI-OK" "$serial_log"' \
  "runner must require CLI evidence"
require_literal "$runner" 'grep -q "DPMK:HTTP-BACKPRESSURE-TIMER-MAXGAP:" "$serial_log"' \
  "runner must require timer evidence"
require_literal "$runner" 'grep -q "DPMK:HTTP-BACKPRESSURE-FAULT-CONTAINED" "$serial_log"' \
  "runner must require fault-containment evidence"
require_literal "$runner" 'HTTP backpressure pcap did not contain a split response' \
  "runner must keep pcap split-response validation"

require_literal "$matrix_runner" "http-backpressure) echo 'make x86_64-microkernel-http-backpressure' ;;" \
  "validation matrix should dispatch the packet-facing target"
require_literal "$matrix_runner" "http-backpressure) echo 'x86_64 microkernel HTTP backpressure proof passed.' ;;" \
  "validation matrix must keep the positive packet marker"
require_literal "$matrix_runner" "http-backpressure) echo 'HTTP backpressure log|HTTP backpressure pcap|serial log|qemu log' ;;" \
  "validation matrix must keep the packet artifacts explicit"

require_literal "$makefile" 'x86_64-microkernel-http-backpressure-contract: guard-scripts-executable' \
  "Makefile must expose the HTTP backpressure contract target"
require_literal "$makefile" 'x86_64-microkernel-fat32-http-backpressure: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-http-backpressure-contract' \
  "Makefile must wire the packet-facing proof target through the contract"
require_literal "$makefile" 'x86_64-microkernel-http-backpressure: x86_64-microkernel-fat32-http-backpressure' \
  "Makefile must keep packet-facing alias wiring"
require_literal "$makefile" './tools/x86_64_microkernel_fat32_run.sh --http-backpressure-proof' \
  "Makefile target must invoke the proof runner"

reject_regex "$runner" 'https://|OpenSSL|certificate|private[[:space:]]+key|curl[[:space:]]+-k|DP_MICROKERNEL_TLS_PROOF|--tls-proof|rustls|webpki|embedded-tls|crypto-provider' \
  "HTTP backpressure proof must not add TLS or certificate tooling"
reject_regex "$runner" 'keep-alive|chunked|generic[[:space:]]+socket|socket[[:space:]]+api|broad[[:space:]]+TCP[[:space:]]+compliance|retransmission[[:space:]]+queue|out-of-order' \
  "HTTP backpressure proof must not broaden into unsupported transport scope"

echo "x86_64 microkernel HTTP backpressure contract guard passed."
