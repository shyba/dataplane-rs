#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

kernel="crates/dataplane-x86_64-microkernel-smoke/src/kernel.rs"
network_task="crates/dataplane-x86_64-microkernel-smoke/src/network_task.rs"
layout="crates/dataplane-x86_64-microkernel-smoke/src/layout.rs"
control_protocol="crates/dataplane-x86_64-microkernel-smoke/src/control_protocol.rs"
scenarios="crates/dataplane-x86_64-microkernel-smoke/src/scenarios.rs"
arch="crates/dataplane-x86_64-microkernel-smoke/src/arch.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
makefile="Makefile"
fail_closed_guard="tools/check_x86_64_microkernel_runner_fail_closed_expansion_contract.sh"
tasks="changes/__archived_changes_2026-05-31/x86_64-microkernel-control-protocol-v1/tasks.md"
frontier="aidocs/050_microkernel_robust_design_frontier_2026-05-30.md"
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

reject_protocol_scoped_regex() {
  local file="$1"
  local regex="$2"
  local note="$3"
  local scoped matches status
  set +e
  scoped="$(rg -n -- 'control_protocol_v1|control-protocol-v1|CONTROL_PROTOCOL_V1|x86_64-microkernel-control-protocol-v1' "$file")"
  status=$?
  set -e
  [[ "$status" -eq 0 || "$status" -eq 1 ]] || fail "could not scan $file for control-protocol-v1 scope"
  [[ -n "$scoped" ]] || return 0

  set +e
  matches="$(printf '%s\n' "$scoped" | rg -- "$regex")"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    printf '%s\n' "$matches"
    fail "$note"
  fi
  [[ "$status" -eq 1 ]] || fail "could not scan $file for forbidden control-protocol-v1 regex: $regex"
}

for file in \
  "$kernel" \
  "$network_task" \
  "$layout" \
  "$control_protocol" \
  "$scenarios" \
  "$arch" \
  "$runner" \
  "$makefile" \
  "$fail_closed_guard" \
  "$frontier" \
  "$plan" \
  "$diary"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel Control Protocol V1 Contract Guard ==="

for literal in \
  'const UDP_ECHO_PAYLOAD_BYTES: usize = 16;' \
  'const IPV4_UDP_REPLY_BYTES: usize = IPV4_HEADER_BYTES + 8 + UDP_ECHO_PAYLOAD_BYTES;' \
  'fn prepare_udp_reply(' \
  'let protocol = CONTROL_PROTOCOL_V1_ROUTE;' \
  'reject_control_protocol_v1(ControlProtocolV1RejectReason::ShortPayload);' \
  'reject_control_protocol_v1(ControlProtocolV1RejectReason::WrongSrcPort);' \
  'reject_control_protocol_v1(ControlProtocolV1RejectReason::WrongRoute);' \
  'reject_control_protocol_v1(ControlProtocolV1RejectReason::BadMagic);' \
  'reject_control_protocol_v1(ControlProtocolV1RejectReason::BadCheck);' \
  'let expected_udp_len = 8 + protocol.max_payload_bytes;' \
  'if udp_len < expected_udp_len {' \
  'if udp_len > expected_udp_len {' \
  'if src_port != HOST_UDP_PORT {' \
  'if dst_port != VM_UDP_PORT {' \
  'frame[udp_payload..udp_payload + protocol.request_magic.len()] != *protocol.request_magic' \
  'if check != (seq ^ 0xa5a5_a5a5)' \
  'payload[reply_payload..reply_payload + protocol.response_magic.len()]' \
  'NetworkReplyKind::Udp { seq }'; do
  require_literal "$network_task" "$literal" "network task source must contain control-protocol-v1 literal: $literal"
done

for literal in \
  'const HOST_UDP_PORT: u16 = 40_000;' \
  'const VM_UDP_PORT: u16 = 40_001;' \
  'const HTTP_SERVER_PORT: u16 = 80;' \
  'const DHCP_SERVER_PORT: u16 = 67;' \
  'const DHCP_CLIENT_PORT: u16 = 68;' \
  'const ETHER_TYPE_ARP: u16 = 0x0806;' \
  'const ETHER_TYPE_IPV4: u16 = 0x0800;' \
  'const IPV4_PROTOCOL_ICMP: u8 = 1;' \
  'const IPV4_PROTOCOL_TCP: u8 = 6;' \
  'const IPV4_PROTOCOL_UDP: u8 = 17;'; do
  require_literal "$layout" "$literal" "layout module must contain literal: $literal"
