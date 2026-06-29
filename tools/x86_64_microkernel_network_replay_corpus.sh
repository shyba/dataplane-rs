#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mnt_root="/home/user/mnt/dataplane"
log_dir="$mnt_root/logs"
tmp_dir="$mnt_root/tmp"
corpus="tools/x86_64_microkernel_network_replay_corpus.tsv"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
run_log="$log_dir/x86_64-microkernel-network-replay-corpus-$run_id.control.run.log"
summary="$log_dir/x86_64-microkernel-network-replay-corpus-$run_id.summary"

mkdir -p "$log_dir" "$tmp_dir"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

extract_label_path() {
  local log_file="$1"
  local label="$2"
  local count value
  count="$(awk -F': ' -v key="$label" '$1 == key { count++ } END { print count + 0 }' "$log_file")"
  [[ "$count" -eq 1 ]] || fail "artifact label '$label' count in $log_file was $count"
  value="$(awk -F': ' -v key="$label" '$1 == key { print $2 }' "$log_file")"
  [[ -n "$value" ]] || fail "empty artifact label '$label'"
  [[ "$value" == "$log_dir"/* ]] || fail "artifact outside $log_dir: $value"
  [[ -s "$value" ]] || fail "missing or empty artifact: $value"
  printf '%s\n' "$value"
}

validate_corpus() {
  local corpus_file="$1"
  local serial_log="$2"
  local host_log="$3"
  local pcap="$4"
  python3 - "$corpus_file" "$serial_log" "$host_log" "$pcap" <<'PY'
import csv
import sys
from pathlib import Path

corpus_path, serial_path, host_path, pcap_path = map(Path, sys.argv[1:])
rows = list(csv.DictReader(corpus_path.open(encoding="ascii"), delimiter="\t"))
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
    raise SystemExit(f"network replay corpus case set drifted: {sorted(seen)}")
if len(rows) != len(seen):
    raise SystemExit("network replay corpus has duplicate case_id")

serial = serial_path.read_text(encoding="ascii", errors="strict")
host = host_path.read_text(encoding="ascii", errors="strict")
pcap = pcap_path.read_bytes()
if len(pcap) <= 24:
    raise SystemExit("pcap only contains a global header")

for row in rows:
    if row["expected"] not in {"pass", "reject"}:
        raise SystemExit(f"{row['case_id']}: invalid expected value {row['expected']}")
    marker = row["serial_marker"]
    if marker != "-" and marker not in serial:
        raise SystemExit(f"{row['case_id']}: missing serial marker {marker}")
    artifact_key = row["artifact_key"]
    if artifact_key != "-" and artifact_key not in host:
        raise SystemExit(f"{row['case_id']}: missing host artifact key {artifact_key}")
    pcap_hex = row["pcap_hex"]
    if pcap_hex != "-":
        try:
            needle = bytes.fromhex(pcap_hex)
        except ValueError as exc:
            raise SystemExit(f"{row['case_id']}: invalid pcap hex {pcap_hex}") from exc
        if needle not in pcap:
            raise SystemExit(f"{row['case_id']}: missing pcap hex {pcap_hex}")

for forbidden in (
    "TLS",
    "HTTPS",
    "DNS",
    "NTP",
    "generic socket",
    "fuzzing framework",
    "TCP compliance",
):
    if forbidden in corpus_path.read_text(encoding="ascii"):
        raise SystemExit(f"network replay corpus must not overclaim: {forbidden}")
PY
}

reject_duplicate_keys() {
  local file="$1"
  local duplicate
  duplicate="$(awk -F= '/^[A-Za-z0-9_]+=/ { print $1 }' "$file" | sort | uniq -d)"
  [[ -z "$duplicate" ]] || fail "duplicate summary key(s) in $file: $duplicate"
}

TMPDIR="$tmp_dir" DP_MICROKERNEL_CONTROL_PROTOCOL_V1_PROOF=1 \
  ./tools/x86_64_microkernel_fat32_run.sh --control-protocol-v1-proof >"$run_log" 2>&1

serial_log="$(extract_label_path "$run_log" "serial log")"
host_log="$(extract_label_path "$run_log" "network host log")"
pcap="$(extract_label_path "$run_log" "network pcap")"
control_summary="$(extract_label_path "$run_log" "control protocol v1 summary")"
pcap_bytes="$(wc -c <"$pcap" | tr -d ' ')"
(( pcap_bytes > 24 )) || fail "network replay pcap only contains a global header"

validate_corpus "$corpus" "$serial_log" "$host_log" "$pcap"

bad_corpus="$tmp_dir/x86_64-microkernel-network-replay-corpus-$run_id.bad.tsv"
sed 's/4450554450523030/deadbeefcafebabe/' "$corpus" >"$bad_corpus"
if validate_corpus "$bad_corpus" "$serial_log" "$host_log" "$pcap" >/dev/null 2>&1; then
  fail "corrupt replay corpus was accepted"
fi

{
  echo "network_replay_corpus_summary_status=pass"
  echo "network_replay_corpus_run_id=$run_id"
  echo "network_replay_corpus_file=$corpus"
  echo "network_replay_corpus_rows=10"
  echo "network_replay_corpus_run_log=$run_log"
  echo "network_replay_corpus_serial_log=$serial_log"
  echo "network_replay_corpus_host_log=$host_log"
  echo "network_replay_corpus_pcap=$pcap"
  echo "network_replay_corpus_pcap_bytes=$pcap_bytes"
  echo "network_replay_corpus_control_summary=$control_summary"
  echo "network_replay_corpus_arp_case=true"
  echo "network_replay_corpus_icmp_case=true"
  echo "network_replay_corpus_udp_control_case=true"
  echo "network_replay_corpus_http_cases=true"
  echo "network_replay_corpus_udp_negative_cases=true"
  echo "network_replay_corpus_tcp_negative_markers=true"
  echo "network_replay_corpus_pcap_backed=true"
  echo "network_replay_corpus_host_log_backed=true"
  echo "network_replay_corpus_stale_corpus_rejected=true"
  echo "network_replay_corpus_tls=false"
  echo "network_replay_corpus_https=false"
  echo "network_replay_corpus_dns=false"
  echo "network_replay_corpus_ntp=false"
  echo "network_replay_corpus_generic_socket=false"
  echo "network_replay_corpus_fuzzing_framework=false"
  echo "network_replay_corpus_tcp_compliance_claim=false"
  echo "network_replay_corpus_benchmark_result=false"
} >"$summary"

reject_duplicate_keys "$summary"

echo "x86_64 microkernel network replay corpus proof passed."
echo "summary: $summary"
