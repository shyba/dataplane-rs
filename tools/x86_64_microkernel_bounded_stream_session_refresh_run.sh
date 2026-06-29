#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

source "tools/x86_64_microkernel_validation_matrix_lib.sh"

log_root="$(dp_default_log_root DP_BOUNDED_STREAM_SESSION_REFRESH_LOG_ROOT)"
run_id="$(dp_new_run_id)"
summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-bounded-stream-session-refresh" "$run_id" "summary")"
stream_run_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-bounded-stream-session-refresh" "$run_id" "stream-run.log")"
http_policy_run_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-bounded-stream-session-refresh" "$run_id" "http-policy-run.log")"
pcap_metrics="$(dp_artifact_path "$log_root" "x86_64-microkernel-bounded-stream-session-refresh" "$run_id" "pcap.metrics")"
mkdir -p "$log_root"

require_summary_value() {
  local file="$1"
  local key="$2"
  local expected="$3"
  if ! dp_require_key_value "$file" "$key" "$expected"; then
    echo "FAIL: expected $key=$expected in $file" >&2
    return 1
  fi
}

require_contains() {
  local file="$1"
  local needle="$2"
  local note="$3"
  if ! dp_require_marker "$file" "$needle"; then
    echo "FAIL: $note" >&2
    return 1
  fi
}

validate_session_pcap() {
  local pcap="$1"
  python3 - "$pcap" <<'PY'
import struct
import sys
from pathlib import Path

pcap = Path(sys.argv[1])
data = pcap.read_bytes()
if len(data) < 24:
    raise SystemExit("pcap is too small")

magic = data[:4]
if magic in (b"\xd4\xc3\xb2\xa1", b"\x4d\x3c\xb2\xa1"):
    endian = "<"
elif magic in (b"\xa1\xb2\xc3\xd4", b"\xa1\xb2\x3c\x4d"):
    endian = ">"
else:
    raise SystemExit("unsupported pcap magic")

offset = 24
tcp_packets = 0
fin_packets = 0
payload_packets = 0

while offset + 16 <= len(data):
    _ts_sec, _ts_usec, incl_len, _orig_len = struct.unpack_from(endian + "IIII", data, offset)
    offset += 16
    packet = data[offset:offset + incl_len]
    offset += incl_len
    if len(packet) < 14:
        continue
    eth_type = int.from_bytes(packet[12:14], "big")
    if eth_type != 0x0800:
        continue
    ip = packet[14:]
    if len(ip) < 20:
        continue
    version = ip[0] >> 4
    ihl = (ip[0] & 0x0f) * 4
    if version != 4 or ihl < 20 or len(ip) < ihl:
        continue
    total_len = int.from_bytes(ip[2:4], "big")
    if total_len < ihl or len(ip) < total_len:
        continue
    if ip[9] != 6:
        continue
    tcp = ip[ihl:total_len]
    if len(tcp) < 20:
        continue
    data_offset = (tcp[12] >> 4) * 4
    if data_offset < 20 or len(tcp) < data_offset:
        continue
    tcp_packets += 1
    flags = tcp[13]
    if flags & 0x01:
        fin_packets += 1
    if len(tcp) > data_offset:
        payload_packets += 1

if tcp_packets <= 0:
    raise SystemExit("pcap has no TCP packets")
if fin_packets <= 0:
    raise SystemExit("pcap has no FIN packets")
if payload_packets <= 0:
    raise SystemExit("pcap has no TCP payload packets")

print(f"stream_session_refresh_pcap_bytes={len(data)}")
print(f"stream_session_refresh_pcap_tcp_packets={tcp_packets}")
print(f"stream_session_refresh_pcap_fin_packets={fin_packets}")
print(f"stream_session_refresh_pcap_payload_packets={payload_packets}")
PY
}

echo "=== x86_64 Microkernel Bounded Stream Session Refresh ==="

./tools/x86_64_microkernel_fat32_run.sh --stream-session-state-proof >"$stream_run_log" 2>&1
./tools/x86_64_microkernel_http_policy_matrix_run.sh >"$http_policy_run_log" 2>&1

