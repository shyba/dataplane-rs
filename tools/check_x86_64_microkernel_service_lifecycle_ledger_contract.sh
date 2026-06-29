#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

core="crates/dataplane-microkernel-core/src/lib.rs"
main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
cli="crates/dataplane-x86_64-microkernel-smoke/src/cli.rs"
services="crates/dataplane-x86_64-microkernel-smoke/src/services.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
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
  local out="/tmp/dataplane-lifecycle-contract.$$"
  local err="/tmp/dataplane-lifecycle-contract-err.$$"
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

for file in "$core" "$main" "$cli" "$services" "$runner" "$makefile" "$plan" "$diary"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel Service Lifecycle Ledger Contract ==="

for literal in \
  'pub enum TaskStatus {' \
  'Empty,' \
  'Ready,' \
  'Waiting,' \
  'Faulted,' \
  'Stopped,' \
  'status: TaskStatus::Ready' \
  'slot.status = TaskStatus::Faulted;' \
  'pub fn record_fault(&mut self, id: TaskId)'; do
  require_literal "$core" "$literal" "core lifecycle substrate must preserve literal: $literal"
done

for literal in \
  'emit_service_lifecycle_ledger();' \
  'fn register_initial_tasks(&mut self)' \
  'if self.tasks.active_count() != 8' \
  'DPMK:FAULT-CONTAINED'; do
  require_literal "$main" "$literal" "guest lifecycle source must preserve literal: $literal"
done

for literal in \
  'TaskStatus::Faulted => "faulted"' \
  'TaskStatus::Stopped => "stopped"'; do
  require_literal "$cli" "$literal" "guest lifecycle source must preserve literal: $literal"
done

for literal in \
  'pub(crate) fn emit_service_lifecycle_ledger()' \
  'DPMK:SERVICE-LIFECYCLE-LEDGER' \
  'DPLIFE:status-set empty=1 ready=1 waiting=defined faulted=1 stopped=defined degraded=0' \
  'DPLIFE:startup register_initial_tasks active=' \
  'DPLIFE:running timer=DPMK:TIMER service_loop=entered' \
  'DPLIFE:ready timer=ready cli=ready fs=ready block=ready net=ready tcpip=ready http=ready dhcp=ready' \
  'DPLIFE:cli-visible timer=ready fs=ready block=ready tcpip=ready queues=bounded' \
  'DPLIFE:serving cli=DPMK:CLI-COMMANDS-OK http=DPMK:HTTP-GET-OK' \
  'DPLIFE:fault-path task=' \
  'DPLIFE:deferred stopped=not-exercised degraded=not-defined restart=0 replay=0 cleanup_guarantee=0' \
  'DPMK:SERVICE-LIFECYCLE-LEDGER-OK' \
  'fn emit_post_fault_lifecycle_evidence(&self)' \
  'DPLIFE:task block id=' \
  'restart=0 replay=0 cleanup_guarantee=0' \
  'DPMK:SERVICE-LIFECYCLE-FAULT-OK' \
  '.record_fault(TASK_BLOCK)' \
  'if slot.status != TaskStatus::Faulted || slot.counters.faults != 1'; do
  require_literal "$services" "$literal" "guest lifecycle source must preserve literal: $literal"
done

for literal in \
  'DPMK:SERVICE-LIFECYCLE-LEDGER' \
  'DPLIFE:status-set empty=1 ready=1 waiting=defined faulted=1 stopped=defined degraded=0' \
  'DPLIFE:startup register_initial_tasks active=8 task_table_slots=9 status=ready' \
  'DPLIFE:running timer=DPMK:TIMER service_loop=entered' \
  'DPLIFE:ready timer=ready cli=ready fs=ready block=ready net=ready tcpip=ready http=ready dhcp=ready' \
  'DPLIFE:cli-visible timer=ready fs=ready block=ready tcpip=ready queues=bounded' \
  'DPLIFE:serving cli=DPMK:CLI-COMMANDS-OK http=DPMK:HTTP-GET-OK' \
  'DPLIFE:fault-path task=4 status=faulted faults=1 marker=DPMK:FAULT-CONTAINED' \
  'DPLIFE:deferred stopped=not-exercised degraded=not-defined restart=0 replay=0 cleanup_guarantee=0' \
  'DPMK:SERVICE-LIFECYCLE-LEDGER-OK' \
  'DPLIFE:task block id=4 endpoint=4 status=faulted faults=1 restart=0 replay=0 cleanup_guarantee=0' \
  'DPMK:SERVICE-LIFECYCLE-FAULT-OK' \
  'DPCLI:TASK block id=4 endpoint=4 status=ready' \
  'DPMK:FAULT-CONTAINED'; do
  require_literal "$runner" "$literal" "QEMU runner must validate lifecycle evidence: $literal"
done

require_literal "$makefile" 'x86_64-microkernel-service-lifecycle-ledger-contract:' \
  "Makefile must expose service lifecycle ledger contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_service_lifecycle_ledger_contract.sh' \
  "Makefile must run service lifecycle guard"
require_literal "$makefile" 'x86_64-microkernel-service-lifecycle-ledger:' \
  "Makefile must expose service lifecycle proof target"
require_literal "$plan" 'x86_64-microkernel-service-lifecycle-ledger' \
  "plan must keep service lifecycle ledger packet"
require_literal "$diary" 'x86_64-microkernel-service-lifecycle-ledger' \
  "diary must record service lifecycle ledger packet"

reject_regex "$services" 'DPLIFE.*(restart=1|replay=1|cleanup_guarantee=1|degraded=1|stopped=exercised|automatic replay|automatic restart)' \
  "lifecycle ledger must not claim restart, replay, cleanup, degraded, or stopped behavior"
reject_regex "$makefile" 'x86_64-microkernel-service-lifecycle-ledger.*(strict-five|STRICT_FIVE_CALIBRATION|retune|calibration)' \
  "service lifecycle target must not substitute benchmark calibration evidence"

echo "x86_64 microkernel service lifecycle ledger contract OK"
