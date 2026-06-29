#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
runner="tools/x86_64_microkernel_status_route_unification.sh"
fat32_runner="tools/x86_64_microkernel_fat32_run.sh"
route_guard="tools/check_x86_64_microkernel_service_route_table_contract.sh"
ipc_guard="tools/check_x86_64_microkernel_service_ipc_audit_contract.sh"
boundary_guard="tools/check_x86_64_microkernel_service_boundary_audit_contract.sh"
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

for file in "$main" "$runner" "$fat32_runner" "$route_guard" "$ipc_guard" "$boundary_guard" "$makefile" "$plan" "$diary"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel Status Route Unification Contract ==="

for literal in \
  'DPMK:SERVICE-MAILBOX-ENVELOPE-LEDGER' \
  'DPMBOX:shape fields=from,to,request,capability,body inline_bytes=' \
  'DPMBOX:tasks timer=' \
  'DPMBOX:requests timer_tick=' \
  'DPMBOX:routes timer=1->1 cli_fs=2->3 fs_block=3->4 tcpip_net=6->5 http_fs=7->3 dhcp_net=8->5' \
  'DPMBOX:errors queue_full=MailboxError::Full empty=MailboxError::Empty send=mailbox-send recv=mailbox-recv timeout=deferred stale_reply=deferred denied_route=deferred' \
  'DPMK:SERVICE-MAILBOX-ENVELOPE-OK' \
  'DPMK:IPC-OK'; do
  require_literal "$main" "$literal"
done

for literal in \
  'DPMK:SERVICE-MAILBOX-ENVELOPE-LEDGER' \
  'DPMBOX:shape fields=from' \
  'DPMBOX:tasks timer=1 cli=2 fs=3 block=4 net=5 tcpip=6 http=7 dhcp=8' \
  'DPMBOX:requests timer_tick=1 cli_fs=2 fs_block=3 tcpip_net=4 http_fs=5 dhcp_net=6' \
  'DPMBOX:routes timer=1->1 cli_fs=2->3 fs_block=3->4 tcpip_net=6->5 http_fs=7->3 dhcp_net=8->5' \
  'DPMBOX:errors queue_full=MailboxError::Full empty=MailboxError::Empty send=mailbox-send recv=mailbox-recv timeout=deferred stale_reply=deferred denied_route=deferred' \
  'DPMK:SERVICE-MAILBOX-ENVELOPE-OK' \
  'DPMK:IPC-OK'; do
  require_literal "$fat32_runner" "$literal"
done

for literal in \
  'mnt_root="/home/user/mnt/dataplane"' \
  './tools/x86_64_microkernel_fat32_run.sh >"$run_log" 2>&1' \
  'DPMBOX:routes timer=1->1 cli_fs=2->3 fs_block=3->4 tcpip_net=6->5 http_fs=7->3 dhcp_net=8->5' \
  'status_route_unification_summary_status=pass' \
  'status_route_unification_model=deferred-mailbox-route-ledger-compatibility' \
  'status_route_unification_route_ledger_ok=true' \
  'status_route_unification_ipc_marker_ok=true' \
  'status_route_unification_corrupt_marker_rejected=true' \
  'status_route_unification_status_surface_present=false' \
  'status_route_unification_cli_http_same_body_claim=false' \
  'status_route_unification_source_table_claim=false' \
  'status_route_unification_route_probe_claim=false' \
  'status_route_unification_runner_mode_claim=false' \
  'status_route_unification_restart=false' \
  'status_route_unification_replay=false' \
  'status_route_unification_tls=false' \
  'status_route_unification_https=false' \
  'status_route_unification_benchmark_result=false' \
  'corrupt status route compatibility ledger was accepted'; do
  require_literal "$runner" "$literal"
done

require_literal "$route_guard" 'service_route_table_model=service-mailbox-envelope-ledger'
require_literal "$ipc_guard" 'service_ipc_audit_model=service-mailbox-envelope-ledger'
require_literal "$boundary_guard" 'service_boundary_audit_status_route_unification_claim=false'
require_literal "$makefile" 'x86_64-microkernel-status-route-unification-contract:'
require_literal "$makefile" './tools/check_x86_64_microkernel_status_route_unification_contract.sh'
require_literal "$makefile" 'x86_64-microkernel-status-route-unification: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-service-route-table-contract x86_64-microkernel-service-ipc-audit-contract x86_64-microkernel-service-boundary-audit-contract x86_64-microkernel-status-route-unification-contract'
require_literal "$makefile" './tools/x86_64_microkernel_status_route_unification.sh'
require_literal "$plan" '`x86_64-microkernel-status-route-unification`'
require_literal "$diary" '`x86_64-microkernel-status-route-unification`'

reject_regex "$makefile" 'DP_MICROKERNEL_STATUS_ROUTE_UNIFICATION_PROOF|--status-route-unification-proof' \
  "Makefile must not call the removed status route unification proof mode"
reject_regex "$runner" 'SERVICE_ROUTE_TABLE|ServiceRouteEntry|DPSTATUS:ROUTE-TABLE|STATUS-ROUTE-UNIFICATION-SOURCE|status_surface_present=true|cli_http_same_body_claim=true|source_table_claim=true|route_probe_claim=true|runner_mode_claim=true|https://|curl[[:space:]]+-k|strict-five|STRICT_FIVE_CALIBRATION' \
  "status route compatibility proof must not revive stale status/route-table claims"

echo "x86_64 microkernel status route compatibility contract OK"
