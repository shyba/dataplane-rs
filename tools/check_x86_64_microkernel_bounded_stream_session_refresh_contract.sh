#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

plan="aidocs/051_microkernel_pre_tls_appliance_plan_2026-05-30.md"
frontier="aidocs/050_microkernel_robust_design_frontier_2026-05-30.md"
diary="aidocs/050_microkernel_robust_design_diary_2026-05-30.md"
runner="tools/x86_64_microkernel_bounded_stream_session_refresh_run.sh"
fat32_runner="tools/x86_64_microkernel_fat32_run.sh"
stream_guard="tools/check_x86_64_microkernel_stream_session_state_contract.sh"
http_guard="tools/check_x86_64_microkernel_http_policy_matrix_contract.sh"
matrix_runner="tools/x86_64_microkernel_validation_matrix_run.sh"
matrix_guard="tools/check_x86_64_microkernel_validation_matrix_contract.sh"
helper="tools/x86_64_microkernel_validation_matrix_lib.sh"
makefile="Makefile"

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

echo "=== x86_64 Microkernel Bounded Stream Session Refresh Contract Guard ==="

for path in \
  "$plan" \
  "$frontier" \
  "$diary" \
  "$runner" \
  "$fat32_runner" \
  "$stream_guard" \
  "$http_guard" \
  "$matrix_runner" \
  "$matrix_guard" \
  "$helper" \
  "$makefile"; do
  require_file "$path"
done

require_literal "$plan" "## 2026-05-31 TLS Deferral Expansion" \
  "pre-TLS plan must include the explicit deferral expansion"
require_literal "$plan" "Bounded stream/session refresh." \
  "pre-TLS plan must name the bounded stream/session refresh replacement work"
require_literal "$plan" "pcap proof" \
  "pre-TLS plan must require host-visible stream/session pcap proof"
require_literal "$frontier" "x86_64-microkernel-bounded-stream-session-refresh" \
  "frontier must route the bounded stream/session refresh"
require_literal "$diary" "## Round 26 Plan" \
  "diary must include the active bounded stream/session round"

require_literal "$runner" 'log_root="$(dp_default_log_root DP_BOUNDED_STREAM_SESSION_REFRESH_LOG_ROOT)"' \
  "refresh runner must use shared log-root helper"
require_literal "$runner" 'summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-bounded-stream-session-refresh" "$run_id" "summary")"' \
  "refresh runner must write a summary artifact"
require_literal "$runner" './tools/x86_64_microkernel_fat32_run.sh --stream-session-state-proof >"$stream_run_log" 2>&1' \
  "refresh runner must reuse stream-session-state proof"
require_literal "$runner" './tools/x86_64_microkernel_http_policy_matrix_run.sh >"$http_policy_run_log" 2>&1' \
  "refresh runner must rerun HTTP policy as regression evidence"
require_literal "$runner" 'validate_session_pcap()' \
  "refresh runner must validate host-visible pcap"
require_literal "$runner" 'python3 - "$pcap" <<' \
  "refresh runner must use python3 for pcap parsing"
require_literal "$runner" 'stream_session_refresh_pcap_tcp_packets=' \
  "refresh runner must report TCP packet count"
require_literal "$runner" 'stream_session_refresh_pcap_fin_packets=' \
  "refresh runner must report FIN packet count"
require_literal "$runner" 'stream_session_refresh_pcap_payload_packets=' \
  "refresh runner must report TCP payload packet count"
require_literal "$runner" 'require_summary_value "$stream_summary" "$key" "$expected"' \
  "refresh runner must validate stream summary key/value evidence"
require_literal "$runner" '"DPMK:TCP-CTRL-RST-BEFORE-COMPLETE:1"' \
  "refresh runner must require reset-before-complete marker"
require_literal "$runner" '"DPMK:TCP-CTRL-FIN-DURING-RESPONSE:1"' \
  "refresh runner must require FIN-during-response marker"
require_literal "$runner" 'require_summary_value "$http_policy_summary" "http_policy_matrix_status" "pass"' \
  "refresh runner must require HTTP policy regression success"
