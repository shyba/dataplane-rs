#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
matrix_runner="tools/x86_64_microkernel_validation_matrix_run.sh"
matrix_guard="tools/check_x86_64_microkernel_validation_matrix_contract.sh"
nontls_service_guard="tools/check_x86_64_microkernel_nontls_network_service_contract.sh"
nontls_negative_guard="tools/check_x86_64_microkernel_nontls_network_negative_matrix_contract.sh"
makefile="Makefile"
frontier="aidocs/050_microkernel_robust_design_frontier_2026-05-30.md"
plan="aidocs/051_microkernel_pre_tls_appliance_plan_2026-05-30.md"

require_file() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    echo "missing required file: $path" >&2
    exit 1
  fi
}

require_literal() {
  local path="$1"
  local literal="$2"
  local message="$3"
  if ! rg -Fq -- "$literal" "$path"; then
    echo "$message" >&2
    echo "missing literal in $path: $literal" >&2
    exit 1
  fi
}

reject_regex() {
  local path="$1"
  local regex="$2"
  local message="$3"
  local tmp
  tmp="$(mktemp)"
  set +e
  rg -n -- "$regex" "$path" >"$tmp" 2>&1
  local status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    echo "$message" >&2
    cat "$tmp" >&2
    rm -f "$tmp"
    exit 1
  fi
  if [[ "$status" -ne 1 ]]; then
    echo "rg failed while scanning $path for $regex" >&2
    cat "$tmp" >&2
    rm -f "$tmp"
    exit 1
  fi
  rm -f "$tmp"
}

echo "=== x86_64 Microkernel Network Control Plane Slice Contract Guard ==="

for path in \
  "$main" \
  "$runner" \
  "$matrix_runner" \
  "$matrix_guard" \
  "$nontls_service_guard" \
  "$nontls_negative_guard" \
  "$makefile" \
  "$frontier" \
  "$plan"; do
  require_file "$path"
done

require_literal "$plan" "x86_64-microkernel-network-control-plane-slice" \
  "pre-TLS plan must keep the active network control-plane packet"
require_literal "$plan" "DHCP bad transaction ID rejection" \
  "pre-TLS plan must keep the DHCP rejection requirement"
require_literal "$plan" "prove network pressure does not starve CLI, timer, FAT32, HTTP, status, or" \
  "pre-TLS plan must keep the mixed-load non-starvation requirement"

require_literal "$frontier" "x86_64-microkernel-network-control-plane-slice" \
  "frontier must name the active network control-plane packet"

for literal in \
  "DPMK:DHCP-CONTROL-OFFER-OK" \
  "DPMK:DHCP-CONTROL-REQUEST-OK" \
  "DPMK:DHCP-CONTROL-ACK-OK" \
  "DPMK:DHCP-CONTROL-BAD-XID-DROP-OK" \
  "DPMK:NET-CTRL-TCP-OK" \
  "DPMK:NETWORK-CONTROL-PLANE-SLICE-OK" \
  "network_control_plane_slice_summary" \
  "network_control_plane_slice_summary_status=pass" \
  "network_control_plane_slice_arp_positive_ok=true" \
  "network_control_plane_slice_icmp_positive_ok=true" \
  "network_control_plane_slice_udp_positive_ok=true" \
  "network_control_plane_slice_dhcp_offer_ok=true" \
  "network_control_plane_slice_dhcp_request_ok=true" \
  "network_control_plane_slice_dhcp_ack_ok=true" \
  "network_control_plane_slice_dhcp_bad_xid_rejected_ok=true" \
  "network_control_plane_slice_tcp_control_ok=true" \
  "network_control_plane_slice_selected_drop_counters_ok=true" \
  "network_control_plane_slice_pcap_ok=true" \
  "network_control_plane_slice_mixed_load_non_starvation_ok=true" \
  "network_control_plane_slice_status_counter_agreement_ok=true" \
  "network_control_plane_slice_cli_not_starved_ok=true" \
  "network_control_plane_slice_timer_not_starved_ok=true" \
  "network_control_plane_slice_fat32_not_starved_ok=true" \
  "network_control_plane_slice_http_not_starved_ok=true" \
  "network_control_plane_slice_fault_reporting_not_starved_ok=true"; do
  require_literal "$runner" "$literal" \
    "runner must expose network control-plane slice literal: $literal"
done

require_literal "$runner" 'DP_MICROKERNEL_NETWORK_CONTROL_PLANE_SLICE_PROOF' \
  "runner must expose the control-plane proof env override"
require_literal "$runner" '--network-control-plane-slice-proof' \
  "runner must expose the control-plane proof mode"
require_literal "$runner" 'network control-plane slice summary: $network_control_plane_slice_summary' \
  "runner must emit the control-plane summary artifact label"
