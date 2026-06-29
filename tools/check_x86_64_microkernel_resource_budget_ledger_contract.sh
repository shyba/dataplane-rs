#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
kernel="crates/dataplane-x86_64-microkernel-smoke/src/kernel.rs"
kernel_ledgers="crates/dataplane-x86_64-microkernel-smoke/src/kernel_ledgers.rs"
runner="tools/x86_64_microkernel_resource_budget_ledger.sh"
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
  local status
  set +e
  rg -n -- "$regex" "$file" >/tmp/dataplane-budget-guard.$$ 2>/tmp/dataplane-budget-guard-err.$$
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    cat /tmp/dataplane-budget-guard.$$
    rm -f /tmp/dataplane-budget-guard.$$ /tmp/dataplane-budget-guard-err.$$
    fail "$note"
  fi
  if [[ "$status" -ne 1 ]]; then
    cat /tmp/dataplane-budget-guard-err.$$ >&2 || true
    rm -f /tmp/dataplane-budget-guard.$$ /tmp/dataplane-budget-guard-err.$$
    fail "could not scan $file for forbidden regex: $regex"
  fi
  rm -f /tmp/dataplane-budget-guard.$$ /tmp/dataplane-budget-guard-err.$$
}

for file in "$main" "$kernel" "$kernel_ledgers" "$runner" "$makefile" "$plan" "$diary"; do
  require_file "$file"
done

for literal in \
  'mnt_root="/home/user/mnt/dataplane"' \
  'TMPDIR="$mnt_root/tmp" ./tools/x86_64_microkernel_fat32_run.sh >"$run_log" 2>&1' \
  'DPBUDGET:task-bytes block=16384 fs=4096 net=65536' \
  'DPBUDGET:virtq-bytes block=12288 net=16384 block_cap=256 net_cap=256' \
  'DPBUDGET:protocol-bytes cli_line=64 cli_commands=24 http_line=96 http_headers=384 net_reply=512 udp_payload=16' \
  'DPBUDGET:fat32-bytes sector=512 index_file=256 large_file=512' \
  'resource_budget_ledger_summary_status=pass' \
  'resource_budget_ledger_begin_marker_ok=true' \
  'resource_budget_ledger_ok_marker_ok=true' \
  'resource_budget_ledger_task_bytes_block=16384' \
  'resource_budget_ledger_task_bytes_fs=4096' \
  'resource_budget_ledger_task_bytes_net=65536' \
  'resource_budget_ledger_virtq_bytes_block=12288' \
  'resource_budget_ledger_virtq_bytes_net=16384' \
  'resource_budget_ledger_block_queue_cap=256' \
  'resource_budget_ledger_net_queue_cap=256' \
  'resource_budget_ledger_cli_line_bytes=64' \
  'resource_budget_ledger_cli_command_count=24' \
  'resource_budget_ledger_http_request_line_bytes=96' \
  'resource_budget_ledger_http_header_bytes=384' \
  'resource_budget_ledger_net_reply_payload_bytes=512' \
  'resource_budget_ledger_udp_payload_bytes=16' \
  'resource_budget_ledger_fat32_sector_bytes=512' \
  'resource_budget_ledger_index_file_bytes=256' \
  'resource_budget_ledger_large_file_bytes=512' \
  'resource_budget_ledger_status_summary_agree=true' \
  'resource_budget_ledger_dynamic_negotiation=false' \
  'resource_budget_ledger_temporary_unbounded=false' \
  'resource_budget_ledger_heap_growth_claim=false' \
  'resource_budget_ledger_heap_mailbox=false' \
  'resource_budget_ledger_generic_actor_framework=false' \
  'resource_budget_ledger_rp2040_networking_claim=false' \
  'resource_budget_ledger_tls=false' \
  'resource_budget_ledger_https=false' \
  'resource_budget_ledger_benchmark_result=false' \
  'duplicate summary key:'; do
  require_literal "$runner" "$literal" "runner must preserve resource budget runtime proof literal: $literal"
done

echo "=== x86_64 Microkernel Resource Budget Ledger Contract ==="

for literal in \
  'emit_resource_budget_ledger();'; do
  require_literal "$kernel" "$literal" "guest orchestration must call resource budget ledger: $literal"
done

