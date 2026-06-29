#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
kernel="crates/dataplane-x86_64-microkernel-smoke/src/kernel.rs"
scenarios="crates/dataplane-x86_64-microkernel-smoke/src/scenarios.rs"
tcp_stream="crates/dataplane-x86_64-microkernel-smoke/src/tcp_stream.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
matrix_runner="tools/x86_64_microkernel_validation_matrix_run.sh"
makefile="Makefile"
bounded_guard="tools/check_x86_64_microkernel_bounded_tcp_stream_contract.sh"

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
  local matches status
  set +e
  matches="$(rg -n -- "$regex" "$file")"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    printf '%s\n' "$matches"
    fail "$note"
  fi
  [[ "$status" -eq 1 ]] || fail "could not scan $file for forbidden regex: $regex"
}

for file in "$main" "$kernel" "$scenarios" "$tcp_stream" "$runner" "$matrix_runner" "$makefile" "$bounded_guard"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel Stream Session State Contract Guard ==="

for literal in \
  'DPMK:TCP-CTRL-PARTIAL:' \
  'DPMK:TCP-CTRL-DUPLICATE:' \
  'DPMK:TCP-CTRL-SESSION-FULL:' \
  'DPMK:TCP-CTRL-RX-OVERFLOW:' \
  'DPMK:TCP-CTRL-OK'; do
  require_literal "$scenarios" "$literal" "guest source must contain stream/session literal: $literal"
done

for literal in \
  'active_sessions: u32' \
  'too_many_sessions: u32'; do
  require_literal "$tcp_stream" "$literal" "guest source must contain stream/session literal: $literal"
done

for literal in \
  'x86_64-microkernel-stream-session-state-contract:' \
  'x86_64-microkernel-stream-session-state: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-bounded-tcp-stream-contract x86_64-microkernel-stream-session-state-contract' \
  'DP_MICROKERNEL_STREAM_SESSION_STATE_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --stream-session-state-proof'; do
  require_literal "$makefile" "$literal" "Makefile must contain stream/session literal: $literal"
done

for literal in \
  'stream-session-state' \
  'stream session state summary|client log|serial log|qemu log|network pcap' \
  'x86_64 microkernel stream session state proof passed.' \
  "stream-session-state) echo 'make x86_64-microkernel-stream-session-state' ;;" \
  "stream-session-state) echo 'x86_64 microkernel stream session state proof passed.' ;;" \
  "stream-session-state) echo 'stream session state summary|client log|serial log|qemu log|network pcap' ;;"; do
  require_literal "$matrix_runner" "$literal" "validation matrix must contain stream/session literal: $literal"
done

for literal in \
  'bounded_tcp_reset_before_completion_no_response=true' \
  'bounded_tcp_fin_during_response_fin_ack_seen=true'; do
  require_literal "$runner" "$literal" "runner must contain bounded stream/session literal: $literal"
done

require_literal "$makefile" 'x86_64-microkernel-stream-session-state-contract:' \
  "Makefile must expose the stream/session contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_stream_session_state_contract.sh' \
  "Makefile stream/session contract must run the focused guard"
require_literal "$makefile" 'x86_64-microkernel-stream-session-state: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-bounded-tcp-stream-contract x86_64-microkernel-stream-session-state-contract' \
  "Makefile stream/session proof target must preserve neighboring contract coverage"
require_literal "$makefile" 'DP_MICROKERNEL_STREAM_SESSION_STATE_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --stream-session-state-proof' \
  "Makefile stream/session target must run the focused proof mode"

reject_regex "$runner" 'curl[[:space:]]+-k|https://|openssl|rustls|embedded-tls|webpki|aws-lc-rs|DPMK:TLS|HTTPS-' \
  "stream/session packet must not reopen TLS"
reject_regex "$matrix_runner" 'STRICT_FIVE_CALIBRATION|benchmark[[:space:]]+tuning|retun(e|ing)' \
  "stream/session packet must not tune benchmarks"
reject_regex "$kernel" 'GenericSocket|SocketApi|listen_socket|accept_socket|TcpListener' \
  "stream/session packet must not add a generic socket API"

echo "x86_64 microkernel stream session state contract OK"
