#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

plan="aidocs/051_microkernel_pre_tls_appliance_plan_2026-05-30.md"
diary="aidocs/050_microkernel_robust_design_diary_2026-05-30.md"
runner="tools/x86_64_microkernel_http_policy_matrix_run.sh"
fat32_runner="tools/x86_64_microkernel_fat32_run.sh"
matrix_runner="tools/x86_64_microkernel_validation_matrix_run.sh"
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

echo "=== x86_64 Microkernel HTTP Policy Matrix Contract Guard ==="

for path in "$plan" "$diary" "$runner" "$fat32_runner" "$matrix_runner" "$helper" "$makefile"; do
  require_file "$path"
done

require_literal "$plan" "#### Queue 4: HTTP Appliance Policy Matrix" \
  "pre-TLS plan must name HTTP appliance policy matrix queue"
require_literal "$plan" "host-visible client evidence for GET, HEAD, missing file, unsupported method," \
  "pre-TLS plan must require core HTTP policy cases"
require_literal "$diary" "## Round 25 Plan" \
  "diary must include the active HTTP policy round"
require_literal "$diary" 'Implement `x86_64-microkernel-http-policy-matrix`' \
  "diary must identify the HTTP policy target"

require_literal "$runner" 'log_root="$(dp_default_log_root DP_HTTP_POLICY_MATRIX_LOG_ROOT)"' \
  "HTTP policy runner must use shared log-root helper"
require_literal "$runner" 'summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-http-policy-matrix" "$run_id" "summary")"' \
  "HTTP policy runner must write a summary artifact"
require_literal "$runner" './tools/x86_64_microkernel_fat32_run.sh --curl-proof >"$curl_run_log" 2>&1' \
  "HTTP policy runner must reuse the host curl proof"
require_literal "$runner" './tools/x86_64_microkernel_fat32_run.sh --operator-appliance-proof >"$operator_run_log" 2>&1' \
  "HTTP policy runner must reuse the status page proof"
require_literal "$runner" 'require_summary_value "$curl_log" "curl_get_index_status" "200"' \
  "HTTP policy runner must require GET 200 evidence"
require_literal "$runner" 'require_summary_value "$curl_log" "curl_head_index_body_bytes" "0"' \
  "HTTP policy runner must require body-free HEAD evidence"
require_literal "$runner" 'require_summary_value "$curl_log" "curl_missing_status" "404"' \
  "HTTP policy runner must require missing-file 404 evidence"
require_literal "$runner" 'require_summary_value "$curl_log" "curl_method_not_allowed_status" "405"' \
  "HTTP policy runner must require unsupported-method 405 evidence"
require_literal "$runner" 'require_summary_value "$curl_log" "curl_overlong_request_status" "413"' \
  "HTTP policy runner must require oversized-request 413 evidence"
require_literal "$runner" 'require_summary_value "$curl_log" "curl_malformed_request_status" "413"' \
  "HTTP policy runner must require malformed-request 413 evidence"
require_literal "$runner" 'require_contains "$curl_get_headers" "Connection: close"' \
  "HTTP policy runner must require bounded close headers"
require_literal "$runner" 'require_contains "$operator_serial_log" "DPMK:FS-READ-MATRIX-SMALL-HTTP-OK"' \
  "HTTP policy runner must require service-backed small-file HTTP evidence"
require_literal "$runner" 'require_contains "$operator_serial_log" "DPMK:FS-READ-MATRIX-CHAIN-HTTP-OK"' \
  "HTTP policy runner must require service-backed multi-cluster HTTP evidence"
require_literal "$runner" 'require_summary_value "$operator_client_log" "operator_status_page_ok" "true"' \
  "HTTP policy runner must require status page evidence"
require_literal "$runner" 'http_policy_matrix_status_page_ok=true' \
  "HTTP policy summary must record status page success"
require_literal "$runner" 'dp_emit_artifact_line "HTTP policy matrix summary" "$summary"' \
  "HTTP policy runner must emit summary artifact label"

require_literal "$matrix_runner" "http-policy-matrix) echo 'make x86_64-microkernel-http-policy-matrix' ;;" \
  "validation matrix must dispatch HTTP policy through Make"
require_literal "$matrix_runner" "http-policy-matrix) echo 'x86_64 microkernel HTTP policy matrix proof passed.' ;;" \
  "validation matrix must require HTTP policy marker"
require_literal "$matrix_runner" "http-policy-matrix) echo 'HTTP policy matrix summary|curl log|curl pcap|operator client log|operator network pcap' ;;" \
  "validation matrix must require HTTP policy artifacts"
require_literal "$makefile" "x86_64-microkernel-http-policy-matrix-contract:" \
  "Makefile must expose HTTP policy contract target"
require_literal "$makefile" "x86_64-microkernel-http-policy-matrix: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-validation-matrix-contract x86_64-microkernel-http-policy-matrix-contract" \
  "Makefile must expose HTTP policy proof target"

require_literal "$fat32_runner" 'curl_method_not_allowed_status=405' \
  "curl proof must already validate unsupported method"
require_literal "$fat32_runner" 'curl_overlong_request_status=413' \
  "curl proof must already validate oversized request"
require_literal "$fat32_runner" 'curl_malformed_request_status=413' \
  "curl proof must already validate malformed request"
require_literal "$fat32_runner" 'operator_status_page_ok=true' \
  "operator proof must already validate status page"

reject_regex "$runner" 'TLS|HTTPS|OpenSSL|certificate|private[[:space:]]+key|curl[[:space:]]+-k|DP_MICROKERNEL_TLS_PROOF|--tls-proof' \
  "HTTP policy packet must not add TLS/HTTPS/certificate tooling"
reject_regex "$runner" 'keep-alive|chunked|CGI|scripting|generic[[:space:]]+socket|socket[[:space:]]+api|compiled-in[[:space:]]+shortcut' \
  "HTTP policy packet must not broaden HTTP scope"
reject_regex "$runner" 'benchmark[[:space:]]+tuning|retun(e|ing)|calibration|journaling|durability|default[[:space:]]+write' \
  "HTTP policy packet must not mix performance or storage semantics"
reject_regex "$fat32_runner" 'HTTPS|OpenSSL|certificate|private[[:space:]]+key|curl[[:space:]]+-k|DP_MICROKERNEL_TLS_PROOF|--tls-proof' \
  "reused FAT32 proof path must not add TLS/HTTPS/certificate tooling"
reject_regex "$fat32_runner" 'keep-alive|chunked|CGI|scripting|generic[[:space:]]+socket|socket[[:space:]]+api|compiled-in[[:space:]]+shortcut' \
  "reused FAT32 proof path must not broaden HTTP scope"

echo "x86_64 microkernel HTTP policy matrix contract guard passed."
