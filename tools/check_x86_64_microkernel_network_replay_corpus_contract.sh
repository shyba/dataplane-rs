#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

runner="tools/x86_64_microkernel_network_replay_corpus.sh"
guard="tools/check_x86_64_microkernel_network_replay_corpus_contract.sh"
corpus="tools/x86_64_microkernel_network_replay_corpus.tsv"
fat32_runner="tools/x86_64_microkernel_fat32_run.sh"
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

for file in "$runner" "$guard" "$corpus" "$fat32_runner" "$makefile" "$plan" "$diary"; do
  require_file "$file"
done

python3 - "$corpus" <<'PY'
import csv
import sys
from pathlib import Path

rows = list(csv.DictReader(Path(sys.argv[1]).open(encoding="ascii"), delimiter="\t"))
required = {
    "arp_request_reply",
    "icmp_echo_reply",
    "udp_control_echo",
    "tcp_http_get",
    "tcp_http_head",
    "udp_bad_magic",
    "udp_overlong_payload",
    "udp_wrong_route",
    "tcp_duplicate_control",
    "tcp_malformed_partial",
}
seen = {row["case_id"] for row in rows}
if seen != required:
    raise SystemExit(f"unexpected case set: {sorted(seen)}")
if len(rows) != 10:
    raise SystemExit(f"expected 10 rows, got {len(rows)}")
if len(seen) != len(rows):
    raise SystemExit("duplicate case_id")
for row in rows:
    if row["expected"] not in {"pass", "reject"}:
        raise SystemExit(f"bad expected value: {row}")
PY

for literal in \
  'mnt_root="/home/user/mnt/dataplane"' \
  'DP_MICROKERNEL_CONTROL_PROTOCOL_V1_PROOF=1' \
  './tools/x86_64_microkernel_fat32_run.sh --control-protocol-v1-proof' \
  'validate_corpus "$corpus" "$serial_log" "$host_log" "$pcap"' \
  'if validate_corpus "$bad_corpus" "$serial_log" "$host_log" "$pcap"' \
  'network_replay_corpus_summary_status=pass' \
  'network_replay_corpus_rows=10' \
  'network_replay_corpus_pcap_backed=true' \
  'network_replay_corpus_host_log_backed=true' \
  'network_replay_corpus_stale_corpus_rejected=true' \
  'network_replay_corpus_tls=false' \
  'network_replay_corpus_https=false' \
  'network_replay_corpus_dns=false' \
  'network_replay_corpus_ntp=false' \
  'network_replay_corpus_generic_socket=false' \
  'network_replay_corpus_fuzzing_framework=false' \
  'network_replay_corpus_tcp_compliance_claim=false'; do
  require_literal "$runner" "$literal"
done

for literal in \
  'arp_request_reply' \
  'icmp_echo_reply' \
  'udp_control_echo' \
  'tcp_http_get' \
  'tcp_http_head' \
  'udp_bad_magic' \
  'udp_overlong_payload' \
  'udp_wrong_route' \
  'tcp_duplicate_control' \
  'tcp_malformed_partial' \
  'DPMK:NET-ARP-REPLY' \
  'DPMK:NET-ICMP-REPLY' \
  'DPMK:NET-UDP-ECHO' \
  'DPMK:HTTP-GET-OK' \
  'DPMK:HTTP-HEAD-OK' \
  'DPMK:TCP-CTRL-DUPLICATE:1' \
  'DPMK:TCP-CTRL-PARTIAL:1' \
  '4450554450523030'; do
  require_literal "$corpus" "$literal"
done

for literal in \
  'control_protocol_v1_bad_magic_rejected="$(' \
  'control_protocol_v1_overlong_payload_rejected="$(' \
  'control_protocol_v1_wrong_route_rejected="$(' \
  '[[ "$control_protocol_v1_invalid_response_absent" == true && "$control_protocol_v1_negative_case_sent_bad_magic" == true ]] && echo true || echo false' \
  '[[ "$control_protocol_v1_invalid_response_absent" == true && "$control_protocol_v1_negative_case_sent_overlong_payload" == true ]] && echo true || echo false' \
  '[[ "$control_protocol_v1_invalid_response_absent" == true && "$control_protocol_v1_negative_case_sent_wrong_route" == true ]] && echo true || echo false' \
  'seen_arp_reply=true' \
  'seen_icmp_reply=true' \
  'seen_udp_echo=true' \
  'http_get_index_ok=true' \
  'http_head_index_ok=true'; do
  require_literal "$fat32_runner" "$literal"
done

require_literal "$makefile" '.PHONY: x86_64-microkernel-network-replay-corpus-contract x86_64-microkernel-network-replay-corpus'
require_literal "$makefile" 'x86_64-microkernel-network-replay-corpus-contract:'
require_literal "$makefile" './tools/check_x86_64_microkernel_network_replay_corpus_contract.sh'
require_literal "$makefile" 'x86_64-microkernel-network-replay-corpus: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-control-protocol-v1-contract x86_64-microkernel-network-replay-corpus-contract'
require_literal "$makefile" './tools/x86_64_microkernel_network_replay_corpus.sh'
require_literal "$plan" '`x86_64-microkernel-network-replay-corpus`'
require_literal "$diary" '`x86_64-microkernel-network-replay-corpus`'

if rg -n -- 'TLS|HTTPS|DNS|NTP|generic socket|fuzzing framework|TCP compliance|benchmark' "$corpus"; then
  fail "network replay corpus must not contain stop-line claims"
fi

echo "x86_64 microkernel network replay corpus contract OK"
