#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

guest="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
kernel="crates/dataplane-x86_64-microkernel-smoke/src/kernel.rs"
network_task="crates/dataplane-x86_64-microkernel-smoke/src/network_task.rs"
scenarios="crates/dataplane-x86_64-microkernel-smoke/src/scenarios.rs"
tcp_stream="crates/dataplane-x86_64-microkernel-smoke/src/tcp_stream.rs"
roadmap="aidocs/047_microkernel_http_fat32_cli_roadmap_2026-05-30.md"
runner="tools/x86_64_microkernel_fat32_run.sh"
root_manifest="Cargo.toml"
smoke_manifest="crates/dataplane-x86_64-microkernel-smoke/Cargo.toml"
core_manifest="crates/dataplane-microkernel-core/Cargo.toml"
lockfile="Cargo.lock"
tmp="${TMPDIR:-/tmp}/dataplane_bounded_tcp_stream_guard.$$"

cleanup() {
  rm -f "$tmp"
}
trap cleanup EXIT

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
  if ! rg -Fq -- "$literal" "$path"; then
    echo "$message" >&2
    echo "missing literal in $path: $literal" >&2
    exit 1
  fi
}

reject_regex() {
  local path="$1"
  local regex="$2"
  local message="$3"
  set +e
  rg -n -- "$regex" "$path" >"$tmp" 2>&1
  local status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    echo "$message" >&2
    cat "$tmp" >&2
    exit 1
  fi
  if [[ "$status" -ne 1 ]]; then
    echo "rg failed while scanning $path for $regex" >&2
    cat "$tmp" >&2
    exit 1
  fi
}

echo "=== x86_64 Microkernel Bounded TCP Stream Contract Guard ==="

require_file "$guest"
require_file "$kernel"
require_file "$network_task"
require_file "$scenarios"
require_file "$tcp_stream"
require_file "$roadmap"
require_file "$runner"
require_file "$root_manifest"
require_file "$smoke_manifest"
require_file "$core_manifest"
require_file "$lockfile"

require_literal "$roadmap" "Stage J1: Bounded TCP Stream Prerequisite" \
  "roadmap must define Stage J1"
require_literal "$roadmap" "one static server-side listener with fixed session slots is enough" \
  "roadmap must keep Stage J1 bounded to one static server-side listener with fixed session slots"
require_literal "$roadmap" "in-order RX payload buffering only" \
  "roadmap must require bounded in-order RX buffering"
require_literal "$roadmap" "bounded TX segmentation" \
  "roadmap must require bounded TX segmentation"
require_literal "$roadmap" "explicit ACK/FIN/RST handling" \
  "roadmap must require explicit ACK/FIN/RST handling"
require_literal "$roadmap" "no heap allocation" \
  "roadmap must forbid heap allocation for Stage J1"
require_literal "$roadmap" "no TLS dependency" \
  "roadmap must forbid TLS dependency for Stage J1"

require_literal "$tcp_stream" "struct BoundedTcpStream" \
  "guest must define a bounded TCP stream seam"
require_literal "$tcp_stream" "enum TcpStreamState" \
  "guest must define explicit TCP stream state"
require_literal "$network_task" "const TCP_STREAM_RX_BYTES" \
  "guest must define a fixed RX stream budget"
require_literal "$network_task" "const TCP_STREAM_TX_BYTES" \
  "guest must define a fixed TX stream budget"
require_literal "$network_task" "http_streams: [BoundedTcpStream; TCP_STREAM_SESSIONS]" \
  "TcpIpTask must own fixed bounded stream slots"
require_literal "$tcp_stream" "ingest_in_order_payload" \
  "bounded stream must ingest in-order payload bytes"
require_literal "$tcp_stream" "request_complete" \
  "bounded stream must gate HTTP on request completion"
require_literal "$tcp_stream" "queue_send_bytes" \
  "bounded stream must queue TX bytes before packet serialization"
require_literal "$tcp_stream" "emit_next_segment" \
  "bounded stream must expose bounded TX segment emission"
