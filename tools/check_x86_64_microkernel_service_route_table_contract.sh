#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
services="crates/dataplane-x86_64-microkernel-smoke/src/services.rs"
runner="tools/x86_64_microkernel_service_route_table.sh"
fat32_runner="tools/x86_64_microkernel_fat32_run.sh"
mailbox_guard="tools/check_x86_64_microkernel_service_mailbox_envelope_contract.sh"
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

for file in "$main" "$services" "$runner" "$fat32_runner" "$mailbox_guard" "$makefile" "$plan" "$diary"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel Service Route Ledger Contract ==="

for literal in \
  'Mailbox<SERVICE_MAILBOX_CAP>'; do
  require_literal "$main" "$literal"
done

for literal in \
  'const SERVICE_MAILBOX_CAP: usize = 4;' \
  'pub(crate) fn emit_service_mailbox_envelope_ledger()' \
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
  'DPMK:SERVICE-MAILBOX-ENVELOPE-OK'; do
  require_literal "$fat32_runner" "$literal"
done

for literal in \
  'mnt_root="/home/user/mnt/dataplane"' \
  './tools/x86_64_microkernel_fat32_run.sh >"$run_log" 2>&1' \
  'DPMBOX:routes timer=1->1 cli_fs=2->3 fs_block=3->4 tcpip_net=6->5 http_fs=7->3 dhcp_net=8->5' \
  'service_route_table_summary_status=pass' \
  'service_route_table_model=service-mailbox-envelope-ledger' \
  'service_route_table_route_rows=6' \
  'service_route_table_task_rows=8' \
  'service_route_table_request_rows=6' \
  'service_route_table_dynamic_discovery=false' \
  'service_route_table_source_table_claim=false' \
  'service_route_table_denied_route_runtime_claim=false' \
  'service_route_table_stale_reply_runtime_claim=false' \
  'service_route_table_restart=false' \
  'service_route_table_replay=false' \
  'service_route_table_tls=false' \
  'service_route_table_https=false' \
  'service_route_table_benchmark_result=false' \
  'corrupt service route ledger was accepted'; do
  require_literal "$runner" "$literal"
done

require_literal "$mailbox_guard" 'DPMBOX:routes timer=1->1 cli_fs=2->3 fs_block=3->4 tcpip_net=6->5 http_fs=7->3 dhcp_net=8->5'
require_literal "$makefile" 'x86_64-microkernel-service-route-table-contract:'
require_literal "$makefile" './tools/check_x86_64_microkernel_service_route_table_contract.sh'
require_literal "$makefile" 'x86_64-microkernel-service-route-table: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-service-mailbox-envelope-contract x86_64-microkernel-service-route-table-contract'
require_literal "$makefile" './tools/x86_64_microkernel_service_route_table.sh'
require_literal "$plan" '`x86_64-microkernel-service-route-table`'
require_literal "$diary" '`x86_64-microkernel-service-route-table`'

reject_regex "$runner" 'SERVICE_ROUTE_TABLE|ServiceRouteEntry|service-route-table-proof|DP_MICROKERNEL_SERVICE_ROUTE_TABLE_PROOF|dynamic_discovery=true|source_table_claim=true|denied_route_runtime_claim=true|stale_reply_runtime_claim=true|restart=true|replay=true|https://|curl[[:space:]]+-k|strict-five|STRICT_FIVE_CALIBRATION' \
  "service route ledger proof must not revive stale table symbols or cross stop lines"

echo "x86_64 microkernel service route ledger contract OK"
