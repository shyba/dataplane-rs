#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

kernel="crates/dataplane-x86_64-microkernel-smoke/src/kernel.rs"
# Protocol implementations are split across this module directory.
network_task="crates/dataplane-x86_64-microkernel-smoke/src/network_task"
scenarios="crates/dataplane-x86_64-microkernel-smoke/src/scenarios"
runner="tools/x86_64_microkernel_fat32_run.sh"
matrix="tools/x86_64_microkernel_validation_matrix_run.sh"
makefile="Makefile"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_file() {
  # A Rust module directory is scanned recursively by rg.
  [[ -f "$1" || -d "$1" ]] || fail "missing required source: $1"
}

require_literal() {
  local file="$1"
  local needle="$2"
  local note="$3"
  require_file "$file"
  rg -q --fixed-strings -- "$needle" "$file" || fail "$note: missing '$needle' in $file"
}

reject_regex() {
  local file="$1"
  local pattern="$2"
  local note="$3"
  require_file "$file"
  if rg -n --pcre2 -- "$pattern" "$file"; then
    fail "$note"
  fi
}

extract_mode_block() {
  local file="$1"
  local start_pattern="$2"
  awk -v start="$start_pattern" '
    index($0, start) { in_block = 1 }
    in_block { print }
    in_block && /^fi$/ { exit }
  ' "$file"
}

echo "=== x86_64 Microkernel Non-TLS Network Negative Matrix Contract Guard ==="

for file in "$kernel" "$network_task" "$scenarios" "$runner" "$matrix" "$makefile"; do
  require_file "$file"
done

require_literal "$network_task" 'unsupported_ethertype: u32' \
  "guest must count unsupported ethertypes explicitly"
require_literal "$network_task" 'arp_malformed: u32' \
  "guest must count malformed ARP inputs explicitly"
require_literal "$network_task" 'icmp_malformed: u32' \
  "guest must count malformed ICMP inputs explicitly"
require_literal "$network_task" 'network_drop_counters.unsupported_ethertype += 1' \
  "TcpIpTask must own unsupported-ethertype accounting"
require_literal "$network_task" 'src != crate::layout::VM_MAC && (dst == crate::layout::VM_MAC || dst == [0xff; 6])' \
  "unsupported-ethertype accounting must ignore the guest's own raw probe frames"
require_literal "$network_task" 'network_drop_counters.arp_malformed += 1' \
  "TcpIpTask must own malformed-ARP accounting"
require_literal "$network_task" 'network_drop_counters.icmp_malformed += 1' \
  "TcpIpTask must own malformed-ICMP accounting"
require_literal "$network_task" 'fn ethernet_frame_view(frame: &[u8]) -> Option<&[u8]>' \
  "guest must keep Ethernet frame admission explicit for unsupported ethertype accounting"
require_literal "$network_task" 'return Some(frame);' \
  "guest Ethernet frame view must pass full Ethernet frames to TcpIpTask policy"
require_literal "$network_task" 'build_unsupported_ethertype_frame' \
  "static guest proof must include unsupported ethertype input"
require_literal "$network_task" 'build_arp_malformed_frame' \
  "static guest proof must include malformed ARP input"
require_literal "$network_task" 'build_icmp_malformed_frame' \
  "static guest proof must include malformed ICMP input"
require_literal "$scenarios" 'DPMK:NONTLS-NEG-UNSUPPORTED-ETHERTYPE-OK' \
  "guest must emit unsupported-ethertype negative marker"
require_literal "$scenarios" 'DPMK:NONTLS-NEG-ARP-MALFORMED-OK' \
  "guest must emit malformed-ARP negative marker"
require_literal "$scenarios" 'DPMK:NONTLS-NEG-ICMP-MALFORMED-OK' \
  "guest must emit malformed-ICMP negative marker"
require_literal "$scenarios" 'DPMK:NONTLS-NETWORK-NEGATIVE-MATRIX-OK' \
  "guest must emit matrix completion marker"

require_literal "$scenarios" 'fn emit_nontls_negative_matrix_markers()' \
  "guest must emit the static non-TLS negative marker block from executable code"
require_literal "$scenarios" 'serial::write_str("DPMK:NONTLS-NEG-UNSUPPORTED-ETHERTYPE-OK\n");' \
  "guest static negative marker emission must be executable"
require_literal "$scenarios" 'serial::write_str("DPMK:NONTLS-NEG-ARP-MALFORMED-OK\n");' \
  "guest static negative marker emission must be executable"
require_literal "$scenarios" 'serial::write_str("DPMK:NONTLS-NEG-ICMP-MALFORMED-OK\n");' \
  "guest static negative marker emission must be executable"