for literal in \
  'fn emit_resource_budget_ledger()' \
  'DPMK:RESOURCE-BUDGET-LEDGER' \
  'DPBUDGET:task-bytes block=' \
  'DPBUDGET:virtq-bytes block=' \
  'DPBUDGET:protocol-bytes cli_line=' \
  'DPBUDGET:fat32-bytes sector=' \
  'DPMK:RESOURCE-BUDGET-LEDGER-OK' \
  'serial::write_decimal(BLOCK_TASK_BYTES as u32)' \
  'serial::write_decimal(FS_TASK_BYTES as u32)' \
  'serial::write_decimal(NET_TASK_BYTES as u32)' \
  'serial::write_decimal(BLOCK_QUEUE_CAP as u32)' \
  'serial::write_decimal(NET_QUEUE_CAP as u32)' \
  'serial::write_decimal(CLI_LINE_BYTES as u32)' \
  'serial::write_decimal(CLI_COMMAND_COUNT as u32)' \
  'serial::write_decimal(HTTP_REQUEST_LINE_BYTES as u32)' \
  'serial::write_decimal(HTTP_HEADER_BYTES as u32)' \
  'serial::write_decimal(NET_REPLY_PAYLOAD_BYTES as u32)' \
  'serial::write_decimal(CONTROL_PROTOCOL_V1_MAX_PAYLOAD_BYTES as u32)' \
  'serial::write_decimal(FAT32_BYTES_PER_SECTOR as u32)' \
  'serial::write_decimal(HTTP_INDEX_FILE_BYTES as u32)' \
  'serial::write_decimal(HTTP_LARGE_FILE_BYTES as u32)'; do
  require_literal "$kernel_ledgers" "$literal" "guest source must expose fixed budget literal: $literal"
done

require_literal "$makefile" 'x86_64-microkernel-resource-budget-ledger-contract:' \
  "Makefile must expose the resource budget ledger contract"
require_literal "$makefile" 'x86_64-microkernel-resource-budget-ledger:' \
  "Makefile must expose the resource budget ledger proof target"
require_literal "$makefile" './tools/check_x86_64_microkernel_resource_budget_ledger_contract.sh' \
  "Makefile must run the resource budget ledger contract guard"
require_literal "$makefile" './tools/x86_64_microkernel_resource_budget_ledger.sh' \
  "Makefile proof target must run the resource budget ledger runtime proof"
require_literal "$plan" 'x86_64-microkernel-resource-budget-ledger' \
  "plan must keep the resource budget ledger packet"
require_literal "$diary" 'x86_64-microkernel-resource-budget-ledger' \
  "diary must record the resource budget ledger packet"

reject_regex "$kernel_ledgers" 'DPBUDGET.*(Vec|String|Box|BTree|HashMap|format!|alloc::|std::)' \
  "budget ledger must not add heap-backed formatting or containers"
reject_regex "$kernel_ledgers" 'extern crate alloc|alloc::|Vec<|String::|Box<|HashMap|BTreeMap|format!|to_vec|to_string|reserve\(|resize\(' \
  "x86_64 microkernel guest must not add heap-backed dynamic growth for this packet"
reject_regex "crates/dataplane-microkernel-core/src/lib.rs" 'extern crate alloc|alloc::|Vec<|String::|Box<|HashMap|BTreeMap|format!|to_vec|to_string|reserve\(|resize\(' \
  "microkernel core must not add heap-backed dynamic growth for this packet"
reject_regex "$runner" 'resource_budget_ledger_.*(dynamic_negotiation=true|temporary_unbounded=true|heap_growth_claim=true|tls=true|https=true|benchmark_result=true)' \
  "budget ledger summary must not claim dynamic budget, heap growth, TLS, or benchmark evidence"
reject_regex "$kernel_ledgers" 'DPMK:RESOURCE-BUDGET.*(dynamic|negotiat|temporary|unbounded)' \
  "budget ledger must not claim dynamic or unbounded budget behavior"
reject_regex "$makefile" 'x86_64-microkernel-resource-budget-ledger.*(strict-five|STRICT_FIVE_CALIBRATION|retune|calibration)' \
  "budget ledger target must not substitute benchmark calibration evidence"

echo "x86_64 microkernel resource budget ledger contract OK"
