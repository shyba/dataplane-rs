#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

source "tools/x86_64_microkernel_validation_matrix_lib.sh"

log_root="$(dp_default_log_root DP_MICROKERNEL_STREAM_TIMEOUT_RETRY_ACCOUNTING_LOG_ROOT)"
run_id="$(dp_new_run_id)"
run_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-stream-timeout-retry-accounting" "$run_id" "run.log")"
summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-stream-timeout-retry-accounting" "$run_id" "summary")"
client_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-stream-timeout-retry-accounting" "$run_id" "client.log")"

mkdir -p "$log_root"
rm -f "$run_log" "$summary" "$client_log"

DP_MICROKERNEL_BOUNDED_TCP_NEGATIVE_PROOF=1 \
  ./tools/x86_64_microkernel_fat32_run.sh --bounded-tcp-negative-proof >"$run_log" 2>&1

bounded_tcp_log="$(dp_extract_artifact "$run_log" "bounded TCP log")" || {
  echo "FAIL: missing bounded TCP client log artifact"
  sed -n '1,220p' "$run_log" 2>/dev/null || true
  exit 1
}
bounded_tcp_pcap="$(dp_extract_artifact "$run_log" "bounded TCP pcap")" || {
  echo "FAIL: missing bounded TCP pcap artifact"
  sed -n '1,220p' "$run_log" 2>/dev/null || true
  exit 1
}
serial_log="$(dp_extract_artifact "$run_log" "serial log")" || {
  echo "FAIL: missing serial log artifact"
  sed -n '1,220p' "$run_log" 2>/dev/null || true
  exit 1
}
qemu_log="$(dp_extract_artifact "$run_log" "qemu log")" || {
  echo "FAIL: missing qemu log artifact"
  sed -n '1,220p' "$run_log" 2>/dev/null || true
  exit 1
}

cp "$bounded_tcp_log" "$client_log"

require_serial_marker() {
  local marker="$1"
  if ! grep -Fq "$marker" "$serial_log"; then
    echo "FAIL: serial log missing marker: $marker"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    exit 1
  fi
}

require_client_marker() {
  local marker="$1"
  if ! grep -Fq "$marker" "$client_log"; then
    echo "FAIL: client log missing marker: $marker"
    sed -n '1,220p' "$client_log" 2>/dev/null || true
    exit 1
  fi
}

require_serial_marker "DPMK:STREAM-RETRY-EVENTS:1"
require_serial_marker "DPMK:STREAM-TIMEOUT-ACCOUNTING owner=TcpIpTask stream=BoundedTcpStream"
require_serial_marker "timeout_events=1"
require_serial_marker "evidence=live-session-owner synthetic_timer_timeout_proof=reject"
require_serial_marker "DPMK:STREAM-TIMEOUT-RETRY-ACCOUNTING-OK"

require_client_marker "bounded_tcp_partial_request_no_response=true"
require_client_marker "bounded_tcp_partial_request_peer_port=41283"
require_client_marker "bounded_tcp_duplicate_payload_no_second_response=true"
require_client_marker "bounded_tcp_duplicate_payload_http_response_count=1"

if [[ ! -s "$bounded_tcp_pcap" ]]; then
  echo "FAIL: bounded TCP pcap artifact is empty"
  exit 1
fi
pcap_bytes="$(wc -c <"$bounded_tcp_pcap" | tr -d ' ')"
if (( pcap_bytes <= 24 )); then
  echo "FAIL: bounded TCP pcap contains only a global header"
  exit 1
fi

python3 - "$bounded_tcp_pcap" <<'PY'
import sys

pcap = open(sys.argv[1], "rb").read()
if len(pcap) <= 24:
    raise SystemExit("pcap has no packet records")
if b"GET /INDEX.HTM HTTP/1.0\r\n" not in pcap:
    raise SystemExit("pcap missing host-visible bounded stream request bytes")
if b"HTTP/1.0 200 OK\r\n" not in pcap:
    raise SystemExit("pcap missing host-visible bounded stream response bytes")
if b"\xa1C\x00P" not in pcap:
    raise SystemExit("pcap missing host-visible partial-session TCP ports 41283->80")
PY

accounting_line="$(grep -F "DPMK:STREAM-TIMEOUT-ACCOUNTING owner=TcpIpTask stream=BoundedTcpStream" "$serial_log" | tail -n 1)"
if [[ -z "$accounting_line" ]]; then
  echo "FAIL: missing focused stream timeout accounting line"
  sed -n '1,220p' "$serial_log" 2>/dev/null || true
  exit 1