done

for literal in \
  'DPMK:NET-UDP-ECHO' \
  'DPSCHED:control-echo task=' \
  'DPMK:CTRL-V1-NET-REGION-OK' \
  'DPMK:CTRL-V1-DENIED-ROUTE-OK' \
  'DPMK:CTRL-V1-BOUNDARY-OK' \
  'fn prove_control_protocol_v1_boundary(' \
  'Err("net-request-route") => Ok(true),' \
  'DPSCHED:control-echo task=' \
  'control-echo-accounting' \
  'return Err("control-protocol-v1-net-region");' \
  'return Err("control-protocol-v1-denied-route");'; do
  require_literal "$scenarios" "$literal" "scenario boundary proof must contain literal: $literal"
done

require_literal "$arch" 'pub(crate) fn prove_net_region_fault_containment() -> bool {' \
  "arch module must own net region containment proof"

for literal in \
  'const CONTROL_PROTOCOL_V1_VERSION: u8 = 0;' \
  'const CONTROL_PROTOCOL_V1_OPCODE_STATUS_ECHO: u8 = 0;' \
  'const CONTROL_PROTOCOL_V1_MAX_PAYLOAD_BYTES: usize = UDP_ECHO_PAYLOAD_BYTES;' \
  'const CONTROL_PROTOCOL_V1_REQUEST_MAGIC: &[u8; 8] = b"DPUDPQ00";' \
  'const CONTROL_PROTOCOL_V1_RESPONSE_MAGIC: &[u8; 8] = b"DPUDPR00";' \
  'const UDP_REQUEST_MAGIC: &[u8; 8] = CONTROL_PROTOCOL_V1_REQUEST_MAGIC;' \
  'const UDP_RESPONSE_MAGIC: &[u8; 8] = CONTROL_PROTOCOL_V1_RESPONSE_MAGIC;' \
  'const CONTROL_PROTOCOL_V1_ROUTE: ControlProtocolV1Route = ControlProtocolV1Route {' \
  'pub(crate) struct ControlProtocolV1Route {' \
  'pub(crate) enum ControlProtocolV1RejectReason {' \
  'fn marker(self) -> &' \
  'fn reject_control_protocol_v1(reason: ControlProtocolV1RejectReason)' \
  'DPMK:CTRL-V1-REJECT:bad_magic' \
  'DPMK:CTRL-V1-REJECT:bad_check' \
  'DPMK:CTRL-V1-REJECT:wrong_route' \
  'DPMK:CTRL-V1-REJECT:wrong_src_port' \
  'DPMK:CTRL-V1-REJECT:short_payload' \
  'DPMK:CTRL-V1-REJECT:overlong_payload'; do
  require_literal "$control_protocol" "$literal" "control protocol module must contain literal: $literal"
done

