#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

kernel="crates/dataplane-x86_64-microkernel-smoke/src/kernel.rs"
kernel_ledgers="crates/dataplane-x86_64-microkernel-smoke/src/kernel_ledgers.rs"
timer_model="crates/dataplane-x86_64-microkernel-smoke/src/timer_model.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
wrapper="tools/x86_64_microkernel_timer_timeout_service.sh"
makefile="Makefile"
spec="changes/__archived_changes_2026-05-31/x86_64-microkernel-timer-timeout-service-contract/specs/timer-timeout-service-contract/spec.md"
tasks="changes/__archived_changes_2026-05-31/x86_64-microkernel-timer-timeout-service-contract/tasks.md"
proposal="changes/__archived_changes_2026-05-31/x86_64-microkernel-timer-timeout-service-contract/proposal.md"
design="changes/__archived_changes_2026-05-31/x86_64-microkernel-timer-timeout-service-contract/design.md"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_file() {
  local path="$1"
  [[ -f "$path" ]] || fail "missing required file: $path"
}

require_literal() {
  local path="$1"
  local literal="$2"
  local note="$3"
  rg -Fq -- "$literal" "$path" || fail "$note"
}

reject_regex() {
  local path="$1"
  local regex="$2"
  local note="$3"
  local tmp status
  tmp="$(mktemp)"
  set +e
  rg -n -- "$regex" "$path" >"$tmp" 2>&1
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    cat "$tmp" >&2
    rm -f "$tmp"
    fail "$note"
  fi
  if [[ "$status" -ne 1 ]]; then
    cat "$tmp" >&2
    rm -f "$tmp"
    fail "could not scan $path for forbidden regex: $regex"
  fi
  rm -f "$tmp"
}

echo "=== x86_64 Microkernel Timer Timeout Service Contract Guard ==="

for path in "$kernel" "$kernel_ledgers" "$timer_model" "$runner" "$wrapper" "$makefile" "$spec" "$tasks" "$proposal" "$design"; do
  require_file "$path"
done

awk 'BEGIN{ok=1} /^### Requirement:/{seen=1; next} seen && NF{ if ($0 !~ /SHALL|MUST/) { print "FAIL: requirement body missing SHALL/MUST -> " $0; ok=0 } seen=0 } END{ if (!seen && ok) print "Requirement syntax check passed."; exit(ok?0:1)}' "$spec" >/dev/null || \
  fail "spec requirement syntax check failed"

require_literal "$spec" "Expose the timer service owner and tick source" \
  "spec must cover timer service owner/tick source"
require_literal "$spec" "Bound timeout resolution and wrap policy" \
  "spec must cover timeout resolution and wrap policy"
require_literal "$spec" "Report the maximum observed gap" \
  "spec must cover max observed gap"
require_literal "$spec" "Fail closed on stale and expired timeout behavior" \
  "spec must cover stale/expired handling"

require_literal "$proposal" "bounded timer/timeout service contract" \
  "proposal must describe the bounded timer/timeout packet"
require_literal "$proposal" "stale timeout" \
  "proposal must keep stale timeout handling explicit"
require_literal "$proposal" "expired timeout" \
  "proposal must keep expired timeout handling explicit"
require_literal "$design" "Fail closed on stale and expired timeout behavior" \
  "design must retain the stale/expired requirement heading"

require_literal "$timer_model" "struct TimerTimeoutCase" \
  "guest ledger must define a real timeout case shape"
require_literal "$timer_model" "struct TimerTimeoutProof" \
  "guest ledger must define a real timeout proof shape"
require_literal "$timer_model" "fn evaluate_timer_timeout_proof() -> TimerTimeoutProof" \
  "guest ledger must evaluate a real timeout proof"
require_literal "$timer_model" "fn age(self) -> u32" \
  "guest ledger must compute timeout age from state"
require_literal "$timer_model" "fn is_expired(self) -> bool" \
  "guest ledger must compare deadline and grace"
require_literal "$timer_model" "TimerTimeoutProof {" \
  "guest ledger must construct proof state"
require_literal "$kernel_ledgers" "DPTIMER:positive-delivery now=" \
  "guest ledger must emit computed positive delivery evidence"
require_literal "$kernel_ledgers" "DPTIMER:stale-timeout now=" \
  "guest ledger must emit computed stale timeout evidence"
require_literal "$kernel_ledgers" "DPTIMER:expired-timeout now=" \
  "guest ledger must emit computed expired timeout evidence"
require_literal "$kernel_ledgers" "DPTIMER:wrap-comparison now=" \
  "guest ledger must emit computed wrap comparison evidence"
require_literal "$kernel_ledgers" "DPTIMER:gap-bounds observed=" \
  "guest ledger must emit observed gap evidence"
require_literal "$kernel_ledgers" "DPTIMER:delivery control=bounded owner=task=" \
  "guest ledger must expose bounded delivery control and owner"
require_literal "$kernel_ledgers" "proof.stale_case.is_expired()" \
  "guest ledger must compute stale behavior from proof state"
