#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
kernel="crates/dataplane-x86_64-microkernel-smoke/src/kernel.rs"
task_mailbox="crates/dataplane-x86_64-microkernel-smoke/src/task_mailbox.rs"
scenarios="crates/dataplane-x86_64-microkernel-smoke/src/scenarios.rs"
scenarios_src="crates/dataplane-x86_64-microkernel-smoke/src/scenarios"
services="crates/dataplane-x86_64-microkernel-smoke/src/services.rs"
runner="tools/x86_64_microkernel_service_boundary_audit.sh"
fat32_runner="tools/x86_64_microkernel_fat32_run.sh"
mailbox_guard="tools/check_x86_64_microkernel_service_mailbox_envelope_contract.sh"
route_guard="tools/check_x86_64_microkernel_service_route_table_contract.sh"
ipc_guard="tools/check_x86_64_microkernel_service_ipc_audit_contract.sh"
lifecycle_guard="tools/check_x86_64_microkernel_service_lifecycle_ledger_contract.sh"
makefile="Makefile"
plan="aidocs/055_microkernel_tls_deferred_robust_appliance_plan_2026-05-31.md"
diary="aidocs/058_x86_64_microkernel_main_split_diary_2026-06-01.md"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_file() {
  [[ -f "$1" ]] || fail "missing required file: $1"
}

require_path() {
  [[ -e "$1" ]] || fail "missing required path: $1"
}

