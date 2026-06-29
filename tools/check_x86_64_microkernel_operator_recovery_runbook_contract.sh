#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
matrix_runner="tools/x86_64_microkernel_validation_matrix_run.sh"
matrix_guard="tools/check_x86_64_microkernel_validation_matrix_contract.sh"
makefile="Makefile"
plan="aidocs/051_microkernel_pre_tls_appliance_plan_2026-05-30.md"
cli_guard="tools/check_x86_64_microkernel_cli_operator_contract.sh"
memory_guard="tools/check_x86_64_microkernel_memory_isolation_map_contract.sh"
service_guard="tools/check_x86_64_microkernel_service_ipc_audit_contract.sh"

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
  local matches status
  set +e
  matches="$(rg -n -- "$regex" "$file")"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    printf '%s\n' "$matches"
    fail "$note"
  fi
  [[ "$status" -eq 1 ]] || fail "could not scan $file for forbidden regex: $regex"
}

rust_fn_body() {
  local file="$1"
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
  ' "$file")"
  status=$?
  set -e
  [[ "$status" -eq 0 ]] || fail "could not extract function body for ${function}"
  printf '%s\n' "$body"
}

require_fn_literal() {
  local file="$1"
  local function="$2"
  local needle="$3"
  local note="$4"
  local body
  body="$(rust_fn_body "$file" "$function")"
  [[ "$body" == *"$needle"* ]] || fail "$note"
}

reject_fn_literal() {
  local file="$1"
  local function="$2"
  local needle="$3"
  local note="$4"
  local body
  body="$(rust_fn_body "$file" "$function")"
  [[ "$body" != *"$needle"* ]] || fail "$note"
}

for file in \
  "$main" \
  "$runner" \
  "$matrix_runner" \
  "$matrix_guard" \
  "$makefile" \
  "$plan" \
  "$cli_guard" \
  "$memory_guard" \
  "$service_guard"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel Operator Recovery Runbook Contract Guard ==="

for literal in \
  'fn run_operator_recovery_runbook_probe(&mut self)' \
  'kernel.run_operator_recovery_runbook_probe()' \
  'DPCLI:RECOVERY-STATUS-BEGIN' \
  'DPCLI:RECOVERY-STATUS-END' \
  'DPCLI:RECOVERY service=status inspectable=1 action=preserve-logs-stop-rerun' \
  'DPMK:OPERATOR-RECOVERY-STATUS-OK' \
  'DPMK:OPERATOR-RECOVERY-NO-RESTART-OK' \
  'DPMK:OPERATOR-RECOVERY-SAFE-ACTION:preserve-logs-stop-rerun' \
  'DPMK:OPERATOR-RECOVERY-RUNBOOK-OK' \
  'DPMK:RESET-PRECONDITION-MAILBOX-DRAIN-OK' \
  'DPMK:RESET-CANCEL-REQUESTS:1' \
  'DPMK:RESET-CANCEL-REPLIES:1' \
  'DPMK:RESET-COUNTERS-PRESERVED:TASK_BLOCK:1' \
  'DPMK:RESET-PRECONDITION-NO-RESTART' \
  'DPMK:FAULT-COUNT:TASK_BLOCK:' \
  'DPSTATUS:RECOVERY task=block' \
  'status=faulted' \
  'denied_region=block_task_region' \
  'denied_right=block-write' \
  'no_restart=1' \
  'safe_action=preserve-logs-stop-rerun'; do
  require_literal "$main" "$literal" "guest source must contain operator recovery literal: $literal"
done

require_fn_literal "$main" run_operator_recovery_runbook_probe 'TaskStatus::Faulted' \
  "recovery probe must prove the affected task remains faulted"
require_fn_literal "$main" run_operator_recovery_runbook_probe 'block.counters.faults != 1' \
  "recovery probe must validate the contained fault counter"
require_fn_literal "$main" run_operator_recovery_runbook_probe 'operator_status_snapshot()?' \
  "recovery probe must use the bounded operator status snapshot"
require_fn_literal "$main" run_operator_recovery_runbook_probe 'render_operator_status_body(snapshot, &mut body)?' \
  "recovery probe must render the same status body exposed by CLI/HTTP"
require_fn_literal "$main" render_operator_status_body 'DPSTATUS:RECOVERY task=block' \
  "status output must include explicit recovery state"