require_literal "$kernel_ledgers" "proof.expired_case.is_expired()" \
  "guest ledger must compute expired behavior from proof state"
require_literal "$kernel_ledgers" "proof.observed_gap_ticks" \
  "guest ledger must compute the observed gap from proof state"
require_literal "$kernel_ledgers" "timeout_resolution_ticks=8" \
  "guest ledger must expose timeout resolution"
require_literal "$kernel_ledgers" "bounded-drop" \
  "guest ledger must expose bounded stale behavior"
require_literal "$kernel_ledgers" "reject" \
  "guest ledger must expose expired rejection behavior"
require_literal "$kernel_ledgers" "observed=" \
  "guest ledger must expose observed max gap"
require_literal "$kernel_ledgers" "DPTIMER:source cooperative-run_timer_tick monotonic=work-units wall_clock=0 rtc=0 ntp=0 cert_time=0" \
  "guest ledger must keep source-real tick source evidence"

require_literal "$wrapper" "DP_MICROKERNEL_TIMER_TIMEOUT_SERVICE_PROOF=1" \
  "wrapper must select the timer timeout proof mode"
require_literal "$wrapper" "--timer-timeout-service-proof" \
  "wrapper must invoke the focused proof mode"

require_literal "$runner" 'timer_timeout_service_summary="$log_root/x86_64-microkernel-fat32-$run_id.timer-timeout-service.summary"' \
  "runner must write the timer summary under /home/user/mnt/dataplane/logs"
require_literal "$runner" "DP_MICROKERNEL_TIMER_TIMEOUT_SERVICE_PROOF" \
  "runner must accept the timer timeout env override"
require_literal "$runner" "--timer-timeout-service-proof" \
  "runner must expose the timer timeout proof mode"
require_literal "$runner" "timer_timeout_service_summary_status=pass" \
  "runner summary must record pass status"
require_literal "$runner" "timer_service_owner=" \
  "runner summary must record timer service owner"
require_literal "$runner" "timer_tick_source=" \
  "runner summary must record timer tick source"
require_literal "$runner" "timer_timeout_resolution_ticks=" \
  "runner summary must record timeout resolution"
require_literal "$runner" "timer_wrap_policy=wrap-saturating" \
  "runner summary must record wrap policy"
require_literal "$runner" "timer_max_observed_gap_ticks=" \
  "runner summary must record max observed gap"
require_literal "$runner" "timer_stale_timeout_behavior=bounded-drop" \
  "runner summary must record stale timeout behavior"
require_literal "$runner" "timer_expired_timeout_behavior=reject" \
  "runner summary must record expired timeout behavior"
require_literal "$runner" "timer_positive_control_case=bounded" \
  "runner summary must include a concrete positive control case"
require_literal "$runner" "timer_timeout_service_positive_delivery_ok=true" \
  "runner must prove the bounded delivery control case"
require_literal "$runner" "timer_timeout_service_stale_timeout_ok=true" \
  "runner must prove the stale timeout negative case"
require_literal "$runner" "timer_timeout_service_expired_timeout_ok=true" \
  "runner must prove the expired timeout negative case"
require_literal "$runner" "timer_timeout_service_timer_owner_visible=true" \
  "runner must fail closed if timer owner evidence is missing"
require_literal "$runner" "timer_timeout_service_tick_source_visible=true" \
  "runner must fail closed if tick source evidence is missing"
require_literal "$runner" 'timer timeout service summary: $timer_timeout_service_summary' \
  "runner must print the timer summary path"

require_literal "$makefile" "x86_64-microkernel-timer-timeout-service-contract:" \
  "Makefile must expose the timer timeout contract target"
require_literal "$makefile" "./tools/check_x86_64_microkernel_timer_timeout_service_contract.sh" \
  "Makefile contract must run the timer timeout guard"
require_literal "$makefile" "x86_64-microkernel-timer-timeout-service: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-timer-timeout-service-contract" \
  "Makefile proof target must use the focused timer contract"
require_literal "$makefile" "DP_MICROKERNEL_TIMER_TIMEOUT_SERVICE_PROOF=1 bash ./tools/x86_64_microkernel_timer_timeout_service.sh" \
  "Makefile proof target must run the focused timer wrapper"

reject_regex "$runner" 'wall_clock=[1-9]|rtc=[1-9]|ntp=[1-9]|cert_time=[1-9]' \
  "timer packet must not claim wall-clock, RTC, NTP, or cert-based time"
reject_regex "$kernel_ledgers" 'DPTIMER:stale-timeout behavior=|DPTIMER:expired-timeout behavior=' \
  "guest ledger must not rely on static behavior labels"
reject_regex "$wrapper" 'https://|curl[[:space:]]+-k|openssl|rustls|embedded-tls|webpki|aws-lc-rs|security-grade time|restart/reset|recovery|generic scheduler rewrite|production TCP/IP|hardware readiness' \
  "timer packet must not overclaim scope"

echo "x86_64 microkernel timer timeout service contract guard passed."
