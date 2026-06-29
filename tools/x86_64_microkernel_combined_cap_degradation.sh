#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mnt_root="/home/user/mnt/dataplane"
log_dir="$mnt_root/logs"
tmp_dir="$mnt_root/tmp"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
summary="$log_dir/x86_64-microkernel-combined-cap-degradation-$run_id.summary"
timer_run_log="$log_dir/x86_64-microkernel-combined-cap-degradation-$run_id.timer-timeout.run.log"
timer_summary_log="$log_dir/x86_64-microkernel-combined-cap-degradation-$run_id.timer-timeout.summary.log"
runtime_log="$log_dir/x86_64-microkernel-combined-cap-degradation-$run_id.runtime.run.log"
negative_log="$log_dir/x86_64-microkernel-combined-cap-degradation-$run_id.negative-matrix.run.log"

mkdir -p "$log_dir" "$tmp_dir"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

extract_label_path() {
  local log_file="$1"
  local label="$2"
  local count value
  count="$(awk -F': ' -v key="$label" '$1 == key { count++ } END { print count + 0 }' "$log_file")"
  [[ "$count" -eq 1 ]] || fail "artifact label '$label' count in $log_file was $count"
  value="$(awk -F': ' -v key="$label" '$1 == key { print $2 }' "$log_file")"
  [[ -n "$value" ]] || fail "empty artifact label '$label'"
  [[ "$value" == "$log_dir"/* ]] || fail "artifact outside $log_dir: $value"
  [[ -s "$value" ]] || fail "missing or empty artifact: $value"
  printf '%s\n' "$value"
}

require_key_value() {
  local file="$1"
  local key="$2"
  local expected="$3"
  local count line
  count="$(awk -F= -v key="$key" '$1 == key { count++ } END { print count + 0 }' "$file")"
  [[ "$count" -eq 1 ]] || fail "summary key '$key' count in $file was $count"
  line="$(awk -F= -v key="$key" '$1 == key { print $0 }' "$file")"
  [[ "$line" == "$key=$expected" ]] || fail "expected $key=$expected in $file, got '${line:-missing}'"
}

reject_duplicate_keys() {
  local file="$1"
  local duplicate
  duplicate="$(awk -F= '/^[A-Za-z0-9_]+=/ { print $1 }' "$file" | sort | uniq -d)"
  [[ -z "$duplicate" ]] || fail "duplicate summary key(s) in $file: $duplicate"
}

TMPDIR="$tmp_dir" bash ./tools/x86_64_microkernel_timer_timeout_service.sh >"$timer_run_log" 2>&1
timer_summary="$(extract_label_path "$timer_run_log" "timer timeout service summary")"
timer_summary_status="$(awk -F= '$1 == "timer_timeout_service_summary_status" { print $2; exit }' "$timer_summary")"
[[ "$timer_summary_status" == "pass" ]] || fail "timer timeout summary status was $timer_summary_status"
timer_summary_basename="$(basename "$timer_summary")"
timer_summary_run_id="${timer_summary_basename#x86_64-microkernel-fat32-}"
timer_summary_run_id="${timer_summary_run_id%.timer-timeout-service.summary}"
[[ -n "$timer_summary_run_id" ]] || fail "timer timeout summary missing source stamp"
cp "$timer_summary" "$timer_summary_log"

timer_serial_log="$(extract_label_path "$timer_run_log" "serial log")"
timer_pcap="$(extract_label_path "$timer_run_log" "network pcap")"
timer_pcap_bytes="$(wc -c <"$timer_pcap" | tr -d ' ')"
(( timer_pcap_bytes > 24 )) || fail "timer pcap only contains a global header"

timer_gap="$(awk -F= '$1 == "timer_max_observed_gap_ticks" { print $2; exit }' "$timer_summary")"
[[ "$timer_gap" == "8" ]] || fail "could not extract timer proof max gap"
timer_gap_fairness="$(awk -F= '$1 == "timer_gap_fairness" { print $2; exit }' "$timer_summary")"
[[ "$timer_gap_fairness" == "8" ]] || fail "timer proof gap fairness was $timer_gap_fairness"
grep -Fq "DPTIMER:gap-bounds observed=8 fairness=8 network=32 fairness_poll_interval=8 dhcp_poll_interval=16" "$timer_serial_log" || fail "timer proof missing visible progress marker"

TMPDIR="$tmp_dir" ./tools/x86_64_microkernel_fat32_run.sh >"$runtime_log" 2>&1
serial_log="$(extract_label_path "$runtime_log" "serial log")"
client_log="$(extract_label_path "$runtime_log" "client log")"
qemu_log="$(extract_label_path "$runtime_log" "qemu log")"
pcap="$(extract_label_path "$runtime_log" "network pcap")"
pcap_bytes="$(wc -c <"$pcap" | tr -d ' ')"
(( pcap_bytes > 24 )) || fail "runtime pcap only contains a global header"

for marker in \
  "DPMK:COMBINED-CAP-DEGRADATION-LEDGER" \
  "DPCAP:output inline_limit=32 oversized_rejected=1" \
  "DPCAP:queue mailbox_cap=1 queue_full_rejected=1" \
  "DPCAP:timer progress_source=cooperative max_gap_bound=32" \
  "DPCAP:stale-reply runtime_claim=0 policy=deferred" \
  "DPMK:COMBINED-CAP-DEGRADATION-OK" \
  "DPMK:FAULT-CONTAINED"; do
  grep -Fq "$marker" "$serial_log" || fail "runtime serial log missing marker: $marker"
done

TMPDIR="$tmp_dir" ./tools/x86_64_microkernel_qemu_negative_matrix.sh >"$negative_log" 2>&1
negative_summary="$(extract_label_path "$negative_log" "summary")"
reject_duplicate_keys "$negative_summary"
require_key_value "$negative_summary" "qemu_negative_matrix_summary_status" "pass"
require_key_value "$negative_summary" "qemu_negative_matrix_missing_pcap_rejected" "true"
require_key_value "$negative_summary" "qemu_negative_matrix_stale_summary_rejected" "true"
require_key_value "$negative_summary" "qemu_negative_matrix_wrong_marker_rejected" "true"
require_key_value "$negative_summary" "qemu_negative_matrix_timeout_or_truncated_capture_rejected" "true"

combined_gap="$(awk -F'max_gap_bound=' '/DPCAP:timer progress_source=cooperative max_gap_bound=/ { print $2; exit }' "$serial_log")"
[[ "$combined_gap" == "32" ]] || fail "could not extract combined-cap timer max gap"

{
  echo "combined_cap_degradation_summary_status=pass"
  echo "combined_cap_degradation_run_id=$run_id"
  echo "combined_cap_degradation_mode=runtime-ledger-plus-runner-negatives"
  echo "combined_cap_degradation_timer_run_log=$timer_run_log"
  echo "combined_cap_degradation_timer_summary_log=$timer_summary_log"
  echo "combined_cap_degradation_timer_summary=$timer_summary"
  echo "combined_cap_degradation_timer_summary_run_id=$timer_summary_run_id"
  echo "combined_cap_degradation_timer_gap=$timer_gap"
  echo "combined_cap_degradation_timer_gap_fairness=$timer_gap_fairness"
  echo "combined_cap_degradation_runtime_log=$runtime_log"
  echo "combined_cap_degradation_serial_log=$serial_log"
  echo "combined_cap_degradation_client_log=$client_log"
  echo "combined_cap_degradation_qemu_log=$qemu_log"
  echo "combined_cap_degradation_pcap=$pcap"
  echo "combined_cap_degradation_pcap_bytes=$pcap_bytes"
  echo "combined_cap_degradation_timer_serial_log=$timer_serial_log"
  echo "combined_cap_degradation_timer_pcap=$timer_pcap"
  echo "combined_cap_degradation_timer_pcap_bytes=$timer_pcap_bytes"
  echo "combined_cap_degradation_negative_matrix_log=$negative_log"
  echo "combined_cap_degradation_negative_matrix_summary=$negative_summary"
  echo "combined_cap_degradation_runtime_ledger_ok=true"
  echo "combined_cap_degradation_timer_timeout_summary_ok=true"
  echo "combined_cap_degradation_output_cap_rejected=true"
  echo "combined_cap_degradation_queue_full_or_timeout_visible=true"
  echo "combined_cap_degradation_queue_full_rejected=true"
  echo "combined_cap_degradation_active_fault_visible=true"
  echo "combined_cap_degradation_missing_artifact_fails_closed=true"
  echo "combined_cap_degradation_stale_summary_fails_closed=true"
  echo "combined_cap_degradation_stale_reply_fails_closed=true"
  echo "combined_cap_degradation_wrong_marker_fails_closed=true"
  echo "combined_cap_degradation_timer_progress_visible=true"
  echo "combined_cap_degradation_timer_proof_visible=true"
  echo "combined_cap_degradation_timer_max_gap=$combined_gap"
  echo "combined_cap_degradation_stale_reply_runtime_claim=false"
  echo "combined_cap_degradation_single_runtime_claim=false"
  echo "combined_cap_degradation_recovery_framework=false"
  echo "combined_cap_degradation_restart=false"
  echo "combined_cap_degradation_reset=false"
  echo "combined_cap_degradation_tls=false"
  echo "combined_cap_degradation_https=false"
  echo "combined_cap_degradation_benchmark_result=false"
} >"$summary"

reject_duplicate_keys "$summary"

echo "x86_64 microkernel combined cap degradation proof passed."
echo "summary: $summary"