require_literal "$scenarios" 'if tcpip_task.network_drop_counters().arp_malformed == 0' \
  "guest static drop-policy verification must inspect real counters"
require_literal "$scenarios" 'return Err("static-network-drop-policy");' \
  "guest static drop-policy verification must fail closed"

negative_mode_block="$(extract_mode_block "$runner" 'if [[ "$mode" == "--nontls-network-negative-matrix-proof" ]]; then')"
[[ -n "$negative_mode_block" ]] || fail "unable to extract non-TLS negative matrix mode block from runner"

for literal in \
  'nontls_network_negative_matrix_summary="$log_dir/x86_64-microkernel-fat32-$run_id.nontls-network-negative-matrix.summary"' \
  'nontls_network_negative_matrix_summary_status=pass' \
  'nontls_network_negative_matrix_tls_deferred=true' \
  'negative_network_inputs_sent=true' \
  'nontls_network_negative_matrix_static_markers_ok=true' \
  'nontls_network_negative_matrix_artifact_label=negative-matrix'; do
  printf '%s\n' "$negative_mode_block" | grep -Fq -- "$literal" || fail "runner must expose non-TLS negative matrix evidence: missing '$literal'"
done

printf '%s\n' "$negative_mode_block" | grep -Eq 'nontls_network_negative_matrix_(arp_malformed_dropped_ok|unsupported_ethertype_dropped_ok|icmp_malformed_length_dropped_ok|udp_wrong_port_no_reply_ok|udp_bad_payload_no_reply_ok|udp_malformed_length_dropped_ok|tcp_reset_before_completion_no_response_ok|tcp_fin_during_response_fin_ack_ok|tcp_duplicate_payload_single_response_ok|tcp_partial_request_no_response_ok|tcp_backpressure_bounded_ok|tcp_session_exhaustion_no_slot_reuse_ok|mixed_work_ok|drop_counters_ok|pcap_ok)=true' && fail "runner must not advertise unsupported TCP/DHCP/live-drop claims in the non-TLS negative packet"
printf '%s\n' "$negative_mode_block" | grep -Eq 'seen_(arp_reply|icmp_reply|udp_echo)=true' && fail "runner must not claim live reply evidence that the current packet does not prove"
printf '%s\n' "$negative_mode_block" | grep -Eq 'negative_done =|negative_enabled =' && fail "runner must not depend on placeholder negative-loop comments"

for literal in \
  'nontls-network-negative-matrix' \
  'make x86_64-microkernel-nontls-network-negative-matrix' \
  'x86_64 microkernel non-TLS network negative matrix passed.' \
  'non-TLS network negative summary|network host log|network pcap|serial log|qemu log'; do
  require_literal "$matrix" "$literal" "validation matrix must include non-TLS negative scenario"
done

require_literal "$makefile" 'x86_64-microkernel-nontls-network-negative-matrix-contract:' \
  "Makefile must expose the non-TLS negative contract"
require_literal "$makefile" 'x86_64-microkernel-nontls-network-negative-matrix:' \
  "Makefile must expose the non-TLS negative proof target"
require_literal "$makefile" 'DP_MICROKERNEL_NONTLS_NETWORK_NEGATIVE_MATRIX_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --nontls-network-negative-matrix-proof' \
  "Makefile proof target must run the focused runner mode"

reject_regex "$runner" 'curl[[:space:]]+-k|openssl|rustls|embedded-tls|webpki|ring::|aws-lc-rs|--tls-proof|https://' \
  "runner reopened TLS/HTTPS in the non-TLS negative packet"
reject_regex "$network_task" 'rustls|embedded-tls|webpki|ring::|aws-lc-rs|DPMK:TLS|HTTPS' \
  "guest reopened TLS/HTTPS in the non-TLS negative packet"
reject_regex "$makefile" 'STRICT_FIVE_CALIBRATION|retune|calibration' \
  "packet must not retune benchmark gates"
reject_regex "$network_task" 'TCP compliant|TCP compliance|generic socket API|listen socket|accept socket' \
  "packet must not claim TCP compliance or add generic sockets"
printf '%s\n' "$negative_mode_block" | grep -Eq 'bounded-tcp-negative|dhcp-proof|DHCP proof|tcp_reset_before_completion|tcp_fin_during_response|tcp_duplicate_payload|tcp_partial_request|tcp_backpressure|tcp_session_exhaustion' && fail "runner must not widen the non-TLS negative packet into the separate bounded TCP or DHCP proofs"

echo "x86_64 microkernel non-TLS network negative matrix contract OK"
