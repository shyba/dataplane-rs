#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

runner="tools/x86_64_microkernel_combined_cap_degradation.sh"
kernel="crates/dataplane-x86_64-microkernel-smoke/src/kernel.rs"
kernel_ledgers="crates/dataplane-x86_64-microkernel-smoke/src/kernel_ledgers.rs"
makefile="Makefile"
plan="aidocs/055_microkernel_tls_deferred_robust_appliance_plan_2026-05-31.md"
diary="aidocs/050_microkernel_robust_design_diary_2026-05-30.md"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_file() {
  [[ -f "$1" ]] || fail "missing required file: $1"
}

require_literal() {
  local file="$1"
  local literal="$2"
  local note="$3"
  grep -Fq -- "$literal" "$file" || fail "$note"
}

reject_regex() {
  local file="$1"
  local regex="$2"
  local note="$3"
  local out="/home/user/mnt/dataplane/tmp/combined-degradation-guard.$$"
  local err="/home/user/mnt/dataplane/tmp/combined-degradation-guard-err.$$"
  local status
  mkdir -p /home/user/mnt/dataplane/tmp
  set +e
  rg -n -- "$regex" "$file" >"$out" 2>"$err"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    cat "$out"
    rm -f "$out" "$err"
    fail "$note"
  fi
  if [[ "$status" -ne 1 ]]; then
    cat "$err" >&2 || true
    rm -f "$out" "$err"
    fail "could not scan $file for forbidden regex: $regex"
  fi
  rm -f "$out" "$err"
}

for file in "$runner" "$kernel" "$kernel_ledgers" "$makefile" "$plan" "$diary"; do
  require_file "$file"
done

for literal in \
  'mnt_root="/home/user/mnt/dataplane"' \
  'timer_run_log="$log_dir/x86_64-microkernel-combined-cap-degradation-$run_id.timer-timeout.run.log"' \
  'timer_summary_log="$log_dir/x86_64-microkernel-combined-cap-degradation-$run_id.timer-timeout.summary.log"' \
  'TMPDIR="$tmp_dir" bash ./tools/x86_64_microkernel_timer_timeout_service.sh >"$timer_run_log" 2>&1' \
  'timer_summary="$(extract_label_path "$timer_run_log" "timer timeout service summary")"' \
  'timer_summary_status="$(awk -F= '\''$1 == "timer_timeout_service_summary_status" { print $2; exit }'\'' "$timer_summary")"' \
  'timer_gap="$(awk -F= '\''$1 == "timer_max_observed_gap_ticks" { print $2; exit }'\'' "$timer_summary")"' \
  'grep -Fq "DPTIMER:gap-bounds observed=8 fairness=8 network=32 fairness_poll_interval=8 dhcp_poll_interval=16" "$timer_serial_log"' \
  './tools/x86_64_microkernel_fat32_run.sh >"$runtime_log" 2>&1' \
  './tools/x86_64_microkernel_qemu_negative_matrix.sh >"$negative_log" 2>&1' \
  'DPMK:COMBINED-CAP-DEGRADATION-LEDGER' \
  'DPCAP:output inline_limit=32 oversized_rejected=1' \
  'DPCAP:queue mailbox_cap=1 queue_full_rejected=1' \
  'DPCAP:timer progress_source=cooperative max_gap_bound=32' \
  'DPCAP:stale-reply runtime_claim=0 policy=deferred' \
  'DPMK:COMBINED-CAP-DEGRADATION-OK' \
  'DPMK:FAULT-CONTAINED' \
  'combined_cap_degradation_timer_timeout_summary_ok=true' \
  'combined_cap_degradation_timer_proof_visible=true' \
  'combined_cap_degradation_timer_summary_run_id=' \
  'combined_cap_degradation_timer_gap=' \
  'combined_cap_degradation_timer_gap_fairness=' \
  'qemu_negative_matrix_missing_pcap_rejected' \
  'qemu_negative_matrix_stale_summary_rejected' \
  'qemu_negative_matrix_wrong_marker_rejected' \
  'qemu_negative_matrix_timeout_or_truncated_capture_rejected' \
  'combined_cap_degradation_summary_status=pass' \
  'combined_cap_degradation_mode=runtime-ledger-plus-runner-negatives' \
  'combined_cap_degradation_timer_run_log=' \
  'combined_cap_degradation_timer_summary_log=' \
  'combined_cap_degradation_timer_summary=' \
  'combined_cap_degradation_runtime_ledger_ok=true' \
  'combined_cap_degradation_timer_timeout_summary_ok=true' \
  'combined_cap_degradation_output_cap_rejected=true' \
  'combined_cap_degradation_queue_full_or_timeout_visible=true' \
  'combined_cap_degradation_queue_full_rejected=true' \
  'combined_cap_degradation_active_fault_visible=true' \
  'combined_cap_degradation_missing_artifact_fails_closed=true' \
  'combined_cap_degradation_stale_summary_fails_closed=true' \
  'combined_cap_degradation_stale_reply_fails_closed=true' \
  'combined_cap_degradation_wrong_marker_fails_closed=true' \
  'combined_cap_degradation_timer_progress_visible=true' \
  'combined_cap_degradation_stale_reply_runtime_claim=false' \
  'combined_cap_degradation_single_runtime_claim=false' \
  'combined_cap_degradation_recovery_framework=false' \
  'combined_cap_degradation_restart=false' \
  'combined_cap_degradation_reset=false' \
  'combined_cap_degradation_tls=false' \
  'combined_cap_degradation_https=false' \
  'combined_cap_degradation_benchmark_result=false'; do
  require_literal "$runner" "$literal" "combined degradation runner must preserve literal: $literal"
