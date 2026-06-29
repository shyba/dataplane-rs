#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

source "tools/x86_64_microkernel_validation_matrix_lib.sh"

log_root="$(dp_default_log_root DP_HTTP_POLICY_MATRIX_LOG_ROOT)"
run_id="$(dp_new_run_id)"
summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-http-policy-matrix" "$run_id" "summary")"
curl_run_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-http-policy-matrix" "$run_id" "curl-run.log")"
operator_run_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-http-policy-matrix" "$run_id" "operator-run.log")"
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

extract_existing_artifact() {
  local log_file="$1"
  local label="$2"
  local line path candidate
  if [[ ! -f "$log_file" ]]; then
    echo "missing log file for artifact extraction: $log_file" >&2
    return 1
  fi
  line=""
  while IFS= read -r candidate; do
    case "$candidate" in
      "$label: "*) line="$candidate" ;;
    esac
  done <"$log_file"
  if [[ -z "$line" ]]; then
    echo "missing artifact label '$label' in $log_file" >&2
    return 1
  fi
  path="${line#"$label: "}"
  if [[ -z "$path" ]]; then
    echo "empty artifact path for '$label' in $log_file" >&2
    return 1
  fi
  if [[ "$path" != /home/user/mnt/dataplane/* ]]; then
    echo "artifact '$label' is outside /home/user/mnt/dataplane: $path" >&2
    return 1
  fi
  if [[ ! -e "$path" ]]; then
    echo "artifact '$label' does not exist: $path" >&2
    return 1
  fi
  printf '%s' "$path"
}

echo "=== x86_64 Microkernel HTTP Policy Matrix ==="

./tools/x86_64_microkernel_fat32_run.sh --curl-proof >"$curl_run_log" 2>&1
./tools/x86_64_microkernel_fat32_run.sh --operator-appliance-proof >"$operator_run_log" 2>&1

curl_log="$(dp_extract_artifact "$curl_run_log" "curl log")"
curl_get_body="$(dp_extract_artifact "$curl_run_log" "curl GET body")"
curl_get_headers="$(dp_extract_artifact "$curl_run_log" "curl GET headers")"
curl_get_status="$(dp_extract_artifact "$curl_run_log" "curl GET status")"
curl_head_body="$(extract_existing_artifact "$curl_run_log" "curl HEAD body")"
curl_head_headers="$(dp_extract_artifact "$curl_run_log" "curl HEAD headers")"
curl_head_status="$(dp_extract_artifact "$curl_run_log" "curl HEAD status")"
curl_missing_status="$(dp_extract_artifact "$curl_run_log" "curl missing status")"
curl_method_status="$(dp_extract_artifact "$curl_run_log" "curl method-not-allowed status")"
curl_overlong_status="$(dp_extract_artifact "$curl_run_log" "curl overlong-request status")"
curl_malformed_status="$(dp_extract_artifact "$curl_run_log" "curl malformed-request status")"
curl_malformed_raw="$(dp_extract_artifact "$curl_run_log" "curl malformed-request raw")"
curl_pcap="$(dp_extract_artifact "$curl_run_log" "curl pcap")"
curl_serial_log="$(dp_extract_artifact "$curl_run_log" "serial log")"
curl_qemu_log="$(dp_extract_artifact "$curl_run_log" "qemu log")"

operator_client_log="$(dp_extract_artifact "$operator_run_log" "client log")"
operator_pcap="$(dp_extract_artifact "$operator_run_log" "network pcap")"
operator_serial_log="$(dp_extract_artifact "$operator_run_log" "serial log")"
operator_qemu_log="$(dp_extract_artifact "$operator_run_log" "qemu log")"

require_summary_value "$curl_log" "curl_get_index_ok" "true"
require_summary_value "$curl_log" "curl_head_index_ok" "true"
require_summary_value "$curl_log" "curl_missing_ok" "true"
require_summary_value "$curl_log" "curl_method_not_allowed_ok" "true"
require_summary_value "$curl_log" "curl_overlong_request_ok" "true"
require_summary_value "$curl_log" "curl_malformed_request_ok" "true"
require_summary_value "$curl_log" "curl_get_index_status" "200"
require_summary_value "$curl_log" "curl_head_index_status" "200"
require_summary_value "$curl_log" "curl_missing_status" "404"
require_summary_value "$curl_log" "curl_method_not_allowed_status" "405"
require_summary_value "$curl_log" "curl_overlong_request_status" "413"
require_summary_value "$curl_log" "curl_malformed_request_status" "413"
require_summary_value "$curl_log" "curl_head_index_body_bytes" "0"
require_summary_value "$curl_log" "curl_malformed_request_artifact_bytes" "7"
require_contains "$curl_get_headers" "Connection: close" "GET response must use bounded close behavior"
require_contains "$curl_head_headers" "Connection: close" "HEAD response must use bounded close behavior"
require_contains "$curl_serial_log" "DPMK:HTTP-GET-OK" "serial log must contain GET marker"
require_contains "$curl_serial_log" "DPMK:HTTP-HEAD-OK" "serial log must contain HEAD marker"
require_contains "$curl_serial_log" "DPMK:HTTP-404-OK" "serial log must contain missing-file marker"
require_contains "$curl_serial_log" "DPMK:HTTP-405-OK" "serial log must contain unsupported-method marker"
require_contains "$curl_serial_log" "DPMK:HTTP-413-OK" "serial log must contain oversized/malformed marker"
require_contains "$operator_serial_log" "DPMK:FS-READ-MATRIX-SMALL-HTTP-OK" \
  "HTTP index response must be covered by service-backed filesystem evidence"
require_contains "$operator_serial_log" "DPMK:FS-READ-MATRIX-CHAIN-HTTP-OK" \
  "HTTP multi-cluster response must be covered by service-backed filesystem evidence"
require_summary_value "$operator_client_log" "operator_status_page_ok" "true"
require_summary_value "$operator_client_log" "operator_status_page_status" "200"
require_summary_value "$operator_client_log" "operator_status_page_bounded_ok" "true"
require_summary_value "$operator_client_log" "operator_status_agreement_ok" "true"
require_contains "$operator_serial_log" "DPMK:HTTP-STATUS-PAGE-OK" "serial log must contain status page marker"

{
  echo "http_policy_matrix_run_id=$run_id"
  echo "http_policy_matrix_log_root=$log_root"
  echo "http_policy_matrix_summary=$summary"
  echo "http_policy_matrix_status=pass"
  echo "http_policy_matrix_get_ok=true"
  echo "http_policy_matrix_head_ok=true"
  echo "http_policy_matrix_missing_file_ok=true"
  echo "http_policy_matrix_unsupported_method_ok=true"
  echo "http_policy_matrix_malformed_request_ok=true"
  echo "http_policy_matrix_oversized_request_ok=true"
  echo "http_policy_matrix_bounded_close_ok=true"
  echo "http_policy_matrix_status_page_ok=true"
  echo "http_policy_matrix_fs_task_route_ok=true"
  echo "curl_log=$curl_log"
  echo "curl_get_body=$curl_get_body"
  echo "curl_get_headers=$curl_get_headers"
  echo "curl_get_status=$curl_get_status"
  echo "curl_head_body=$curl_head_body"
  echo "curl_head_headers=$curl_head_headers"
  echo "curl_head_status=$curl_head_status"
  echo "curl_missing_status=$curl_missing_status"
  echo "curl_method_not_allowed_status=$curl_method_status"
  echo "curl_overlong_request_status=$curl_overlong_status"
  echo "curl_malformed_request_status=$curl_malformed_status"
  echo "curl_malformed_request_raw=$curl_malformed_raw"
  echo "curl_pcap=$curl_pcap"
  echo "curl_serial_log=$curl_serial_log"
  echo "curl_qemu_log=$curl_qemu_log"
  echo "operator_client_log=$operator_client_log"
  echo "operator_network_pcap=$operator_pcap"
  echo "operator_serial_log=$operator_serial_log"
  echo "operator_qemu_log=$operator_qemu_log"
} >"$summary"

echo "x86_64 microkernel HTTP policy matrix proof passed."
dp_emit_artifact_line "HTTP policy matrix summary" "$summary"
dp_emit_artifact_line "curl log" "$curl_log"
dp_emit_artifact_line "curl pcap" "$curl_pcap"
dp_emit_artifact_line "operator client log" "$operator_client_log"
dp_emit_artifact_line "operator network pcap" "$operator_pcap"
