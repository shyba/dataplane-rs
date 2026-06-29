#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
tcp_stream="crates/dataplane-x86_64-microkernel-smoke/src/tcp_stream.rs"
runner="tools/x86_64_microkernel_stream_timeout_retry_accounting.sh"
makefile="Makefile"
spec="changes/__archived_changes_2026-06-01/x86_64-microkernel-stream-timeout-retry-accounting/specs/stream-timeout-retry-accounting/spec.md"
tasks="changes/__archived_changes_2026-06-01/x86_64-microkernel-stream-timeout-retry-accounting/tasks.md"

require_file() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    echo "missing required file: $path" >&2
    exit 1
  fi
}

require_literal() {
  local path="$1"
  local literal="$2"
  local message="$3"
  local status
  set +e
  rg -Fq -- "$literal" "$path"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    return 0
  fi
  if [[ "$status" -eq 1 ]]; then
    echo "$message" >&2
    echo "missing literal in $path: $literal" >&2
    exit 1
  fi
  echo "rg failed while scanning $path for required literal: $literal" >&2
  exit 1
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
    echo "$message" >&2
    cat "$tmp" >&2
    rm -f "$tmp"
    exit 1
  fi
  if [[ "$status" -ne 1 ]]; then
    echo "rg failed while scanning $path for forbidden regex: $regex" >&2
    cat "$tmp" >&2
    rm -f "$tmp"
    exit 1
  fi
  rm -f "$tmp"
}

echo "=== x86_64 Microkernel Stream Timeout Retry Accounting Contract Guard ==="

for path in "$main" "$tcp_stream" "$runner" "$makefile" "$spec" "$tasks"; do
  require_file "$path"
done

require_literal "$tcp_stream" "last_activity_ticks: u32" \
  "BoundedTcpStream must carry live last-activity state"
require_literal "$tcp_stream" "timeout_deadline_ticks: u32" \
  "BoundedTcpStream must carry a live timeout deadline"
require_literal "$tcp_stream" "last_age_ticks: u32" \
  "BoundedTcpStream must carry live age accounting"
require_literal "$tcp_stream" "retry_count: u32" \
  "BoundedTcpStream must carry retry accounting"
require_literal "$tcp_stream" "timeout_count: u32" \
  "BoundedTcpStream must carry timeout accounting"
require_literal "$main" "timer_ticks: u32" \
  "TcpIpTask must carry live timer progression"
require_literal "$main" "fn run_stream_timer_ticks(&mut self, ticks: u32) -> Option<StreamTimeoutSnapshot>" \
  "TcpIpTask must advance stream timeout state from timer progression"
require_literal "$main" "fn tick_for_session_event(&mut self) -> u32" \
  "TcpIpTask must update timer state from actual session processing"
require_literal "$main" "self.tcp_counters.retry_events = self.tcp_counters.retry_events.saturating_add(1)" \
  "TcpIpTask must account retries from duplicate live session handling"
require_literal "$main" "self.tcp_counters.timeout_events =" \
  "TcpIpTask must account timeout events from live stream expiry"
require_literal "$main" "let timeout_port = 41_283" \
  "focused guest proof must use the host-visible partial-session port as its session key"
require_literal "$main" "timeout_counters.retry_events != 1" \
  "focused guest proof must require retry accounting on the timeout owner"
require_literal "$main" "snapshot.retry_count != 1" \
  "focused guest proof must require stream retry accounting on the timeout owner"
require_literal "$main" "DPMK:STREAM-TIMEOUT-ACCOUNTING owner=TcpIpTask stream=BoundedTcpStream" \
  "guest serial evidence must expose the live owner path"
require_literal "$main" "stream_retry_count=" \
  "guest serial evidence must expose stream retry count on the accounting line"
require_literal "$main" "retry_events=" \
  "guest serial evidence must expose retry events on the accounting line"
require_literal "$main" "evidence=live-session-owner synthetic_timer_timeout_proof=reject" \
  "guest serial evidence must reject TimerTimeoutProof as stream evidence"
require_literal "$main" "DPMK:STREAM-TIMEOUT-RETRY-ACCOUNTING-OK" \
  "guest serial evidence must include the focused pass marker"

require_literal "$runner" "DP_MICROKERNEL_BOUNDED_TCP_NEGATIVE_PROOF=1" \
  "focused runner must drive a host-visible QEMU TCP session proof"
require_literal "$runner" "stream timeout retry accounting summary" \
  "focused runner must emit a summary artifact"
require_literal "$runner" "stream timeout retry accounting client log" \
  "focused runner must emit a client artifact"
require_literal "$runner" "stream timeout retry accounting serial log" \
  "focused runner must emit a serial artifact"
