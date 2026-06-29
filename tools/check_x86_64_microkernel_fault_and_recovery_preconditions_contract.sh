#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
matrix_runner="tools/x86_64_microkernel_validation_matrix_run.sh"
matrix_guard="tools/check_x86_64_microkernel_validation_matrix_contract.sh"
makefile="Makefile"
plan="aidocs/052_microkernel_tls_deferred_appliance_expansion_plan_2026-05-31.md"
frontier="aidocs/050_microkernel_robust_design_frontier_2026-05-30.md"

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
  grep -Fq -- "$literal" "$path" || fail "$note; missing literal in $path: $literal"
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

rust_fn_body() {
  local path="$1"
  local function="$2"
  local body status
  set +e
  body="$(awk -v target="fn ${function}" '
    BEGIN { depth = 0; found = 0; started = 0; saw_brace = 0 }
    !started && index($0, target) {
      started = 1
      found = 1
    }
    started {
      print
      line = $0
      opens = gsub(/\{/, "{", line)
      if (opens > 0) {
        saw_brace = 1
      }
      line = $0
      closes = gsub(/\}/, "}", line)
      depth += opens - closes
      if (saw_brace && depth == 0 && started) {
        exit 0
      }
    }
    END {
      if (!found || depth != 0) {
        exit 42
      }
    }
  ' "$path")"
  status=$?
  set -e
  [[ "$status" -eq 0 ]] || fail "could not extract function body for ${function}"
  printf '%s\n' "$body"
}

require_fn_literal() {
  local path="$1"
  local function="$2"
  local literal="$3"
  local note="$4"
  local body
  body="$(rust_fn_body "$path" "$function")"
  [[ "$body" == *"$literal"* ]] || fail "$note"
}

reject_fn_literal() {
  local path="$1"
  local function="$2"
  local literal="$3"
  local note="$4"
  local body
  body="$(rust_fn_body "$path" "$function")"
  [[ "$body" != *"$literal"* ]] || fail "$note"
}

echo "=== x86_64 Microkernel Fault And Recovery Preconditions Contract Guard ==="

for path in \
  "$main" \
  "$runner" \
  "$matrix_runner" \
  "$matrix_guard" \
  "$makefile" \
  "$plan" \
  "$frontier"; do
  require_file "$path"
done

require_literal "$plan" "x86_64-microkernel-fault-and-recovery-preconditions" \
  "TLS-deferred plan must route the active fault/recovery preconditions packet"
require_literal "$plan" "Restart/reset remains forbidden until cleanup semantics are proven." \
  "TLS-deferred plan must preserve the no-restart lock"
require_literal "$frontier" "fault/recovery preconditions" \
  "frontier must record the completed fault/recovery preconditions packet"

require_literal "$main" 'TaskStatus::Faulted' \
  "fault/recovery packet must preserve the faulted-state evidence"

require_literal "$matrix_runner" 'fault-and-recovery-preconditions' \
  "validation matrix must include the fault/recovery scenario"
require_literal "$matrix_runner" "fault-and-recovery-preconditions) echo 'make x86_64-microkernel-fault-and-recovery-preconditions' ;;" \
  "validation matrix must dispatch fault/recovery through Make"
require_literal "$matrix_runner" "fault-and-recovery-preconditions) echo 'x86_64 microkernel fault and recovery preconditions proof passed.' ;;" \
  "validation matrix must require the fault/recovery marker"
require_literal "$matrix_runner" "fault-and-recovery-preconditions) echo 'fault and recovery preconditions summary|client log|serial log|qemu log|network pcap' ;;" \
  "validation matrix must require fault/recovery artifacts"
require_literal "$matrix_guard" 'fault-and-recovery-preconditions' \
  "validation matrix guard must preserve the fault/recovery scenario"

require_literal "$makefile" "x86_64-microkernel-fault-and-recovery-preconditions-contract:" \
  "Makefile must expose the fault/recovery contract target"
require_literal "$makefile" "./tools/check_x86_64_microkernel_fault_and_recovery_preconditions_contract.sh" \
  "Makefile contract target must run this guard"
require_literal "$makefile" "x86_64-microkernel-fault-and-recovery-preconditions: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-service-ipc-audit-contract x86_64-microkernel-memory-isolation-map-contract x86_64-microkernel-operator-recovery-runbook-contract x86_64-microkernel-fault-and-recovery-preconditions-contract" \
  "Makefile proof target must preserve the fault/recovery dependency chain"

for path in "$main" "$runner" "$matrix_runner" "$makefile" "$plan" "$frontier"; do
  reject_regex "$path" 'fault[-_ ]?recovery.*(https://|curl[[:space:]]+-k|openssl|rustls|embedded-tls|webpki|aws-lc-rs|crypto-provider|certificate|entropy)' \
    "fault/recovery packet must not reopen TLS or crypto work in $path"
  reject_regex "$path" 'fault[-_ ]?recovery.*(generic socket|GenericSocket|SocketApi|full TCP compliance|tcp compliance)' \
    "fault/recovery packet must not add generic socket or TCP compliance claims in $path"
  reject_regex "$path" 'fault[-_ ]?recovery.*(hardware readiness|hardware-ready|HIL ready)' \
    "fault/recovery packet must not claim hardware readiness in $path"
  reject_regex "$path" 'fault[-_ ]?recovery.*(STRICT_FIVE_CALIBRATION|benchmark[[:space:]]+tuning|retun(e|ing))' \
    "fault/recovery packet must not tune benchmarks in $path"
done

reject_regex "$runner" '/tmp/[^ ]*microkernel|target/[^ ]*\.summary' \
  "fault/recovery artifacts must stay under /home/user/mnt/dataplane/logs"

echo "x86_64 microkernel fault and recovery preconditions contract OK"