require_literal "$runner" 'bounded_stream_session_refresh_pcap_ok=true' \
  "refresh summary must record pcap validation"
require_literal "$runner" 'bounded_stream_session_refresh_http_policy_regression_ok=true' \
  "refresh summary must record HTTP policy regression"
require_literal "$runner" 'dp_emit_artifact_line "bounded stream session refresh summary" "$summary"' \
  "refresh runner must emit a summary artifact label"
require_literal "$runner" 'dp_emit_artifact_line "stream session pcap" "$stream_pcap"' \
  "refresh runner must emit a stream pcap artifact label"
require_literal "$runner" 'x86_64 microkernel bounded stream session refresh proof passed.' \
  "refresh runner must emit the positive marker"

require_literal "$fat32_runner" 'bounded_tcp_reset_before_completion_no_response=true' \
  "FAT32 stream proof must summarize reset-before-complete evidence"
require_literal "$fat32_runner" 'bounded_tcp_fin_during_response_fin_ack_seen=true' \
  "FAT32 stream proof must summarize FIN-during-response evidence"
require_literal "$stream_guard" 'bounded_tcp_reset_before_completion_no_response=true' \
  "stream/session contract must guard reset-before-complete summary evidence"
require_literal "$stream_guard" 'bounded_tcp_fin_during_response_fin_ack_seen=true' \
  "stream/session contract must guard FIN-during-response summary evidence"

require_literal "$matrix_runner" "bounded-stream-session-refresh) echo 'make x86_64-microkernel-bounded-stream-session-refresh' ;;" \
  "validation matrix must dispatch bounded stream/session refresh through Make"
require_literal "$matrix_runner" "bounded-stream-session-refresh) echo 'x86_64 microkernel bounded stream session refresh proof passed.' ;;" \
  "validation matrix must require bounded stream/session refresh positive marker"
require_literal "$matrix_runner" "bounded-stream-session-refresh) echo 'bounded stream session refresh summary|stream session state summary|stream session client log|stream session serial log|stream session pcap|HTTP policy matrix summary|HTTP policy curl pcap' ;;" \
  "validation matrix must require bounded stream/session refresh artifacts"
require_literal "$matrix_guard" "'bounded-stream-session-refresh'" \
  "validation matrix contract must know the bounded stream/session refresh scenario"

require_literal "$makefile" 'x86_64-microkernel-bounded-stream-session-refresh-contract:' \
  "Makefile must expose bounded stream/session refresh contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_bounded_stream_session_refresh_contract.sh' \
  "Makefile contract target must run the focused guard"
require_literal "$makefile" 'x86_64-microkernel-bounded-stream-session-refresh: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-bounded-tcp-stream-contract x86_64-microkernel-stream-session-state-contract x86_64-microkernel-http-policy-matrix-contract x86_64-microkernel-bounded-stream-session-refresh-contract' \
  "Makefile must expose bounded stream/session refresh proof target with neighboring guards"
require_literal "$makefile" './tools/x86_64_microkernel_bounded_stream_session_refresh_run.sh' \
  "Makefile bounded stream/session refresh target must run the focused wrapper"

reject_regex "$runner" 'https://|OpenSSL|certificate|private[[:space:]]+key|curl[[:space:]]+-k|DP_MICROKERNEL_TLS_PROOF|--tls-proof|rustls|embedded-tls|webpki|crypto-provider' \
  "bounded stream/session refresh must not add transport-security tooling"
reject_regex "$runner" 'keep-alive|pipelining|chunked|CGI|scripting|generic[[:space:]]+socket|socket[[:space:]]+api|TcpListener|TcpStream|heap-backed[[:space:]]+session|broad[[:space:]]+TCP[[:space:]]+compliance' \
  "bounded stream/session refresh must not broaden stream or HTTP scope"
reject_regex "$runner" 'STRICT_FIVE_CALIBRATION|benchmark[[:space:]]+tuning|retun(e|ing)|calibration' \
  "bounded stream/session refresh must not tune benchmarks"

echo "x86_64 microkernel bounded stream session refresh contract guard passed."
