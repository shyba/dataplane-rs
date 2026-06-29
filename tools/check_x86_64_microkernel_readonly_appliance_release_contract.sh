#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

release="tools/x86_64_microkernel_readonly_appliance_release.sh"
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
  local out="/tmp/dataplane-readonly-release.$$"
  local err="/tmp/dataplane-readonly-release-err.$$"
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

for file in "$release" "$makefile" "$plan" "$diary"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel Readonly Appliance Release Contract ==="

for literal in \
  'mnt_root="/home/user/mnt/dataplane"' \
  'tools/x86_64_microkernel_fat32_reproducibility.sh >"$repro_log"' \
  'tools/x86_64_microkernel_fat32_run.sh >"$smoke_log"' \
  'fat32_repro_summary="$(extract_artifact "$repro_log" "summary")"' \
  'serial_log="$(extract_artifact "$smoke_log" "serial log")"' \
  'client_log="$(extract_artifact "$smoke_log" "client log")"' \
  'network_pcap="$(extract_artifact "$smoke_log" "network pcap")"' \
  'DPMK:RESOURCE-BUDGET-LEDGER-OK' \
  'DPMK:PROTOCOL-INPUT-BOUNDS-LEDGER-OK' \
  'DPMK:TIMER-TIMEOUT-SERVICE-LEDGER-OK' \
  'DPMK:SERVICE-MAILBOX-ENVELOPE-OK' \
  'DPCLI:STAT /HELLO.TXT cluster=3 size=40 readonly=1' \
  'DPCLI:STAT /INDEX.HTM cluster=4 size=110 readonly=1' \
  'DPMK:FS-NEGATIVE-OK:UNSUPPORTED-WRITE' \
  'fat32_artifact_reproducibility_summary_status=pass' \
  'fat32_artifact_reproducibility_default_writable=false' \
  'markers_found=true' \
  'seen_raw_tx=true' \
  'readonly_appliance_release_summary_status=pass' \
  'readonly_appliance_release_smoke_ok=true' \
  'readonly_appliance_release_cli_read_ok=true' \
  'readonly_appliance_release_http_read_ok=true' \
  'readonly_appliance_release_default_write_rejected_ok=true' \
  'readonly_appliance_release_tls_deferred=true' \
  'readonly_appliance_release_tls=false' \
  'readonly_appliance_release_https=false' \
  'readonly_appliance_release_security_claim=false' \
  'readonly_appliance_release_hardware_readiness=false' \
  'readonly_appliance_release_default_writable=false' \
  'readonly_appliance_release_write_feature_used=false' \
  'readonly_appliance_release_benchmark_result=false' \
  'readonly_appliance_release_fat32_repro_summary=$fat32_repro_summary' \
  'readonly_appliance_release_network_pcap_sha256=$(sha256_file "$network_pcap")' \
  'readonly_appliance_release_network_pcap_ok=true' \
  'readonly_appliance_release_serial_markers_ok=true' \
  'readonly_appliance_release_budget_ledger_ok=true' \
  'readonly_appliance_release_protocol_bounds_ledger_ok=true' \
  'readonly_appliance_release_timer_ledger_ok=true' \
  'readonly_appliance_release_mailbox_ledger_ok=true' \
  'readonly_appliance_release_fat32_reproducibility_ok=true' \
  'readonly_appliance_release_fault_containment_ok=true'; do
  require_literal "$release" "$literal" "release script must preserve literal: $literal"
done

require_literal "$makefile" 'x86_64-microkernel-readonly-appliance-release-packet-contract:' \
  "Makefile must expose read-only appliance release contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_readonly_appliance_release_contract.sh' \
  "Makefile must run read-only appliance release guard"
require_literal "$makefile" 'x86_64-microkernel-readonly-appliance-release-packet:' \
  "Makefile must expose read-only appliance release target"
require_literal "$makefile" './tools/x86_64_microkernel_readonly_appliance_release.sh' \
  "Makefile must run read-only appliance release proof"
require_literal "$plan" 'x86_64-microkernel-readonly-appliance-release-packet' \
  "plan must keep read-only appliance release packet"
require_literal "$diary" 'x86_64-microkernel-readonly-appliance-release-packet' \
  "diary must record read-only appliance release packet"

reject_regex "$release" 'security_claim=true|hardware_readiness=true|default_writable=true|benchmark_result=true|TLS.*false|https://|curl[[:space:]]+-k|strict-five|STRICT_FIVE_CALIBRATION|retune|calibration' \
  "read-only appliance release must not claim security, hardware readiness, writable storage, TLS, or benchmark evidence"

echo "x86_64 microkernel read-only appliance release contract OK"