done

for literal in \
  'emit_combined_cap_degradation_ledger();'; do
  require_literal "$kernel" "$literal" \
    "guest orchestration must call combined degradation runtime literal: $literal"
done

for literal in \
  'fn emit_combined_cap_degradation_ledger()' \
  'MessageBody::inline_bytes(&oversize).is_err()' \
  'mailbox.send(message).is_ok() && mailbox.send(message).is_err()' \
  'DPMK:COMBINED-CAP-DEGRADATION-LEDGER' \
  'DPCAP:stale-reply runtime_claim=0 policy=deferred' \
  'DPMK:COMBINED-CAP-DEGRADATION-OK'; do
  require_literal "$kernel_ledgers" "$literal" \
    "guest must preserve combined degradation runtime literal: $literal"
done

require_literal "$makefile" 'x86_64-microkernel-combined-cap-degradation-contract:' \
  "Makefile must expose combined cap degradation contract"
require_literal "$makefile" './tools/check_x86_64_microkernel_combined_cap_degradation_contract.sh' \
  "Makefile must run combined cap degradation guard"
require_literal "$makefile" 'x86_64-microkernel-combined-cap-degradation:' \
  "Makefile must expose combined cap degradation proof"
require_literal "$makefile" './tools/x86_64_microkernel_combined_cap_degradation.sh' \
  "Makefile must run combined cap degradation wrapper"
require_literal "$plan" 'x86_64-microkernel-combined-cap-degradation' \
  "plan must name combined cap degradation packet"
require_literal "$diary" 'x86_64-microkernel-combined-cap-degradation' \
  "diary must record combined cap degradation packet"

reject_regex "$runner" 'combined_cap_degradation_.*(single_runtime_claim=true|recovery_framework=true|restart=true|reset=true|tls=true|https=true|benchmark_result=true)' \
  "combined degradation summary must not overclaim runtime, recovery, restart, TLS, HTTPS, or benchmark evidence"
reject_regex "$runner" 'combined_cap_degradation_.*(timer_timeout_summary_ok=false|timer_proof_visible=false|stale_reply_fails_closed=false|missing_artifact_fails_closed=false|stale_summary_fails_closed=false|wrong_marker_fails_closed=false)' \
  "combined degradation summary must keep all fail-closed evidence true"
reject_regex "$makefile" 'x86_64-microkernel-combined-cap-degradation.*(strict-five|STRICT_FIVE_CALIBRATION|retune|calibration)' \
  "combined degradation target must not substitute benchmark calibration evidence"

echo "x86_64 microkernel combined cap degradation contract OK"