require_literal "$runner" "stream timeout retry accounting pcap" \
  "focused runner must emit a pcap artifact"
require_literal "$runner" "stream_timeout_retry_accounting_proof_kind=qemu-live-session" \
  "focused summary must identify QEMU live-session evidence"
require_literal "$runner" 'accounting_line="$(grep -F "DPMK:STREAM-TIMEOUT-ACCOUNTING owner=TcpIpTask stream=BoundedTcpStream"' \
  "focused runner must parse timeout and retry from the accounting line"
require_literal "$runner" 'peer_port="$(printf '\''%s\n'\'' "$accounting_line"' \
  "focused runner must parse the peer port from the accounting line"
require_literal "$runner" 'retry_events="$(printf '\''%s\n'\'' "$accounting_line"' \
  "focused runner must parse retry events from the accounting line, not a separate marker"
require_literal "$runner" 'stream_retry_count="$(printf '\''%s\n'\'' "$accounting_line"' \
  "focused runner must parse stream retry count from the accounting line"
require_literal "$runner" "bounded_tcp_partial_request_peer_port=41283" \
  "focused runner must tie client evidence to the accounting peer port"
require_literal "$runner" 'stream_timeout_retry_accounting_peer_port=$peer_port' \
  "focused summary must report the accounting peer port"
require_literal "$runner" 'stream_timeout_retry_accounting_stream_retry_count=$stream_retry_count' \
  "focused summary must report stream retry count from the accounting line"
require_literal "$runner" "pcap missing host-visible partial-session TCP ports 41283->80" \
  "focused runner must require pcap evidence for the accounting peer port"
require_literal "$runner" "timeout and retry accounting did not come from the same live accounting line" \
  "focused runner must reject split timeout/retry marker construction"
require_literal "$runner" "stream_timeout_retry_accounting_synthetic_timer_timeout_proof=rejected" \
  "focused summary must reject TimerTimeoutProof evidence"
require_literal "$runner" "DPMK:STREAM-TIMEOUT-RETRY-ACCOUNTING-OK" \
  "focused runner must fail closed without the guest live-accounting marker"
require_literal "$runner" "age_ticks <= 8" \
  "focused runner must reject non-expired timeout age evidence"
require_literal "$runner" "python3" \
  "focused runner must be compatible with the repo python3 rule"

require_literal "$makefile" "x86_64-microkernel-stream-timeout-retry-accounting-contract:" \
  "Makefile must expose the focused contract target"
require_literal "$makefile" "./tools/check_x86_64_microkernel_stream_timeout_retry_accounting_contract.sh" \
  "Makefile contract target must run the focused guard"
require_literal "$makefile" "x86_64-microkernel-stream-timeout-retry-accounting: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-bounded-tcp-stream-contract x86_64-microkernel-stream-session-state-contract x86_64-microkernel-bounded-stream-session-refresh-contract x86_64-microkernel-timer-timeout-service-contract x86_64-microkernel-stream-timeout-retry-accounting-contract" \
  "Makefile proof target must include the prerequisite gates and focused guard"
require_literal "$makefile" "./tools/x86_64_microkernel_stream_timeout_retry_accounting.sh" \
  "Makefile proof target must run the focused wrapper"

reject_regex "$runner" 'evaluate_timer_timeout_proof|struct[[:space:]]+[A-Za-z0-9_]*Proof' \
  "focused runner must not use synthetic TimerTimeoutProof or hard-coded proof structs"
reject_regex "$runner" 'retry_line=' \
  "focused runner must not parse retry accounting from a separate serial marker"
reject_regex "$runner" 'https://|OpenSSL|certificate|private[[:space:]]+key|curl[[:space:]]+-k|rustls|embedded-tls|webpki|crypto-provider' \
  "stream timeout retry accounting must not add transport-security tooling"
reject_regex "$runner" 'DNS|NTP|service[[:space:]]+discovery|generic[[:space:]]+socket|TcpListener|([^A-Za-z0-9_]|^)TcpStream([^A-Za-z0-9_]|$)|retransmission[[:space:]]+queue|out-of-order|TCP[[:space:]]+compliance' \
  "stream timeout retry accounting must not broaden the networking scope"
reject_regex "$runner" 'STRICT_FIVE_CALIBRATION|benchmark[[:space:]]+tuning|retun(e|ing)|calibration' \
  "stream timeout retry accounting must not tune benchmarks"
reject_regex "$main" 'stream_timeout_retry_accounting_.*TimerTimeoutProof|TimerTimeoutProof.*STREAM-TIMEOUT' \
  "guest stream timeout accounting must not route through TimerTimeoutProof"

echo "x86_64 microkernel stream timeout retry accounting contract guard passed."
