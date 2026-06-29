#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

wrapper="tools/x86_64_microkernel_readonly_appliance_load_fairness_release.sh"
runner="tools/x86_64_microkernel_fat32_run.sh"
release="tools/x86_64_microkernel_readonly_appliance_release.sh"
scheduler_guard="tools/check_x86_64_microkernel_scheduler_fairness_load_contract.sh"
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

reject_packet_regex() {
  local file="$1"
  local regex="$2"
  local note="$3"
  local out="/tmp/dataplane-readonly-load-fairness.$$"
  local err="/tmp/dataplane-readonly-load-fairness-err.$$"
  local status
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

for file in "$wrapper" "$runner" "$release" "$scheduler_guard" "$makefile" "$plan" "$diary"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel Readonly Appliance Load Fairness Release Contract ==="

for literal in \
  'mnt_root="/home/user/mnt/dataplane"' \
  'fresh_marker="$log_dir/x86_64-microkernel-readonly-load-fairness-$run_id.fresh"' \
  'tools/x86_64_microkernel_readonly_appliance_release.sh >"$release_log"' \
  'DP_MICROKERNEL_SCHEDULER_FAIRNESS_LOAD_PROOF=1 \' \
  'tools/x86_64_microkernel_fat32_run.sh --scheduler-fairness-load-proof >"$fairness_log"' \
  'release_summary="$(extract_artifact "$release_log" "summary")"' \
  'fairness_summary="$(extract_artifact "$fairness_log" "scheduler fairness summary")"' \
  'require_summary_key "$release_summary" "readonly_appliance_release_summary_status" "pass"' \
  'require_summary_key "$fairness_summary" "scheduler_fairness_summary_status" "pass"' \
  'require_contains "$release_serial" "DPMK:SERVICE-LIFECYCLE-LEDGER-OK"' \
  'require_contains "$release_serial" "DPMK:SERVICE-LIFECYCLE-FAULT-OK"' \
  'require_contains "$fairness_serial" "DPMK:FAIR-TIMER-MAXGAP:"' \
  'readonly_load_fairness_release_summary_status=pass' \
  'readonly_load_fairness_release_readonly_release_ok=true' \
  'readonly_load_fairness_release_scheduler_fairness_ok=true' \
  'readonly_load_fairness_release_lifecycle_ok=true' \
  'readonly_load_fairness_release_timer_bound_ok=true' \
  'readonly_load_fairness_release_cli_ok=true' \
  'readonly_load_fairness_release_http_ok=true' \
  'readonly_load_fairness_release_fat32_ok=true' \
  'readonly_load_fairness_release_fault_contained_ok=true' \
  'readonly_load_fairness_release_network_pcap_ok=true' \
  'readonly_load_fairness_release_default_writable=false' \
  'readonly_load_fairness_release_tls=false' \
  'readonly_load_fairness_release_write_feature_used=false' \
  'readonly_load_fairness_release_restart=false' \
  'readonly_load_fairness_release_replay=false' \
  'readonly_load_fairness_release_benchmark_result=false' \
  'readonly_load_fairness_release_strict_five_substitution=false'; do
  require_literal "$wrapper" "$literal" "wrapper must preserve literal: $literal"
done

for literal in \
  'DP_MICROKERNEL_SCHEDULER_FAIRNESS_LOAD_PROOF' \
  'DP_SCHEDULER_FAIRNESS_LOAD_PROOF' \
  '--scheduler-fairness-load-proof' \
  'scheduler_fairness_summary="$log_dir/x86_64-microkernel-fat32-$run_id.scheduler-fairness-load.summary"' \
  'scheduler_fairness_summary_status=pass'; do
  require_literal "$runner" "$literal" "runner must preserve scheduler fairness literal: $literal"
done

require_literal "$makefile" 'x86_64-microkernel-readonly-appliance-load-fairness-release-contract:' \
  "Makefile must expose read-only load fairness release contract"
require_literal "$makefile" './tools/check_x86_64_microkernel_readonly_appliance_load_fairness_release_contract.sh' \
  "Makefile must run read-only load fairness release guard"
require_literal "$makefile" 'x86_64-microkernel-readonly-appliance-load-fairness-release:' \
  "Makefile must expose read-only load fairness release target"
require_literal "$makefile" './tools/x86_64_microkernel_readonly_appliance_load_fairness_release.sh' \
  "Makefile must run read-only load fairness release wrapper"
require_literal "$plan" 'x86_64-microkernel-readonly-appliance-load-fairness-release' \
  "plan must record read-only load fairness release packet"
require_literal "$diary" 'x86_64-microkernel-readonly-appliance-load-fairness-release' \
  "diary must record read-only load fairness release packet"

reject_packet_regex "$wrapper" 'readonly_load_fairness_release_.*(tls|https|security).*=(true|1)' \
  "packet must not claim TLS, HTTPS, or security"
reject_packet_regex "$wrapper" 'readonly_load_fairness_release_.*(default_writable|write_feature_used|restart|replay|benchmark_result|strict_five_substitution).*=(true|1)' \
  "packet must not claim writable storage, restart, replay, or benchmark evidence"
reject_packet_regex "$wrapper" 'curl[[:space:]]+-k|openssl|rustls|embedded-tls|webpki|ring::|aws-lc-rs|STRICT_FIVE_CALIBRATION|retune[[:space:]]|calibration' \
  "packet must not introduce TLS tooling or benchmark retuning"

echo "x86_64 microkernel read-only appliance load fairness release contract OK"