fi
peer_port="$(printf '%s\n' "$accounting_line" | sed -n 's/.* peer_port=\([0-9][0-9]*\).*/\1/p')"
session_generation="$(printf '%s\n' "$accounting_line" | sed -n 's/.* session_generation=\([0-9][0-9]*\).*/\1/p')"
age_ticks="$(printf '%s\n' "$accounting_line" | sed -n 's/.* age_ticks=\([0-9][0-9]*\).*/\1/p')"
deadline_ticks="$(printf '%s\n' "$accounting_line" | sed -n 's/.* deadline_ticks=\([0-9][0-9]*\).*/\1/p')"
timeout_events="$(printf '%s\n' "$accounting_line" | sed -n 's/.* timeout_events=\([0-9][0-9]*\).*/\1/p')"
retry_events="$(printf '%s\n' "$accounting_line" | sed -n 's/.* retry_events=\([0-9][0-9]*\).*/\1/p')"
stream_timeout_count="$(printf '%s\n' "$accounting_line" | sed -n 's/.* stream_timeout_count=\([0-9][0-9]*\).*/\1/p')"
stream_retry_count="$(printf '%s\n' "$accounting_line" | sed -n 's/.* stream_retry_count=\([0-9][0-9]*\).*/\1/p')"
timer_ticks="$(printf '%s\n' "$accounting_line" | sed -n 's/.* now_ticks=\([0-9][0-9]*\).*/\1/p')"

if [[ -z "$peer_port" || -z "$session_generation" || -z "$age_ticks" || -z "$deadline_ticks" ||
      -z "$timeout_events" || -z "$retry_events" || -z "$stream_timeout_count" ||
      -z "$stream_retry_count" || -z "$timer_ticks" ]]; then
  echo "FAIL: could not parse live stream timeout accounting line"
  printf '%s\n' "$accounting_line"
  exit 1
fi

if [[ "$peer_port" != "41283" ]]; then
  echo "FAIL: accounting line peer_port does not match host-visible partial session"
  printf '%s\n' "$accounting_line"
  sed -n '1,220p' "$client_log" 2>/dev/null || true
  exit 1
fi

if (( age_ticks <= 8 )); then
  echo "FAIL: stream timeout age did not exceed bounded timeout window"
  printf '%s\n' "$accounting_line"
  exit 1
fi

if (( timeout_events != 1 || retry_events != 1 || stream_timeout_count != 1 || stream_retry_count != 1 )); then
  echo "FAIL: timeout and retry accounting did not come from the same live accounting line"
  printf '%s\n' "$accounting_line"
  exit 1
fi

cat >"$summary" <<EOF_SUMMARY
stream_timeout_retry_accounting_status=pass
stream_timeout_retry_accounting_run_id=$run_id
stream_timeout_retry_accounting_proof_kind=qemu-live-session
stream_timeout_retry_accounting_owner=TcpIpTask
stream_timeout_retry_accounting_stream_owner=BoundedTcpStream
stream_timeout_retry_accounting_peer_port=$peer_port
stream_timeout_retry_accounting_session_generation=$session_generation
stream_timeout_retry_accounting_timeout_events=$timeout_events
stream_timeout_retry_accounting_retry_events=$retry_events
stream_timeout_retry_accounting_stream_timeout_count=$stream_timeout_count
stream_timeout_retry_accounting_stream_retry_count=$stream_retry_count
stream_timeout_retry_accounting_age_ticks=$age_ticks
stream_timeout_retry_accounting_deadline_ticks=$deadline_ticks
stream_timeout_retry_accounting_timer_ticks=$timer_ticks
stream_timeout_retry_accounting_host_visible_partial_peer_port=41283
stream_timeout_retry_accounting_synthetic_timer_timeout_proof=rejected
stream_timeout_retry_accounting_serial_log=$serial_log
stream_timeout_retry_accounting_client_log=$client_log
stream_timeout_retry_accounting_pcap=$bounded_tcp_pcap
stream_timeout_retry_accounting_qemu_log=$qemu_log
stream_timeout_retry_accounting_run_log=$run_log
stream_timeout_retry_accounting_no_tls=true
stream_timeout_retry_accounting_no_tcp_compliance_claim=true
EOF_SUMMARY

if ! dp_reject_duplicate_keys "$summary"; then
  echo "FAIL: stream timeout retry accounting summary has duplicate keys"
  exit 1
fi

echo "x86_64 microkernel stream timeout retry accounting proof passed."
dp_emit_artifact_line "stream timeout retry accounting summary" "$summary"
dp_emit_artifact_line "stream timeout retry accounting client log" "$client_log"
dp_emit_artifact_line "stream timeout retry accounting serial log" "$serial_log"
dp_emit_artifact_line "stream timeout retry accounting pcap" "$bounded_tcp_pcap"
dp_emit_artifact_line "stream timeout retry accounting qemu log" "$qemu_log"
dp_emit_artifact_line "stream timeout retry accounting run log" "$run_log"
