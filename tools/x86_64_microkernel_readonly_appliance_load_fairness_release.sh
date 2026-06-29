#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mnt_root="/home/user/mnt/dataplane"
log_dir="$mnt_root/logs"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
fresh_marker="$log_dir/x86_64-microkernel-readonly-load-fairness-$run_id.fresh"
release_log="$log_dir/x86_64-microkernel-readonly-load-fairness-$run_id.release.log"
fairness_log="$log_dir/x86_64-microkernel-readonly-load-fairness-$run_id.fairness.log"
summary="$log_dir/x86_64-microkernel-readonly-load-fairness-$run_id.summary"

mkdir -p "$log_dir"
: >"$fresh_marker"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

extract_artifact() {
  local log="$1"
  local label="$2"
  local count value
  count="$(awk -F': ' -v key="$label" '$1 == key { count++ } END { print count + 0 }' "$log")"
  [[ "$count" -eq 1 ]] || fail "artifact label '$label' count in $log was $count"
  value="$(awk -F': ' -v key="$label" '$1 == key { print $2 }' "$log")"
  [[ -n "$value" ]] || fail "empty artifact label '$label' in $log"
  printf '%s\n' "$value"
}

require_file() {
  local path="$1"
  [[ -s "$path" ]] || fail "missing or empty artifact: $path"
  [[ "$path" == "$log_dir"/* ]] || fail "artifact outside $log_dir: $path"
  [[ "$path" -nt "$fresh_marker" ]] || fail "stale artifact: $path"
}

require_contains() {
  local path="$1"
  local literal="$2"
  local note="$3"
  grep -Fq -- "$literal" "$path" || fail "$note"
}

require_summary_key() {
  local path="$1"
  local key="$2"
  local expected="$3"
  local count
  count="$(awk -F= -v key="$key" '$1 == key { count++ } END { print count + 0 }' "$path")"
  [[ "$count" -eq 1 ]] || fail "summary key '$key' count in $path was $count"
  require_contains "$path" "$key=$expected" "summary $path missing $key=$expected"
}

summary_value() {
  local path="$1"
  local key="$2"
  awk -F= -v key="$key" '$1 == key { print substr($0, length(key) + 2) }' "$path"
}

sha256_file() {
  sha256sum "$1" | awk '{print $1}'
}

tools/x86_64_microkernel_readonly_appliance_release.sh >"$release_log"
release_summary="$(extract_artifact "$release_log" "summary")"
require_file "$release_summary"

DP_MICROKERNEL_SCHEDULER_FAIRNESS_LOAD_PROOF=1 \
  tools/x86_64_microkernel_fat32_run.sh --scheduler-fairness-load-proof >"$fairness_log"
fairness_summary="$(extract_artifact "$fairness_log" "scheduler fairness summary")"
require_file "$fairness_summary"

require_summary_key "$release_summary" "readonly_appliance_release_summary_status" "pass"
require_summary_key "$release_summary" "readonly_appliance_release_default_writable" "false"
require_summary_key "$release_summary" "readonly_appliance_release_tls" "false"
require_summary_key "$release_summary" "readonly_appliance_release_benchmark_result" "false"
require_summary_key "$release_summary" "readonly_appliance_release_fault_containment_ok" "true"

require_summary_key "$fairness_summary" "scheduler_fairness_summary_status" "pass"
require_summary_key "$fairness_summary" "scheduler_fairness_load_timer_bound_ok" "true"
require_summary_key "$fairness_summary" "scheduler_fairness_load_cli_ok" "true"
require_summary_key "$fairness_summary" "scheduler_fairness_load_http_ok" "true"
require_summary_key "$fairness_summary" "scheduler_fairness_load_fs_ok" "true"
require_summary_key "$fairness_summary" "scheduler_fairness_load_fault_contained" "true"
require_summary_key "$fairness_summary" "scheduler_fairness_load_tls_deferred" "true"
require_summary_key "$fairness_summary" "scheduler_fairness_load_no_benchmark_retune_ok" "true"

release_serial="$(summary_value "$release_summary" "readonly_appliance_release_serial_log")"
release_pcap="$(summary_value "$release_summary" "readonly_appliance_release_network_pcap")"
fairness_serial="$(summary_value "$fairness_summary" "scheduler_fairness_load_serial_log")"
fairness_pcap="$(summary_value "$fairness_summary" "scheduler_fairness_load_pcap")"
fairness_timer_max_gap="$(summary_value "$fairness_summary" "scheduler_fairness_load_timer_max_gap")"
fairness_curl_completed="$(summary_value "$fairness_summary" "scheduler_fairness_load_curl_completed")"

for artifact in "$release_serial" "$release_pcap" "$fairness_serial" "$fairness_pcap"; do
  require_file "$artifact"
done

require_contains "$release_serial" "DPMK:SERVICE-LIFECYCLE-LEDGER-OK" \
  "release serial log missing lifecycle ledger"
require_contains "$release_serial" "DPMK:SERVICE-LIFECYCLE-FAULT-OK" \
  "release serial log missing post-fault lifecycle evidence"
require_contains "$fairness_serial" "DPMK:FAIR-TIMER-MAXGAP:" \
  "fairness serial log missing timer max-gap marker"
require_contains "$fairness_serial" "DPMK:FAIR-OK" \
  "fairness serial log missing FAIR-OK marker"

release_pcap_bytes="$(wc -c <"$release_pcap" | tr -d ' ')"
fairness_pcap_bytes="$(wc -c <"$fairness_pcap" | tr -d ' ')"
(( release_pcap_bytes > 24 )) || fail "release pcap only contains global header"
(( fairness_pcap_bytes > 24 )) || fail "fairness pcap only contains global header"

{
  echo "readonly_load_fairness_release_summary_status=pass"
  echo "readonly_load_fairness_release_run_id=$run_id"
  echo "readonly_load_fairness_release_readonly_release_ok=true"
  echo "readonly_load_fairness_release_scheduler_fairness_ok=true"
  echo "readonly_load_fairness_release_lifecycle_ok=true"
  echo "readonly_load_fairness_release_timer_bound_ok=true"
  echo "readonly_load_fairness_release_cli_ok=true"
  echo "readonly_load_fairness_release_http_ok=true"
  echo "readonly_load_fairness_release_fat32_ok=true"
  echo "readonly_load_fairness_release_fault_contained_ok=true"
  echo "readonly_load_fairness_release_network_pcap_ok=true"
  echo "readonly_load_fairness_release_default_writable=false"
  echo "readonly_load_fairness_release_tls=false"
  echo "readonly_load_fairness_release_write_feature_used=false"
  echo "readonly_load_fairness_release_restart=false"
  echo "readonly_load_fairness_release_replay=false"
  echo "readonly_load_fairness_release_benchmark_result=false"
  echo "readonly_load_fairness_release_strict_five_substitution=false"
  echo "readonly_load_fairness_release_timer_max_gap=$fairness_timer_max_gap"
  echo "readonly_load_fairness_release_fairness_curl_completed=$fairness_curl_completed"
  echo "readonly_load_fairness_release_release_summary=$release_summary"
  echo "readonly_load_fairness_release_fairness_summary=$fairness_summary"
  echo "readonly_load_fairness_release_release_serial_log=$release_serial"
  echo "readonly_load_fairness_release_fairness_serial_log=$fairness_serial"
  echo "readonly_load_fairness_release_release_pcap=$release_pcap"
  echo "readonly_load_fairness_release_fairness_pcap=$fairness_pcap"
  echo "readonly_load_fairness_release_release_pcap_bytes=$release_pcap_bytes"
  echo "readonly_load_fairness_release_fairness_pcap_bytes=$fairness_pcap_bytes"
  echo "readonly_load_fairness_release_release_pcap_sha256=$(sha256_file "$release_pcap")"
  echo "readonly_load_fairness_release_fairness_pcap_sha256=$(sha256_file "$fairness_pcap")"
} >"$summary"

require_summary_key "$summary" "readonly_load_fairness_release_summary_status" "pass"
require_summary_key "$summary" "readonly_load_fairness_release_default_writable" "false"
require_summary_key "$summary" "readonly_load_fairness_release_tls" "false"
require_summary_key "$summary" "readonly_load_fairness_release_benchmark_result" "false"

echo "x86_64 microkernel read-only appliance load fairness release passed."
echo "summary: $summary"