require_literal "$tcp_stream" "accept_fin" \
  "bounded stream must explicitly handle FIN"
require_literal "$tcp_stream" "accept_rst" \
  "bounded stream must explicitly handle RST"
require_literal "$tcp_stream" "ack_segment" \
  "bounded stream must expose an ACK segment helper"
require_literal "$network_task" "NetworkReplyKind::TcpAck" \
  "guest must have an explicit TCP ACK reply kind"
require_literal "$tcp_stream" "NetworkReplyKind::TcpFinAck" \
  "guest must have an explicit TCP FIN ACK reply kind"
require_literal "$network_task" "http_task.build_response(request," \
  "HTTP must consume request bytes from the stream seam"
require_literal "$network_task" "stream.queued_tx(&segment)" \
  "TCP packet serializer must consume queued stream TX bytes"
require_literal "$runner" 'request_parts = session.get("request_parts")' \
  "runner must keep source-visible split-request plumbing for bounded HTTP request sequencing"
require_literal "$scenarios" 'let partial_payload = b"GET /INDEX.HTM HTTP/1.0\r\n";' \
  "guest must keep the partial HTTP request fixture used by the bounded stream proof"
require_literal "$scenarios" 'let complete_payload = b"GET /INDEX.HTM HTTP/1.0\r\nHost: dataplane\r\n\r\n";' \
  "guest must keep the complete HTTP request fixture used by the bounded stream proof"
require_literal "$runner" "--bounded-tcp-negative-proof" \
  "runner must expose a bounded TCP negative proof mode"
require_literal "$runner" "DP_MICROKERNEL_BOUNDED_TCP_NEGATIVE_PROOF" \
  "runner must expose an environment-selectable bounded TCP negative proof mode"
require_literal "$runner" "bounded_tcp_negative_cases=duplicate_payload,partial_request_no_response,session_exhaustion,fin_during_response,overflow_payload,reset_before_completion" \
  "runner must record the full bounded TCP negative case set it exercised"
require_literal "$runner" '"behavior": "duplicate_payload"' \
  "runner must externally exercise duplicate payload handling"
require_literal "$runner" '"behavior": "partial_request_no_response"' \
  "runner must externally exercise partial request no-response handling"
require_literal "$runner" '"behavior": "session_exhaustion_hold"' \
  "runner must externally hold enough sessions to exhaust TCP slots"
require_literal "$runner" '"behavior": "session_exhaustion_probe"' \
  "runner must externally probe session exhaustion"
require_literal "$runner" '"behavior": "fin_during_response"' \
  "runner must externally exercise FIN during response"
require_literal "$runner" "bounded_tcp_overflow_payload_bytes=473" \
  "runner must record oversized payload bytes for the overflow negative case"
require_literal "$runner" "bounded_tcp_overflow_payload_host_handshake=true" \
  "runner must fail closed unless overflow payload handshake succeeds"
require_literal "$runner" "bounded_tcp_overflow_payload_sent=true" \
  "runner must fail closed unless overflow payload is sent"
require_literal "$runner" "bounded_tcp_overflow_payload_no_response=true" \
  "runner must fail closed unless overflow payload produces no-response evidence"
require_literal "$runner" "bounded_tcp_reset_before_completion_host_handshake=true" \
  "runner must fail closed unless reset-before-completion handshake succeeds"
require_literal "$runner" "bounded_tcp_reset_before_completion_no_response=true" \
  "runner must fail closed unless reset-before-completion produces no-response evidence"
require_literal "$runner" "bounded_tcp_reset_before_completion_rst_sent=true" \
  "runner must record host-side RST evidence for reset-before-completion"
require_literal "$runner" "bounded_tcp_duplicate_payload_host_handshake=true" \
  "runner must fail closed unless duplicate payload handshake succeeds"
require_literal "$runner" "bounded_tcp_duplicate_payload_sent=true" \
  "runner must fail closed unless duplicate payload is sent"
