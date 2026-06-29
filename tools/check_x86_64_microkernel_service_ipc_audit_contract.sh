#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
kernel="crates/dataplane-x86_64-microkernel-smoke/src/kernel.rs"
task_mailbox="crates/dataplane-x86_64-microkernel-smoke/src/task_mailbox.rs"
services="crates/dataplane-x86_64-microkernel-smoke/src/services.rs"
runner="tools/x86_64_microkernel_service_ipc_audit.sh"
fat32_runner="tools/x86_64_microkernel_fat32_run.sh"
route_runner="tools/x86_64_microkernel_service_route_table.sh"
mailbox_guard="tools/check_x86_64_microkernel_service_mailbox_envelope_contract.sh"
route_guard="tools/check_x86_64_microkernel_service_route_table_contract.sh"
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
  grep -Fq -- "$literal" "$file" || fail "missing literal in $file: $literal"
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

for file in "$main" "$kernel" "$task_mailbox" "$services" "$runner" "$fat32_runner" "$route_runner" "$mailbox_guard" "$route_guard" "$makefile" "$plan" "$diary"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel Service IPC Audit Contract ==="

for literal in \
  'Mailbox<SERVICE_MAILBOX_CAP>' \
  'DPMK:IPC-OK'; do
  require_literal "$kernel" "$literal"
done

for literal in \
  'pub(crate) fn send_to_task(' \
  'mailbox.send(message).map_err(|_| "mailbox-send")?' \
  'pub(crate) fn recv_from_task(' \
  'mailbox.recv().map_err(|_| "mailbox-recv")?'; do
  require_literal "$task_mailbox" "$literal"
done

for literal in \
  'const SERVICE_MAILBOX_CAP: usize = 4;' \
  'DPMK:SERVICE-MAILBOX-ENVELOPE-LEDGER' \
  'DPMBOX:shape fields=from,to,request,capability,body inline_bytes=' \
  'DPMBOX:tasks timer=' \
  'DPMBOX:requests timer_tick=' \
  'DPMBOX:routes timer=1->1 cli_fs=2->3 fs_block=3->4 tcpip_net=6->5 http_fs=7->3 dhcp_net=8->5' \
  'DPMBOX:errors queue_full=MailboxError::Full empty=MailboxError::Empty send=mailbox-send recv=mailbox-recv timeout=deferred stale_reply=deferred denied_route=deferred' \
  'DPMBOX:fault-destination task=' \
  'DPMK:SERVICE-MAILBOX-ENVELOPE-OK'; do
  require_literal "$services" "$literal"
done

for literal in \
  'DPMK:SERVICE-MAILBOX-ENVELOPE-LEDGER' \
  'DPMBOX:tasks timer=1 cli=2 fs=3 block=4 net=5 tcpip=6 http=7 dhcp=8' \
  'DPMBOX:requests timer_tick=1 cli_fs=2 fs_block=3 tcpip_net=4 http_fs=5 dhcp_net=6' \
  'DPMBOX:routes timer=1->1 cli_fs=2->3 fs_block=3->4 tcpip_net=6->5 http_fs=7->3 dhcp_net=8->5' \
  'DPMBOX:errors queue_full=MailboxError::Full empty=MailboxError::Empty send=mailbox-send recv=mailbox-recv timeout=deferred stale_reply=deferred denied_route=deferred' \
  'DPMBOX:fault-destination task=4 status=faulted restart=0 replay=0' \
  'DPMK:SERVICE-MAILBOX-ENVELOPE-OK' \
  'DPMK:IPC-OK'; do
  require_literal "$fat32_runner" "$literal"
done

for literal in \
  'mnt_root="/home/user/mnt/dataplane"' \
  './tools/x86_64_microkernel_fat32_run.sh >"$run_log" 2>&1' \
  'DPMK:IPC-OK' \
  'DPMBOX:routes timer=1->1 cli_fs=2->3 fs_block=3->4 tcpip_net=6->5 http_fs=7->3 dhcp_net=8->5' \
  'DPMBOX:errors queue_full=MailboxError::Full empty=MailboxError::Empty send=mailbox-send recv=mailbox-recv timeout=deferred stale_reply=deferred denied_route=deferred' \
  'service_ipc_audit_summary_status=pass' \
  'service_ipc_audit_model=service-mailbox-envelope-ledger' \
  'service_ipc_audit_ipc_marker_ok=true' \
  'service_ipc_audit_mailbox_shape_ok=true' \
  'service_ipc_audit_route_ledger_ok=true' \
  'service_ipc_audit_send_error_name=mailbox-send' \
  'service_ipc_audit_recv_error_name=mailbox-recv' \
  'service_ipc_audit_queue_full_policy=MailboxError::Full' \
  'service_ipc_audit_timeout_runtime_claim=false' \
  'service_ipc_audit_stale_reply_runtime_claim=false' \
  'service_ipc_audit_denied_route_runtime_claim=false' \
  'service_ipc_audit_recovery_runtime_claim=false' \
  'service_ipc_audit_source_table_claim=false' \
  'service_ipc_audit_generic_ipc=false' \
  'service_ipc_audit_heap_mailbox=false' \
  'service_ipc_audit_actor_framework=false' \
  'service_ipc_audit_restart=false' \
  'service_ipc_audit_replay=false' \
  'service_ipc_audit_tls=false' \
  'service_ipc_audit_https=false' \
  'service_ipc_audit_benchmark_result=false' \
  'service_ipc_audit_hardware_readiness=false' \
  'service_ipc_audit_debugger=false' \
  'service_ipc_audit_raw_memory=false' \
  'service_ipc_audit_mmio=false' \
  'service_ipc_audit_page_table=false' \
  'corrupt service IPC ledger was accepted'; do
  require_literal "$runner" "$literal"
done

require_literal "$route_runner" 'service_route_table_model=service-mailbox-envelope-ledger'
require_literal "$mailbox_guard" 'DPMBOX:errors queue_full=MailboxError::Full empty=MailboxError::Empty send=mailbox-send recv=mailbox-recv timeout=deferred stale_reply=deferred denied_route=deferred'
require_literal "$route_guard" 'service_route_table_source_table_claim=false'
require_literal "$makefile" 'x86_64-microkernel-service-ipc-audit-contract:'
require_literal "$makefile" './tools/check_x86_64_microkernel_service_ipc_audit_contract.sh'
require_literal "$makefile" 'x86_64-microkernel-service-ipc-audit: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-service-mailbox-envelope-contract x86_64-microkernel-service-route-table-contract x86_64-microkernel-service-ipc-audit-contract'
require_literal "$makefile" './tools/x86_64_microkernel_service_ipc_audit.sh'
require_literal "$plan" '`x86_64-microkernel-service-ipc-audit`'
require_literal "$diary" '`x86_64-microkernel-service-ipc-audit`'

reject_regex "$runner" 'SERVICE_ROUTE_TABLE|ServiceRouteEntry|service-ipc-audit-proof|DP_MICROKERNEL_SERVICE_IPC_AUDIT_PROOF|denied_route_runtime_claim=true|stale_reply_runtime_claim=true|recovery_runtime_claim=true|source_table_claim=true|generic_ipc=true|heap_mailbox=true|actor_framework=true|restart=true|replay=true|https://|curl[[:space:]]+-k|strict-five|STRICT_FIVE_CALIBRATION' \
  "service IPC audit proof must not revive stale table symbols or broaden deferred claims"

echo "x86_64 microkernel service IPC audit contract OK"