for literal in \
  'DP_MICROKERNEL_CONTROL_PROTOCOL_V1_PROOF' \
  '--control-protocol-v1-proof' \
  'control_protocol_v1_summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "control-protocol-v1.summary")"' \
  'request_magic = b"DPUDPQ00"' \
  'response_magic = b"DPUDPR00"' \
  'host_port_udp = 40000' \
  'vm_port_udp = 40001' \
  'bad_magic_seq = 0xF0000001' \
  'overlong_seq = 0xF0000002' \
  'wrong_route_seq = 0xF0000003' \
  'bad_check_seq = 0xF0000004' \
  'short_payload_seq = 0xF0000005' \
  'wrong_src_port_seq = 0xF0000006' \
  'def udp_request_frame(seq):' \
  'def control_protocol_v1_negative_frame(seq, kind):' \
  'require_host_evidence_for_marker "DPMK:NET-UDP-ECHO" "seen_udp_echo=true"' \
  'require_pcap_evidence_for_marker "DPMK:NET-UDP-ECHO" "4450554450523030"' \
  'seen_udp_echo=true' \
  'control_protocol_v1_summary_status=pass' \
  'control_protocol_v1_seen_udp_echo=true' \
  'control_protocol_v1_pcap_bytes=' \
  'control_protocol_v1_pcap_bytes <= 24' \
  'control_protocol_v1_request_magic=DPUDPQ00' \
  'control_protocol_v1_response_magic=DPUDPR00' \
  'control_protocol_v1_invalid_response_absent=true' \
  'control_protocol_v1_negative_case_sent_bad_magic="$(' \
  'control_protocol_v1_negative_case_sent_overlong_payload="$(' \
  'control_protocol_v1_negative_case_sent_wrong_route="$(' \
  'control_protocol_v1_bad_magic_rejected="$(' \
  'control_protocol_v1_overlong_payload_rejected="$(' \
  'control_protocol_v1_wrong_route_rejected="$(' \
  '[[ "$control_protocol_v1_invalid_response_absent" == true && "$control_protocol_v1_negative_case_sent_bad_magic" == true ]] && echo true || echo false' \
  '[[ "$control_protocol_v1_invalid_response_absent" == true && "$control_protocol_v1_negative_case_sent_overlong_payload" == true ]] && echo true || echo false' \
  '[[ "$control_protocol_v1_invalid_response_absent" == true && "$control_protocol_v1_negative_case_sent_wrong_route" == true ]] && echo true || echo false' \
  'control_protocol_v1_negative_case_sent_bad_check=$(grep -q "control_protocol_v1_negative_case_sent_bad_check=true" "$host_exchange_log" && echo true || echo false)' \
  'control_protocol_v1_negative_case_sent_short_payload=$(grep -q "control_protocol_v1_negative_case_sent_short_payload=true" "$host_exchange_log" && echo true || echo false)' \
  'control_protocol_v1_negative_case_sent_wrong_src_port=$(grep -q "control_protocol_v1_negative_case_sent_wrong_src_port=true" "$host_exchange_log" && echo true || echo false)' \
  'control_protocol_v1_scheduler_accounted=$(grep -q "DPSCHED:control-echo task=6 enqueued=" "$serial_log" && echo true || echo false)' \
  'control_protocol_v1_net_region_ok=$(grep -q "DPMK:CTRL-V1-NET-REGION-OK" "$serial_log" && echo true || echo false)' \
  'control_protocol_v1_denied_route_ok=$(grep -q "DPMK:CTRL-V1-DENIED-ROUTE-OK" "$serial_log" && echo true || echo false)' \
  'control_protocol_v1_boundary_status=$(grep -q "DPMK:CTRL-V1-BOUNDARY-OK" "$serial_log" && echo pass || echo fail)' \
  'grep -q "DPMK:CTRL-V1-NET-REGION-OK" "$serial_log"' \
  'grep -q "DPMK:CTRL-V1-DENIED-ROUTE-OK" "$serial_log"' \
  'grep -q "DPMK:CTRL-V1-BOUNDARY-OK" "$serial_log"' \
  '"control_protocol_v1_net_region_ok": "true",' \
  '"control_protocol_v1_denied_route_ok": "true",' \
  '"control_protocol_v1_boundary_status": "pass",' \
  'DPSCHED:control-echo task=6 enqueued=' \
  'DPMK:CTRL-V1-REJECT:bad_magic' \
  'DPMK:CTRL-V1-REJECT:overlong_payload' \
  'DPMK:CTRL-V1-REJECT:wrong_route' \
  'DPMK:CTRL-V1-REJECT:bad_check' \
  'DPMK:CTRL-V1-REJECT:short_payload' \
  'DPMK:CTRL-V1-REJECT:wrong_src_port' \
  'control_protocol_v1_negative_case_sent_bad_check=$(grep -q "control_protocol_v1_negative_case_sent_bad_check=true" "$host_exchange_log" && echo true || echo false)' \
  'control_protocol_v1_negative_case_sent_short_payload=$(grep -q "control_protocol_v1_negative_case_sent_short_payload=true" "$host_exchange_log" && echo true || echo false)' \
  'control_protocol_v1_negative_case_sent_wrong_src_port=$(grep -q "control_protocol_v1_negative_case_sent_wrong_src_port=true" "$host_exchange_log" && echo true || echo false)' \
  'duplicate summary key: '; do
  require_literal "$runner" "$literal" "runner must contain control-protocol-v1 evidence literal: $literal"