require_literal() {
  local file="$1"
  local literal="$2"
  rg -q --fixed-strings -- "$literal" "$file" || fail "missing literal in $file: $literal"
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

for file in "$main" "$kernel" "$task_mailbox" "$scenarios" "$services" "$runner" "$fat32_runner" "$mailbox_guard" "$route_guard" "$ipc_guard" "$lifecycle_guard" "$makefile" "$plan" "$diary"; do
  require_file "$file"
done
require_path "$scenarios_src"

echo "=== x86_64 Microkernel Service Boundary Audit Contract ==="

for literal in \
  'DPMK:IPC-OK'; do
  require_literal "$kernel" "$literal"
done

for literal in \
  'Mailbox<SERVICE_MAILBOX_CAP>' \
  'fn send_to_task(' \
  'fn recv_from_task('; do
  require_literal "$task_mailbox" "$literal"
done

for literal in \
  'fn run_http_service_boundary_probe(&mut self)' \
  'DPMK:HTTP-POLICY-200' \
  'DPMK:HTTP-POLICY-404' \
  'DPMK:HTTP-POLICY-405' \
  'DPMK:HTTP-POLICY-413' \
  'DPMK:HTTP-POLICY-500' \
  'DPMK:HTTP-PARSER-MALFORMED:' \
  'DPMK:HTTP-PARSER-LINE-TOO-LONG:' \
  'DPMK:HTTP-POLICY-OK'; do
  require_literal "$scenarios_src" "$literal"
done

for literal in \
  'const SERVICE_MAILBOX_CAP: usize = 4;' \
  'pub(crate) fn emit_service_mailbox_envelope_ledger()' \
  'pub(crate) fn emit_service_lifecycle_ledger()' \
  'DPMK:SERVICE-MAILBOX-ENVELOPE-LEDGER' \
  'DPMBOX:routes timer=1->1 cli_fs=2->3 fs_block=3->4 tcpip_net=6->5 http_fs=7->3 dhcp_net=8->5' \
  'DPMBOX:errors queue_full=MailboxError::Full empty=MailboxError::Empty send=mailbox-send recv=mailbox-recv timeout=deferred stale_reply=deferred denied_route=deferred' \
  'DPMK:SERVICE-LIFECYCLE-LEDGER' \
  'DPLIFE:fault-path task=' \
  'DPMK:SERVICE-LIFECYCLE-LEDGER-OK' \
  'DPMK:SERVICE-LIFECYCLE-FAULT-OK'; do
  require_literal "$services" "$literal"
done

for literal in \
  'DPMK:SERVICE-MAILBOX-ENVELOPE-LEDGER' \
  'DPMBOX:tasks timer=1 cli=2 fs=3 block=4 net=5 tcpip=6 http=7 dhcp=8' \
  'DPMBOX:routes timer=1->1 cli_fs=2->3 fs_block=3->4 tcpip_net=6->5 http_fs=7->3 dhcp_net=8->5' \
  'DPMK:SERVICE-LIFECYCLE-LEDGER' \
  'DPLIFE:fault-path task=4 status=faulted faults=1 marker=DPMK:FAULT-CONTAINED' \
  'DPMK:SERVICE-LIFECYCLE-LEDGER-OK' \
  'DPMK:SERVICE-LIFECYCLE-FAULT-OK' \
  'DPMK:IPC-OK'; do
  require_literal "$fat32_runner" "$literal"
done

for literal in \
  'mnt_root="/home/user/mnt/dataplane"' \
  './tools/x86_64_microkernel_fat32_run.sh >"$run_log" 2>&1' \
  'DPMK:HTTP-POLICY-OK' \
  'DPMK:SERVICE-LIFECYCLE-FAULT-OK' \
  'DPMK:IPC-OK' \
  'service_boundary_audit_summary_status=pass' \
  'service_boundary_audit_model=mailbox-route-ipc-lifecycle-http-ledger' \
  'service_boundary_audit_mailbox_ledger_ok=true' \
  'service_boundary_audit_route_ledger_ok=true' \
  'service_boundary_audit_ipc_ledger_ok=true' \
  'service_boundary_audit_lifecycle_ledger_ok=true' \
  'service_boundary_audit_http_policy_ok=true' \
  'service_boundary_audit_http_status_rows=5' \
  'service_boundary_audit_corrupt_marker_rejected=true' \
  'service_boundary_audit_source_table_claim=false' \
  'service_boundary_audit_status_route_unification_claim=false' \
  'service_boundary_audit_denied_route_runtime_claim=false' \
  'service_boundary_audit_stale_reply_runtime_claim=false' \
  'service_boundary_audit_restart=false' \
  'service_boundary_audit_replay=false' \
  'service_boundary_audit_tls=false' \
  'service_boundary_audit_https=false' \
  'service_boundary_audit_benchmark_result=false' \
  'corrupt service boundary ledger was accepted'; do
  require_literal "$runner" "$literal"
done

require_literal "$mailbox_guard" 'DPMBOX:shape fields=from'
require_literal "$route_guard" 'service_route_table_model=service-mailbox-envelope-ledger'
require_literal "$ipc_guard" 'service_ipc_audit_model=service-mailbox-envelope-ledger'
require_literal "$lifecycle_guard" 'DPMK:SERVICE-LIFECYCLE-LEDGER'
require_literal "$makefile" 'x86_64-microkernel-service-boundary-audit-contract:'
require_literal "$makefile" './tools/check_x86_64_microkernel_service_boundary_audit_contract.sh'
require_literal "$makefile" 'x86_64-microkernel-service-boundary-audit: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-service-mailbox-envelope-contract x86_64-microkernel-service-route-table-contract x86_64-microkernel-service-ipc-audit-contract x86_64-microkernel-service-lifecycle-ledger-contract x86_64-microkernel-service-boundary-audit-contract'
require_literal "$makefile" './tools/x86_64_microkernel_service_boundary_audit.sh'
require_literal "$plan" '`x86_64-microkernel-service-boundary-audit`'
require_literal "$diary" '`x86_64-microkernel-service-boundary-audit`'

reject_regex "$runner" 'SERVICE_ROUTE_TABLE|ServiceRouteEntry|status-route-unification-proof|DP_MICROKERNEL_STATUS_ROUTE_UNIFICATION_PROOF|source_table_claim=true|status_route_unification_claim=true|denied_route_runtime_claim=true|stale_reply_runtime_claim=true|restart=true|replay=true|https://|curl[[:space:]]+-k|strict-five|STRICT_FIVE_CALIBRATION' \
  "service boundary audit proof must not revive stale route/status symbols or broaden claims"

echo "x86_64 microkernel service boundary audit contract OK"