require_fn_literal "$main" render_operator_status_body 'no_restart=' \
  "status output must make no-restart state explicit"
require_fn_literal "$main" render_operator_status_body 'safe_action=preserve-logs-stop-rerun' \
  "status output must name the allowed operator action"
require_fn_literal "$main" record_block_fault 'record_fault(TASK_BLOCK)' \
  "fault counter must remain task-associated"
require_fn_literal "$main" run_reset_precondition_probe 'DPMK:RESET-PRECONDITION-NO-RESTART' \
  "reset precondition probe must stay documentation/no-restart only"

for literal in \
  'operator_recovery_summary="$log_dir/x86_64-microkernel-fat32-$run_id.operator-recovery-runbook.summary"' \
  'DP_MICROKERNEL_OPERATOR_RECOVERY_RUNBOOK_PROOF' \
  '--operator-recovery-runbook-proof' \
  'DP_OPERATOR_RECOVERY_RUNBOOK_PROOF' \
  'operator_recovery_status_ok=true' \
  'operator_recovery_no_restart_ok=true' \
  'operator_recovery_safe_action_ok=true' \
  'operator_recovery_fault_count_ok=true' \
  'operator_recovery_denied_region_ok=true' \
  'operator_recovery_denied_right_ok=true' \
  'operator_recovery_safe_action_ok=true' \
  'operator_recovery_summary_status=pass' \
  'x86_64 microkernel operator recovery runbook proof passed.' \
  'operator recovery summary: $operator_recovery_summary'; do
  require_literal "$runner" "$literal" "runner must contain operator recovery literal: $literal"
done

for literal in \
  'operator-recovery-runbook' \
  "operator-recovery-runbook) echo 'make x86_64-microkernel-operator-recovery-runbook' ;;" \
  "operator-recovery-runbook) echo 'x86_64 microkernel operator recovery runbook proof passed.' ;;" \
  "operator-recovery-runbook) echo 'operator recovery summary|client log|serial log|qemu log|network pcap' ;;"; do
  require_literal "$matrix_runner" "$literal" "validation matrix must contain operator recovery literal: $literal"
done

require_literal "$makefile" 'x86_64-microkernel-operator-recovery-runbook-contract:' \
  "Makefile must expose the operator recovery contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_operator_recovery_runbook_contract.sh' \
  "Makefile operator recovery contract must run the focused guard"
require_literal "$makefile" 'x86_64-microkernel-operator-recovery-runbook: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-service-ipc-audit-contract x86_64-microkernel-memory-isolation-map-contract x86_64-microkernel-operator-recovery-runbook-contract' \
  "Makefile operator recovery proof target must preserve neighboring contract coverage"
require_literal "$makefile" 'DP_MICROKERNEL_OPERATOR_RECOVERY_RUNBOOK_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --operator-recovery-runbook-proof' \
  "Makefile operator recovery target must run the focused proof mode"

for command in \
  '"restart"' '"restart task"' '"restart block"' \
  '"reset"' '"reset task"' '"reset block"' \
  '"shell"' '"debug"' '"debugger"' \
  '"peek"' '"poke"' '"dump"' '"memdump"' '"readmem"' '"writemem"' \
  '"mmio"' '"pci"' '"page-table"' '"page table"' '"pagetable"' '"cr3"' '"pte"' '"ioport"'; do
  reject_fn_literal "$main" parse_cli_command "$command" \
    "CLI parser must not accept recovery/debug escape command: $command"
done

reject_regex "$main" 'CliCommand::(Restart|Reset|Shell|Debug|Peek|Poke|Dump|MemDump|ReadMem|WriteMem|Mmio|PageTable)' \
  "guest must not add privileged recovery/debug CLI variants"
reject_regex "$runner" 'curl[[:space:]]+-k|https://|openssl|rustls|embedded-tls|webpki|aws-lc-rs' \
  "operator recovery packet must not reopen TLS"
reject_regex "$runner" 'STRICT_FIVE_CALIBRATION|benchmark[[:space:]]+tuning|retun(e|ing)' \
  "operator recovery packet must not tune benchmarks"
reject_regex "$runner" '/tmp/[^ ]*microkernel|target/[^ ]*\.summary' \
  "operator recovery artifacts must stay under /home/user/mnt/dataplane/logs"

echo "x86_64 microkernel operator recovery runbook contract OK"