stream_summary="$(dp_extract_artifact "$stream_run_log" "stream session state summary")"
stream_client_log="$(dp_extract_artifact "$stream_run_log" "client log")"
stream_serial_log="$(dp_extract_artifact "$stream_run_log" "serial log")"
stream_qemu_log="$(dp_extract_artifact "$stream_run_log" "qemu log")"
stream_pcap="$(dp_extract_artifact "$stream_run_log" "network pcap")"
http_policy_summary="$(dp_extract_artifact "$http_policy_run_log" "HTTP policy matrix summary")"
http_curl_pcap="$(dp_extract_artifact "$http_policy_run_log" "curl pcap")"
http_operator_pcap="$(dp_extract_artifact "$http_policy_run_log" "operator network pcap")"

for key in \
  stream_session_state_summary_status \
  stream_session_state_tls_deferred \
  stream_session_state_accepted_ok \
  stream_session_state_active_zero_ok \
  stream_session_state_peak_ok \
  stream_session_state_full_peak_ok \
  stream_session_state_reset_ok \
  stream_session_state_fin_ok \
  stream_session_state_partial_ok \
  stream_session_state_duplicate_ok \
  stream_session_state_rst_before_complete_ok \
  stream_session_state_backpressure_ok \
  stream_session_state_overflow_ok \
  stream_session_state_fin_during_response_ok \
  stream_session_state_control_ok; do
  expected="true"
  if [[ "$key" == "stream_session_state_summary_status" ]]; then
    expected="pass"
  fi
  require_summary_value "$stream_summary" "$key" "$expected"
done

for marker in \
  "DPMK:TCP-SESSION-ACCEPTED:1" \
  "DPMK:TCP-SESSION-ACTIVE:0" \
  "DPMK:TCP-SESSION-PEAK:1" \
  "DPMK:TCP-SESSION-FULL-PEAK:4" \
  "DPMK:TCP-SESSION-STATE:reset" \
  "DPMK:TCP-SESSION-FIN-STATE:fin-wait" \
  "DPMK:TCP-CTRL-PARTIAL:1" \
  "DPMK:TCP-CTRL-DUPLICATE:1" \
  "DPMK:TCP-CTRL-RST-BEFORE-COMPLETE:1" \
  "DPMK:TCP-CTRL-SESSION-FULL:1" \
  "DPMK:TCP-CTRL-RX-OVERFLOW:1" \
  "DPMK:TCP-CTRL-FIN-DURING-RESPONSE:1" \
  "DPMK:TCP-CTRL-OK"; do
  require_contains "$stream_serial_log" "$marker" "stream/session serial marker missing: $marker"
done

validate_session_pcap "$stream_pcap" >"$pcap_metrics"
require_summary_value "$http_policy_summary" "http_policy_matrix_status" "pass"

{
  echo "bounded_stream_session_refresh_run_id=$run_id"
  echo "bounded_stream_session_refresh_status=pass"
  echo "bounded_stream_session_refresh_mode=post-http-policy-host-visible-session-refresh"
  echo "bounded_stream_session_refresh_session_counters_ok=true"
  echo "bounded_stream_session_refresh_rst_before_complete_ok=true"
  echo "bounded_stream_session_refresh_fin_during_response_ok=true"
  echo "bounded_stream_session_refresh_pcap_ok=true"
  echo "bounded_stream_session_refresh_http_policy_regression_ok=true"
  cat "$pcap_metrics"
  echo "bounded_stream_session_refresh_stream_summary=$stream_summary"
  echo "bounded_stream_session_refresh_stream_client_log=$stream_client_log"
  echo "bounded_stream_session_refresh_stream_serial_log=$stream_serial_log"
  echo "bounded_stream_session_refresh_stream_qemu_log=$stream_qemu_log"
  echo "bounded_stream_session_refresh_stream_pcap=$stream_pcap"
  echo "bounded_stream_session_refresh_http_policy_summary=$http_policy_summary"
  echo "bounded_stream_session_refresh_http_curl_pcap=$http_curl_pcap"
  echo "bounded_stream_session_refresh_http_operator_pcap=$http_operator_pcap"
} >"$summary"

echo "x86_64 microkernel bounded stream session refresh proof passed."
dp_emit_artifact_line "bounded stream session refresh summary" "$summary"
dp_emit_artifact_line "stream session state summary" "$stream_summary"
dp_emit_artifact_line "stream session client log" "$stream_client_log"
dp_emit_artifact_line "stream session serial log" "$stream_serial_log"
dp_emit_artifact_line "stream session pcap" "$stream_pcap"
dp_emit_artifact_line "HTTP policy matrix summary" "$http_policy_summary"
dp_emit_artifact_line "HTTP policy curl pcap" "$http_curl_pcap"
