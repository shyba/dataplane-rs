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
service_guard="tools/check_x86_64_microkernel_service_ipc_audit_contract.sh"

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

echo "=== x86_64 Microkernel Service Timeout/Stale Reply Policy Contract Guard ==="

for path in \
  "$main" \
  "$runner" \
  "$matrix_runner" \
  "$matrix_guard" \
  "$makefile" \
  "$plan" \
  "$service_guard"; do
  require_file "$path"
done

require_literal "$plan" "x86_64-microkernel-service-timeout-stale-reply-policy" \
  "TLS-deferred plan must route the service timeout/stale reply packet"
require_literal "$plan" "no generic IPC framework, generic sockets, heap-backed mailbox, or helper" \
  "TLS-deferred plan must keep the generic IPC stop line"
require_literal "$service_guard" "DPMK:IPC-STALE-REPLY-OK" \
  "service IPC guard must preserve stale reply substrate evidence"

for literal in \
  "run_service_timeout_stale_reply_policy_probe" \
  "DPMK:SERVICE-TIMEOUT-ROUTE-INVENTORY:" \
  "DPMK:SERVICE-TIMEOUT-DENIED-ROUTE-OK" \
  "DPMK:SERVICE-TIMEOUT-QUEUE-FULL-OK" \
  "DPMK:SERVICE-TIMEOUT-STALE-REPLY-OK" \
  "DPMK:SERVICE-TIMEOUT-POLICY-DEFERRED-EXPLICIT" \
  "DPMK:SERVICE-TIMEOUT-TASK-FAULTED-DESTINATION:TASK_BLOCK" \
  "DPMK:SERVICE-TIMEOUT-HELPER-SHORTCUT-REJECTED" \
  "DPMK:SERVICE-TIMEOUT-STALE-REPLY-POLICY-OK"; do
  require_literal "$main" "$literal" \
    "guest source must contain service timeout/stale reply literal: $literal"
done

require_literal "$main" "if let Err(reason) = kernel.run_service_ipc_audit_probe()" \
  "boot path must run the service IPC audit before the service timeout packet inspects its counters"
require_literal "$main" "if let Err(reason) = kernel.run_service_timeout_stale_reply_policy_probe()" \
  "boot path must run the service timeout/stale reply packet"
require_fn_literal "$main" run_service_timeout_stale_reply_policy_probe "validate_service_ipc_routes()?" \
  "service timeout packet must validate the route inventory"
require_fn_literal "$main" run_service_timeout_stale_reply_policy_probe "stale_replies" \
  "service timeout packet must inspect stale reply evidence"
require_fn_literal "$main" run_service_timeout_stale_reply_policy_probe "queue_overflow" \
  "service timeout packet must inspect queue-full evidence"
require_fn_literal "$main" run_service_timeout_stale_reply_policy_probe "denied_sends" \
  "service timeout packet must inspect denied-route evidence"

for literal in \
  "DP_MICROKERNEL_SERVICE_TIMEOUT_STALE_REPLY_POLICY_PROOF" \
  "--service-timeout-stale-reply-policy-proof" \
  'service_timeout_stale_reply_policy_summary="$log_dir/x86_64-microkernel-fat32-$run_id.service-timeout-stale-reply-policy.summary"' \
  "service_timeout_route_inventory_ok=true" \
  "service_timeout_denied_route_ok=true" \
  "service_timeout_queue_full_ok=true" \
  "service_timeout_stale_reply_ok=true" \
  "service_timeout_timeout_policy_ok=true" \
  "service_timeout_task_faulted_destination_ok=true" \
  "service_timeout_helper_shortcuts_rejected_ok=true" \
  "service_timeout_tls_debugger_generic_ipc_drift_rejected_ok=true" \
  "service_timeout_marker_ok=true" \
  "service_timeout_stale_reply_policy_summary_status=pass" \
  'service timeout stale reply policy summary: $service_timeout_stale_reply_policy_summary'; do
  require_literal "$runner" "$literal" \
    "runner must contain service timeout/stale reply literal: $literal"
done

for literal in \
  "service-timeout-stale-reply-policy" \
  "make x86_64-microkernel-service-timeout-stale-reply-policy" \
  "x86_64 microkernel service timeout stale reply policy proof passed." \
  "service timeout stale reply policy summary|client log|serial log|qemu log|network pcap"; do
  require_literal "$matrix_runner" "$literal" \
    "validation matrix must include service timeout/stale reply literal: $literal"
  require_literal "$matrix_guard" "$literal" \
    "validation matrix guard must preserve service timeout/stale reply literal: $literal"
done

require_literal "$makefile" "x86_64-microkernel-service-timeout-stale-reply-policy-contract:" \
  "Makefile must expose the service timeout/stale reply contract target"
require_literal "$makefile" "./tools/check_x86_64_microkernel_service_timeout_stale_reply_policy_contract.sh" \
  "Makefile contract target must run this guard"
require_literal "$makefile" "DP_MICROKERNEL_SERVICE_TIMEOUT_STALE_REPLY_POLICY_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --service-timeout-stale-reply-policy-proof" \
  "Makefile proof target must run the focused service timeout/stale reply mode"

for path in "$main" "$runner" "$matrix_runner" "$makefile" "$plan"; do
  reject_regex "$path" 'service[-_ ]?timeout.*(https://|curl[[:space:]]+-k|openssl|rustls|embedded-tls|webpki|aws-lc-rs|certificate|entropy|crypto-provider)' \
    "service timeout packet must not reopen TLS or crypto work in $path"
  reject_regex "$path" 'service[-_ ]?timeout.*(generic IPC framework|GenericIpc|generic actor|heap-backed mailbox|Vec<|HashMap|BTree|Box<|String::new|format!)' \
    "service timeout packet must not add generic IPC or heap-backed guest policy in $path"
  reject_regex "$path" 'service[-_ ]?timeout.*(generic socket|GenericSocket|SocketApi|full TCP compliance|tcp compliance)' \
    "service timeout packet must not add sockets or TCP compliance claims in $path"
  reject_regex "$path" 'service[-_ ]?timeout.*(restart command|reset task|reset command|replay claim|hardware readiness|hardware-ready)' \
    "service timeout packet must not claim restart/replay or hardware readiness in $path"
done

reject_regex "$runner" '/tmp/[^ ]*microkernel|target/[^ ]*\.summary' \
  "service timeout artifacts must stay under /home/user/mnt/dataplane/logs"

echo "x86_64 microkernel service timeout stale reply policy contract OK"
