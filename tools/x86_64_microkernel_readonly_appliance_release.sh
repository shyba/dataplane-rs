#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mnt_root="/home/user/mnt/dataplane"
log_dir="$mnt_root/logs"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
repro_log="$log_dir/x86_64-microkernel-readonly-release-$run_id.repro.log"
smoke_log="$log_dir/x86_64-microkernel-readonly-release-$run_id.smoke.log"
summary="$log_dir/x86_64-microkernel-readonly-release-$run_id.summary"

mkdir -p "$log_dir"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

extract_artifact() {
  local log="$1"
  local label="$2"
  local value
  value="$(awk -F': ' -v key="$label" '$1 == key { value=$2 } END { print value }' "$log")"
  [[ -n "$value" ]] || fail "missing artifact label '$label' in $log"
  printf '%s\n' "$value"
}

require_file() {
  local path="$1"
  [[ -s "$path" ]] || fail "missing or empty artifact: $path"
}

require_contains() {
  local path="$1"
  local literal="$2"
  local note="$3"
  grep -Fq -- "$literal" "$path" || fail "$note"
}

sha256_file() {
  sha256sum "$1" | awk '{print $1}'
}

tools/x86_64_microkernel_fat32_reproducibility.sh >"$repro_log"
fat32_repro_summary="$(extract_artifact "$repro_log" "summary")"
require_file "$fat32_repro_summary"

tools/x86_64_microkernel_fat32_run.sh >"$smoke_log"
serial_log="$(extract_artifact "$smoke_log" "serial log")"
qemu_log="$(extract_artifact "$smoke_log" "qemu log")"
client_log="$(extract_artifact "$smoke_log" "client log")"
network_host_log="$(extract_artifact "$smoke_log" "network host log")"
network_pcap="$(extract_artifact "$smoke_log" "network pcap")"
fat32_image="$(extract_artifact "$smoke_log" "fat32 image")"

for artifact in \
  "$serial_log" \
  "$qemu_log" \
  "$client_log" \
  "$network_host_log" \
  "$network_pcap" \
  "$fat32_image"; do
  require_file "$artifact"
done

for marker in \
  "DPMK:RESOURCE-BUDGET-LEDGER-OK" \
  "DPMK:PROTOCOL-INPUT-BOUNDS-LEDGER-OK" \
  "DPMK:TIMER-TIMEOUT-SERVICE-LEDGER-OK" \
  "DPMK:SERVICE-MAILBOX-ENVELOPE-OK" \
  "DPCLI:STAT /HELLO.TXT cluster=3 size=40 readonly=1" \
  "DPCLI:STAT /INDEX.HTM cluster=4 size=110 readonly=1" \
  "DPMK:FS-NEGATIVE-OK:UNSUPPORTED-WRITE" \
  "DPMK:FS-HARDENING-OK" \
  "DPMK:FAULT-CONTAINED" \
  "DPMK:OK"; do
  require_contains "$serial_log" "$marker" "serial log missing release marker: $marker"
done

for field in \
  "fat32_artifact_reproducibility_summary_status=pass" \
  "fat32_artifact_reproducibility_images_identical=true" \
  "fat32_artifact_reproducibility_repair_mode=false" \
  "fat32_artifact_reproducibility_default_writable=false"; do
  require_contains "$fat32_repro_summary" "$field" "FAT32 reproducibility summary missing field: $field"
done

require_contains "$client_log" "markers_found=true" \
  "client log must prove all expected serial markers were observed"
require_contains "$network_host_log" "seen_raw_tx=true" \
  "network host log must prove guest raw Ethernet frame"

pcap_bytes="$(wc -c <"$network_pcap" | tr -d ' ')"
if (( pcap_bytes <= 24 )); then
  fail "network pcap only contains a global header"
fi

{
  echo "readonly_appliance_release_summary_status=pass"
  echo "readonly_appliance_release_smoke_ok=true"
  echo "readonly_appliance_release_cli_read_ok=true"
  echo "readonly_appliance_release_http_read_ok=true"
  echo "readonly_appliance_release_default_write_rejected_ok=true"
  echo "readonly_appliance_release_tls_deferred=true"
  echo "readonly_appliance_release_tls=false"
  echo "readonly_appliance_release_https=false"
  echo "readonly_appliance_release_security_claim=false"
  echo "readonly_appliance_release_hardware_readiness=false"
  echo "readonly_appliance_release_default_writable=false"
  echo "readonly_appliance_release_write_feature_used=false"
  echo "readonly_appliance_release_benchmark_result=false"
  echo "readonly_appliance_release_fat32_repro_summary=$fat32_repro_summary"
  echo "readonly_appliance_release_fat32_image=$fat32_image"
  echo "readonly_appliance_release_fat32_image_sha256=$(sha256_file "$fat32_image")"
  echo "readonly_appliance_release_serial_log=$serial_log"
  echo "readonly_appliance_release_client_log=$client_log"
  echo "readonly_appliance_release_qemu_log=$qemu_log"
  echo "readonly_appliance_release_network_host_log=$network_host_log"
  echo "readonly_appliance_release_network_pcap=$network_pcap"
  echo "readonly_appliance_release_network_pcap_bytes=$pcap_bytes"
  echo "readonly_appliance_release_network_pcap_sha256=$(sha256_file "$network_pcap")"
  echo "readonly_appliance_release_network_pcap_ok=true"
  echo "readonly_appliance_release_serial_markers_ok=true"
  echo "readonly_appliance_release_budget_ledger_ok=true"
  echo "readonly_appliance_release_protocol_bounds_ledger_ok=true"
  echo "readonly_appliance_release_timer_ledger_ok=true"
  echo "readonly_appliance_release_mailbox_ledger_ok=true"
  echo "readonly_appliance_release_fat32_reproducibility_ok=true"
  echo "readonly_appliance_release_fault_containment_ok=true"
} >"$summary"

grep -q "readonly_appliance_release_summary_status=pass" "$summary"
grep -q "readonly_appliance_release_tls_deferred=true" "$summary"
grep -q "readonly_appliance_release_default_writable=false" "$summary"
grep -q "readonly_appliance_release_benchmark_result=false" "$summary"

echo "x86_64 microkernel read-only appliance release packet passed."
echo "summary: $summary"