require_literal "$runner" "bounded_tcp_duplicate_payload_replayed=true" \
  "runner must fail closed unless duplicate payload is replayed"
require_literal "$runner" "bounded_tcp_duplicate_payload_no_second_response=true" \
  "runner must fail closed unless duplicate payload produces no second response"
require_literal "$runner" "bounded_tcp_duplicate_payload_http_response_count=1" \
  "runner must record exactly one HTTP response for duplicate payload"
require_literal "$runner" "bounded_tcp_partial_request_host_handshake=true" \
  "runner must fail closed unless partial request handshake succeeds"
require_literal "$runner" "bounded_tcp_partial_request_payload_sent=true" \
  "runner must fail closed unless partial request payload is sent"
require_literal "$runner" "bounded_tcp_partial_request_no_response=true" \
  "runner must fail closed unless partial request has no response"
require_literal "$runner" "bounded_tcp_session_exhaustion_open_sessions=4" \
  "runner must prove four held sessions for session exhaustion"
require_literal "$runner" "bounded_tcp_session_exhaustion_extra_syn_sent=true" \
  "runner must send an extra SYN for session exhaustion"
require_literal "$runner" "bounded_tcp_session_exhaustion_no_slot_reuse=true" \
  "runner must fail closed unless session exhaustion avoids slot reuse"
require_literal "$runner" "bounded_tcp_session_exhaustion_no_http_response=true" \
  "runner must fail closed unless exhausted session produces no HTTP response"
require_literal "$runner" "bounded_tcp_fin_during_response_host_handshake=true" \
  "runner must fail closed unless FIN-during-response handshake succeeds"
require_literal "$runner" "bounded_tcp_fin_during_response_fin_sent=true" \
  "runner must send FIN during response"
require_literal "$runner" "bounded_tcp_fin_during_response_http_response_seen=true" \
  "runner must observe the response for FIN-during-response"
require_literal "$runner" "bounded_tcp_fin_during_response_fin_ack_seen=true" \
  "runner must observe FIN ACK for FIN-during-response"
require_literal "$runner" "bounded TCP negative proof pcap was not written" \
  "runner must reject marker-only negative proof without a QEMU pcap"
require_literal "$runner" "bounded TCP negative proof pcap contains only a global header" \
  "runner must reject empty QEMU pcap evidence for bounded TCP negative proof"
require_literal "$runner" "bounded_tcp_tls_deferred=true" \
  "runner must keep bounded TCP negative proof explicitly separate from TLS"

awk '
  /struct BoundedTcpStream/ { in_range = 1 }
  /fn tcp_server_seq/ { in_range = 0 }
  in_range { print }
' "$tcp_stream" >"$tmp"
if [[ ! -s "$tmp" ]]; then
  echo "failed to extract BoundedTcpStream source range" >&2
  exit 1
fi

reject_regex "$tmp" 'Vec<|VecDeque|Box<|String|format!|alloc::' \
  "bounded TCP stream seam must stay heap-free"
reject_regex "$network_task" 'http_task\.build_response\(request_payload' \
  "HTTP must not be built directly from the current TCP frame payload"
reject_regex "$network_task" 'DPMK:TLS' \
  "guest must not add TLS markers during Stage J1"
reject_regex "$runner" 'tls-proof|DP_MICROKERNEL_TLS_PROOF|https://|openssl|curl -k' \
  "runner must not add TLS proof plumbing during Stage J1"
reject_regex "$runner" 'bounded_tcp_negative_cases_not_externally_covered|external_blocked=guest_tcp_session_peer' \
  "bounded TCP stream proof must not pass with deferred external negative cases"

for path in "$root_manifest" "$smoke_manifest" "$core_manifest" "$lockfile"; do
  reject_regex "$path" '(^name = "|[[:space:]])(rustls|embedded-tls|webpki|ring|aws-lc-rs|aws_lc|embedded_tls)("|[[:space:]=])' \
    "TLS dependencies must not be added during Stage J1: $path"
done

echo "x86_64 microkernel bounded TCP stream contract guard passed."