require_literal "$runner" 'network_control_plane_slice_client_log=$client_log' \
  "runner must record the client log artifact in the control-plane summary"
require_literal "$runner" 'network_control_plane_slice_network_pcap=$net_pcap' \
  "runner must record the pcap artifact in the control-plane summary"
require_literal "$runner" 'network_control_plane_slice_serial_log=$serial_log' \
  "runner must record the serial log artifact in the control-plane summary"
require_literal "$runner" 'network_control_plane_slice_qemu_log=$qemu_log' \
  "runner must record the QEMU log artifact in the control-plane summary"
require_literal "$runner" 'network_control_plane_slice_selected_drop_counters_ok=true' \
  "runner must record selected drop counter agreement"
require_literal "$runner" 'network_control_plane_slice_summary_status=pass' \
  "runner must mark the control-plane summary as passing"

for literal in \
  "network-control-plane-slice" \
  "make x86_64-microkernel-network-control-plane-slice" \
  "x86_64 microkernel network control-plane slice proof passed." \
  "network control-plane slice summary|network host log|network pcap|serial log|qemu log"; do
  require_literal "$matrix_runner" "$literal" \
    "validation matrix must include the network control-plane slice scenario: $literal"
  require_literal "$matrix_guard" "$literal" \
    "validation matrix guard must preserve the network control-plane slice scenario: $literal"
done

require_literal "$makefile" 'x86_64-microkernel-network-control-plane-slice-contract:' \
  "Makefile must expose the network control-plane slice contract"
require_literal "$makefile" 'x86_64-microkernel-network-control-plane-slice:' \
  "Makefile must expose the network control-plane slice proof target"
require_literal "$makefile" 'DP_MICROKERNEL_NETWORK_CONTROL_PLANE_SLICE_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --network-control-plane-slice-proof' \
  "Makefile must run the network control-plane slice proof through the FAT32 runner"

for literal in \
  'DPMK:NET-DROP-POLICY-OK' \
  'DPMK:NET-DROP-ARP-WRONG-TARGET:' \
  'DPMK:NET-DROP-ARP-MALFORMED:' \
  'DPMK:NET-DROP-UNSUPPORTED-ETHERTYPE:' \
  'DPMK:NET-DROP-IPV4-WRONG-TARGET:' \
  'DPMK:NET-DROP-UNSUPPORTED-PROTO:' \
  'DPMK:NET-DROP-ICMP-NON-ECHO:' \
  'DPMK:NET-DROP-ICMP-MALFORMED:' \
  'DPMK:NET-DROP-UDP-WRONG-PORT:' \
  'DPMK:NET-DROP-UDP-BAD-PAYLOAD:' \
  'DPMK:NET-DROP-UDP-MALFORMED:'; do
  require_literal "$nontls_service_guard" "$literal" \
    "existing non-TLS service guard must preserve drop-policy coverage: $literal"
done

for literal in \
  'DPMK:NONTLS-NETWORK-NEGATIVE-MATRIX-OK' \
  'nontls_network_negative_matrix_summary_status=pass' \
  'nontls_network_negative_matrix_pcap_ok=true' \
  'nontls_network_negative_matrix_drop_counters_ok=true' \
  'nontls_network_negative_matrix_mixed_work_ok=true'; do
  require_literal "$nontls_negative_guard" "$literal" \
    "existing non-TLS negative guard must remain intact: $literal"
done

for path in "$runner" "$matrix_runner" "$main"; do
  reject_regex "$path" 'DNS|NTP|service[[:space:]]+discovery|mDNS|LLMNR' \
    "network control-plane work must not drift into discovery protocols in $path"
  reject_regex "$path" 'generic[[:space:]]+socket[[:space:]]+api|socket[[:space:]]+API|listen[[:space:]]+socket|accept[[:space:]]+socket' \
    "network control-plane work must not introduce a generic socket API in $path"
  reject_regex "$path" 'full[[:space:]]+TCP[[:space:]]+compliance|TCP[[:space:]]+compliance|TCP compliant' \
    "network control-plane work must not claim full TCP compliance in $path"
  reject_regex "$path" 'curl[[:space:]]+-k|https://|rustls|embedded-tls|webpki|ring::|aws-lc-rs|openssl|crypto-provider|DP_MICROKERNEL_TLS_PROOF=|mode="?--tls-proof"?' \
    "network control-plane work must remain non-TLS in $path"
  reject_regex "$path" 'STRICT_FIVE_CALIBRATION|benchmark[[:space:]]+tuning|calibration' \
    "network control-plane work must not retune benchmarks in $path"
done

echo "x86_64 microkernel network control-plane slice contract guard passed."
