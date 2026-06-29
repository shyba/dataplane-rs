#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

source "tools/x86_64_microkernel_validation_matrix_lib.sh"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_file() {
  local path="$1"
  [[ -f "$path" ]] || fail "required file missing: $path"
}

require_nonempty_under_log_root() {
  local path="$1"
  local log_root="$2"
  [[ -n "$path" ]] || fail "missing artifact path"
  [[ "$path" == "$log_root"/* ]] || fail "artifact path outside log root: $path"
  [[ -s "$path" ]] || fail "missing or empty artifact: $path"
}

require_fresh_file() {
  local path="$1"
  local start_epoch="$2"
  local mtime
  mtime="$(stat -c %Y "$path")" || fail "could not stat artifact: $path"
  (( mtime >= start_epoch )) || fail "stale artifact predates pack run: $path"
}

require_nonempty_under_log_root_and_fresh() {
  local path="$1"
  local log_root="$2"
  local start_epoch="$3"
  require_nonempty_under_log_root "$path" "$log_root"
  require_fresh_file "$path" "$start_epoch"
}

require_file_under_log_root_and_fresh() {
  local path="$1"
  local log_root="$2"
  local start_epoch="$3"
  [[ -n "$path" ]] || fail "missing artifact path"
  [[ "$path" == "$log_root"/* ]] || fail "artifact path outside log root: $path"
  [[ -e "$path" ]] || fail "missing artifact: $path"
  require_fresh_file "$path" "$start_epoch"
}

require_header_only_pcap_rejection() {
  local path="$1"
  [[ -s "$path" ]] || fail "missing or empty pcap: $path"
  local bytes
  bytes="$(wc -c <"$path" | tr -d ' ')"
  (( bytes > 24 )) || fail "header-only pcap: $path"
}

require_summary_key() {
  local file="$1"
  local key="$2"
  local expected="$3"
  dp_require_key_value "$file" "$key" "$expected" || fail "missing summary key: $key=$expected in $file"
}

capture_artifact() {
  local log_file="$1"
  local label="$2"
  local path
  path="$(dp_extract_artifact "$log_file" "$label")" || fail "missing artifact label '$label' in $log_file"
  printf '%s' "$path"
}

capture_latest_matching_artifact() {
  local log_root="$1"
  local pattern="$2"
  local path
  path="$(find "$log_root" -maxdepth 1 -type f -name "$pattern" -printf '%T@ %p\n' | sort -nr | awk 'NR==1 {print substr($0, index($0, $2))}')"
  [[ -n "$path" ]] || fail "missing matching artifact: $pattern under $log_root"
  [[ -s "$path" ]] || fail "missing or empty matching artifact: $path"
  printf '%s' "$path"
}

run_and_capture() {
  local label="$1"
  shift
  local log_file="$1"
  shift
  local status=0
  "$@" >"$log_file" 2>&1 || status=$?
  [[ "$status" -eq 0 ]] || {
    echo "--- $label log ---" >&2
    sed -n '1,220p' "$log_file" >&2 || true
    fail "$label command failed with status $status"
  }
}

if [[ "${1:-}" != "--appliance-load-fairness-pack-proof" ]]; then
  echo "FAIL: unknown mode: ${1:-<empty>}" >&2
  echo "usage: $0 --appliance-load-fairness-pack-proof" >&2
  exit 2
fi

log_root="$(dp_default_log_root DP_APPLIANCE_LOAD_FAIRNESS_PACK_LOG_ROOT)"
run_id="$(dp_new_run_id)"
summary="$log_root/x86_64-microkernel-appliance-load-fairness-pack-$run_id.summary"
mkdir -p "$log_root"
start_epoch="$(date +%s)"

http_policy_log="$log_root/x86_64-microkernel-appliance-load-fairness-pack-$run_id.http-heavy.command.log"
cli_parity_log="$log_root/x86_64-microkernel-appliance-load-fairness-pack-$run_id.cli-heavy.command.log"
fat32_log="$log_root/x86_64-microkernel-appliance-load-fairness-pack-$run_id.fat32-heavy.command.log"
network_log="$log_root/x86_64-microkernel-appliance-load-fairness-pack-$run_id.network-heavy.command.log"
timer_log="$log_root/x86_64-microkernel-appliance-load-fairness-pack-$run_id.timer-heavy.command.log"
fault_log="$log_root/x86_64-microkernel-appliance-load-fairness-pack-$run_id.contained-fault.command.log"

run_and_capture "http-heavy" "$http_policy_log" env DP_MICROKERNEL_CURL_PROOF=1 bash ./tools/x86_64_microkernel_fat32_run.sh --curl-proof
run_and_capture "cli-heavy" "$cli_parity_log" env DP_MICROKERNEL_CLI_HTTP_OPERATOR_PARITY_PROOF=1 bash ./tools/x86_64_microkernel_fat32_run.sh --cli-http-operator-parity-proof
run_and_capture "fat32-heavy" "$fat32_log" env DP_MICROKERNEL_SCHEDULER_FAIRNESS_LOAD_PROOF=1 bash ./tools/x86_64_microkernel_fat32_run.sh --scheduler-fairness-load-proof
run_and_capture "network-heavy" "$network_log" env DP_MICROKERNEL_NONTLS_NETWORK_SERVICE_PROOF=1 bash ./tools/x86_64_microkernel_fat32_run.sh --nontls-network-service-proof
run_and_capture "timer-heavy" "$timer_log" env TMPDIR="${TMPDIR:-/home/user/mnt/dataplane/tmp}" bash ./tools/x86_64_microkernel_timer_timeout_service.sh
run_and_capture "contained-fault" "$fault_log" env DP_MICROKERNEL_FAULT_POLICY_HARDENING_PROOF=1 bash ./tools/x86_64_microkernel_fat32_run.sh --fault-policy-hardening-proof

http_curl_log="$(capture_artifact "$http_policy_log" "curl log")"
http_curl_pcap="$(capture_artifact "$http_policy_log" "curl pcap")"
http_serial_log="$(capture_artifact "$http_policy_log" "serial log")"
http_qemu_log="$(capture_artifact "$http_policy_log" "qemu log")"

cli_summary="$(capture_latest_matching_artifact "$log_root" 'x86_64-microkernel-fat32-*.cli-http-operator-parity.summary')"
cli_run_id="${cli_summary##*/x86_64-microkernel-fat32-}"
cli_run_id="${cli_run_id%.cli-http-operator-parity.summary}"
cli_cli_transcript="$log_root/x86_64-microkernel-fat32-$cli_run_id.cli-http-operator-parity.cli-transcript.log"
cli_http_body="$log_root/x86_64-microkernel-fat32-$cli_run_id.cli-http-operator-parity.http.body"
cli_http_headers="$log_root/x86_64-microkernel-fat32-$cli_run_id.cli-http-operator-parity.http.headers"
cli_http_status="$log_root/x86_64-microkernel-fat32-$cli_run_id.cli-http-operator-parity.http.status"
cli_cap_body="$log_root/x86_64-microkernel-fat32-$cli_run_id.cli-http-operator-parity.cap-negative.body"
cli_cap_headers="$log_root/x86_64-microkernel-fat32-$cli_run_id.cli-http-operator-parity.cap-negative.headers"
cli_cap_status="$log_root/x86_64-microkernel-fat32-$cli_run_id.cli-http-operator-parity.cap-negative.status"
cli_network_pcap="$(capture_artifact "$cli_parity_log" "network pcap")"
cli_forbidden_guard="$log_root/x86_64-microkernel-fat32-$cli_run_id.cli-http-operator-parity.forbidden-guard.txt"
cli_serial_log="$(capture_artifact "$cli_parity_log" "serial log")"
cli_qemu_log="$(capture_artifact "$cli_parity_log" "qemu log")"

fat32_summary="$(capture_artifact "$fat32_log" "scheduler fairness summary")"
fat32_fairness_log="$(capture_artifact "$fat32_log" "fairness log")"
fat32_fairness_client_log="$(capture_artifact "$fat32_log" "fairness client log")"
fat32_fairness_curl_log="$(capture_artifact "$fat32_log" "fairness curl log")"
fat32_fairness_pcap="$(capture_artifact "$fat32_log" "fairness pcap")"
fat32_serial_log="$(capture_artifact "$fat32_log" "serial log")"
fat32_qemu_log="$(capture_artifact "$fat32_log" "qemu log")"

network_summary="$(capture_artifact "$network_log" "nontls network service summary")"
network_host_log="$(capture_artifact "$network_log" "network host log")"
network_pcap="$(capture_artifact "$network_log" "network pcap")"
network_serial_log="$(capture_artifact "$network_log" "serial log")"
network_qemu_log="$(capture_artifact "$network_log" "qemu log")"

timer_summary="$(capture_artifact "$timer_log" "timer timeout service summary")"
timer_serial_log="$(capture_artifact "$timer_log" "serial log")"
timer_qemu_log="$(capture_artifact "$timer_log" "qemu log")"
timer_client_log="$(capture_artifact "$timer_log" "client log")"
timer_network_pcap="$(capture_artifact "$timer_log" "network pcap")"

fault_summary="$(capture_artifact "$fault_log" "fault policy hardening summary")"
fault_serial_log="$(capture_artifact "$fault_log" "serial log")"
fault_pcap="$(capture_artifact "$fault_log" "network pcap")"

for path in \
  "$http_curl_log" "$http_curl_pcap" "$http_serial_log" "$http_qemu_log" \
  "$cli_summary" "$cli_cli_transcript" "$cli_http_body" "$cli_http_headers" "$cli_http_status" "$cli_cap_headers" "$cli_cap_status" "$cli_network_pcap" "$cli_forbidden_guard" "$cli_serial_log" "$cli_qemu_log" \
  "$fat32_summary" "$fat32_fairness_log" "$fat32_fairness_client_log" "$fat32_fairness_curl_log" "$fat32_fairness_pcap" "$fat32_serial_log" "$fat32_qemu_log" \
  "$network_summary" "$network_host_log" "$network_pcap" "$network_serial_log" "$network_qemu_log" \
  "$timer_summary" "$timer_serial_log" "$timer_qemu_log" "$timer_client_log" "$timer_network_pcap" \
  "$fault_summary" "$fault_serial_log" "$fault_pcap"; do
  require_nonempty_under_log_root_and_fresh "$path" "$log_root" "$start_epoch"
done

require_file_under_log_root_and_fresh "$cli_cap_body" "$log_root" "$start_epoch"

require_header_only_pcap_rejection "$http_curl_pcap"
require_header_only_pcap_rejection "$cli_network_pcap"
require_header_only_pcap_rejection "$fat32_fairness_pcap"
require_header_only_pcap_rejection "$network_pcap"
require_header_only_pcap_rejection "$timer_network_pcap"
require_header_only_pcap_rejection "$fault_pcap"

require_summary_key "$cli_summary" "parity_summary_status" "pass"
require_summary_key "$fat32_summary" "scheduler_fairness_summary_status" "pass"
require_summary_key "$network_summary" "nontls_network_service_summary_status" "pass"
require_summary_key "$timer_summary" "timer_timeout_service_summary_status" "pass"
require_summary_key "$fault_summary" "fault_policy_hardening_summary_status" "pass"

cat >"$summary" <<EOF
appliance_load_fairness_summary_status=pass
appliance_load_fairness_run_id=$run_id
appliance_load_fairness_summary=$summary
appliance_load_fairness_scenarios=http-heavy,cli-heavy,fat32-heavy,network-heavy,timer-heavy,contained-fault
appliance_load_fairness_http_heavy_summary=$http_curl_log
appliance_load_fairness_http_heavy_curl_log=$http_curl_log
appliance_load_fairness_http_heavy_curl_pcap=$http_curl_pcap
appliance_load_fairness_http_heavy_serial_log=$http_serial_log
appliance_load_fairness_http_heavy_qemu_log=$http_qemu_log
appliance_load_fairness_http_heavy_summary_status=pass
appliance_load_fairness_http_heavy_marker=x86_64 microkernel FAT32 curl proof passed.
appliance_load_fairness_cli_heavy_summary=$cli_summary
appliance_load_fairness_cli_heavy_cli_transcript=$cli_cli_transcript
appliance_load_fairness_cli_heavy_http_body=$cli_http_body
appliance_load_fairness_cli_heavy_http_headers=$cli_http_headers
appliance_load_fairness_cli_heavy_http_status=$cli_http_status
appliance_load_fairness_cli_heavy_cap_negative_body=$cli_cap_body
appliance_load_fairness_cli_heavy_cap_negative_headers=$cli_cap_headers
appliance_load_fairness_cli_heavy_cap_negative_status=$cli_cap_status
appliance_load_fairness_cli_heavy_network_pcap=$cli_network_pcap
appliance_load_fairness_cli_heavy_forbidden_guard=$cli_forbidden_guard
appliance_load_fairness_cli_heavy_serial_log=$cli_serial_log
appliance_load_fairness_cli_heavy_qemu_log=$cli_qemu_log
appliance_load_fairness_cli_heavy_summary_status=pass
appliance_load_fairness_cli_heavy_marker=x86_64 microkernel CLI/HTTP operator parity contract OK
appliance_load_fairness_fat32_heavy_summary=$fat32_summary
appliance_load_fairness_fat32_heavy_fairness_log=$fat32_fairness_log
appliance_load_fairness_fat32_heavy_fairness_client_log=$fat32_fairness_client_log
appliance_load_fairness_fat32_heavy_fairness_curl_log=$fat32_fairness_curl_log
appliance_load_fairness_fat32_heavy_fairness_pcap=$fat32_fairness_pcap
appliance_load_fairness_fat32_heavy_serial_log=$fat32_serial_log
appliance_load_fairness_fat32_heavy_qemu_log=$fat32_qemu_log
appliance_load_fairness_fat32_heavy_summary_status=pass
appliance_load_fairness_fat32_heavy_marker=x86_64 microkernel scheduler fairness load proof passed.
appliance_load_fairness_network_heavy_summary=$network_summary
appliance_load_fairness_network_heavy_host_log=$network_host_log
appliance_load_fairness_network_heavy_pcap=$network_pcap
appliance_load_fairness_network_heavy_serial_log=$network_serial_log
appliance_load_fairness_network_heavy_qemu_log=$network_qemu_log
appliance_load_fairness_network_heavy_summary_status=pass
appliance_load_fairness_network_heavy_marker=x86_64 microkernel non-TLS network service matrix passed.
appliance_load_fairness_timer_heavy_summary=$timer_summary
appliance_load_fairness_timer_heavy_serial_log=$timer_serial_log
appliance_load_fairness_timer_heavy_qemu_log=$timer_qemu_log
appliance_load_fairness_timer_heavy_client_log=$timer_client_log
appliance_load_fairness_timer_heavy_network_pcap=$timer_network_pcap
appliance_load_fairness_timer_heavy_summary_status=pass
appliance_load_fairness_timer_heavy_marker=x86_64 microkernel timer timeout service proof passed.
appliance_load_fairness_contained_fault_summary=$fault_summary
appliance_load_fairness_contained_fault_serial_log=$fault_serial_log
appliance_load_fairness_contained_fault_pcap=$fault_pcap
appliance_load_fairness_contained_fault_summary_status=pass
appliance_load_fairness_contained_fault_marker=x86_64 microkernel fault policy hardening proof passed.
EOF

require_nonempty_under_log_root_and_fresh "$summary" "$log_root" "$start_epoch"

dp_reject_duplicate_keys "$summary" || fail "pack summary has duplicate keys"

for key in \
  appliance_load_fairness_summary_status \
  appliance_load_fairness_http_heavy_summary_status \
  appliance_load_fairness_cli_heavy_summary_status \
  appliance_load_fairness_fat32_heavy_summary_status \
  appliance_load_fairness_network_heavy_summary_status \
  appliance_load_fairness_timer_heavy_summary_status \
  appliance_load_fairness_contained_fault_summary_status; do
  require_summary_key "$summary" "$key" "pass"
done

dp_emit_artifact_line "appliance load fairness pack summary" "$summary"
dp_emit_artifact_line "http-heavy summary" "$http_curl_log"
dp_emit_artifact_line "cli-heavy summary" "$cli_summary"
dp_emit_artifact_line "fat32-heavy summary" "$fat32_summary"
dp_emit_artifact_line "network-heavy summary" "$network_summary"
dp_emit_artifact_line "timer-heavy summary" "$timer_summary"
dp_emit_artifact_line "contained-fault summary" "$fault_summary"

echo "appliance load fairness pack summary: $summary"
echo "x86_64 microkernel appliance load fairness pack passed."