done

for literal in \
  'missing_pcap_artifact_failed' \
  'stale_summary_run_id_failed' \
  'duplicate_summary_key_failed' \
  'command_parser_error_failed' \
  'dp_reject_duplicate_keys "$duplicate_summary"'; do
  require_literal "$fail_closed_guard" "$literal" "fail-closed guard must preserve stale/artifact rejection: $literal"
done

require_literal "$makefile" '.PHONY: x86_64-microkernel-control-protocol-v1-contract x86_64-microkernel-control-protocol-v1' \
  "Makefile must declare control-protocol-v1 phony targets"
require_literal "$makefile" 'x86_64-microkernel-control-protocol-v1-contract:' \
  "Makefile must expose control-protocol-v1 contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_control_protocol_v1_contract.sh' \
  "Makefile must run control-protocol-v1 guard"
require_literal "$makefile" 'x86_64-microkernel-control-protocol-v1: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-control-protocol-v1-contract' \
  "Makefile must wire control-protocol-v1 to the restored UDP v1 compile and local guard targets"
require_literal "$makefile" 'DP_MICROKERNEL_CONTROL_PROTOCOL_V1_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --control-protocol-v1-proof' \
  "Makefile must run the dedicated control-protocol-v1 QEMU proof"

require_literal "$runner" 'host harness uses fixed scenario plumbing and artifact validation; the guest owns protocol semantics and state transitions' \
  "runner must describe the host harness boundary truthfully"
require_literal "$tasks" 'host harness remains fixed scenario plumbing and artifact validation, not guest protocol semantics, generic protocol dispatch, TLS/HTTP/schema/control-state handling, or guest state transitions' \
  "tasks must state the fixed harness boundary"
require_literal "$frontier" '`x86_64-microkernel-control-protocol-v1`' \
  "frontier must route the control-protocol-v1 packet"
require_literal "$plan" '`x86_64-microkernel-control-protocol-v1`' \
  "plan must name the control-protocol-v1 packet"
require_literal "$plan" '## TLS Parking Lot' \
  "plan must park TLS instead of reopening it in this packet"
require_literal "$diary" '`x86_64-microkernel-control-protocol-v1`' \
  "diary must include the control-protocol-v1 round plan"
require_literal "$diary" 'Guard/Make consistency worker slice:' \
  "diary must record the guard/Make consistency worker slice"

for scanned in "$kernel" "$runner" "$makefile"; do
  reject_protocol_scoped_regex "$scanned" 'curl[[:space:]]+-k|https://|openssl|OpenSSL|rustls|embedded-tls|webpki|aws-lc-rs|crypto-provider|certificate|private[[:space:]]+key|DP_MICROKERNEL_TLS_PROOF|--tls-proof' \
    "control-protocol-v1 scoped lines must not reopen TLS/HTTPS/certificate tooling in $scanned"
  reject_protocol_scoped_regex "$scanned" 'GenericSocket|SocketApi|std::net|TcpListener|UdpSocket|DNS|Dns|NTP|Ntp|service[[:space:]_-]*discovery|OpenAPI|serde_json|serde::|production[[:space:]_-]*TCP|full[[:space:]_-]*TCP|STRICT_FIVE_CALIBRATION|run_strict_five|strict-five' \
    "control-protocol-v1 scoped lines must not broaden into sockets, DNS/NTP, schemas, production TCP, or benchmarks in $scanned"
done

for scoped in "$runner" "$tasks" "$plan"; do
  reject_protocol_scoped_regex "$scoped" 'ad hoc host logic|no host logic|generic protocol dispatch|guest protocol semantics|guest state transitions|TLS/HTTP/schema/control-state handling' \
    "control-protocol-v1 scoped lines must keep the host harness boundary fixed in $scoped"
done

echo "x86_64 microkernel control protocol v1 contract OK"
