#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mnt_root="/home/user/mnt/dataplane"
log_dir="$mnt_root/logs"
tmp_dir="$mnt_root/tmp"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
repro_log="$log_dir/x86_64-microkernel-fat32-served-byte-repro-$run_id.repro.run.log"
curl_log="$log_dir/x86_64-microkernel-fat32-served-byte-repro-$run_id.curl.run.log"
normal_log="$log_dir/x86_64-microkernel-fat32-served-byte-repro-$run_id.normal.run.log"
summary="$log_dir/x86_64-microkernel-fat32-served-byte-repro-$run_id.summary"

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

extract_label_path_allow_empty() {
  local log_file="$1"
  local label="$2"
  local count value
  count="$(awk -F': ' -v key="$label" '$1 == key { count++ } END { print count + 0 }' "$log_file")"
  [[ "$count" -eq 1 ]] || fail "artifact label '$label' count in $log_file was $count"
  value="$(awk -F': ' -v key="$label" '$1 == key { print $2 }' "$log_file")"
  [[ -n "$value" ]] || fail "empty artifact label '$label'"
  [[ "$value" == "$log_dir"/* ]] || fail "artifact outside $log_dir: $value"
  [[ -e "$value" ]] || fail "missing artifact: $value"
  printf '%s\n' "$value"
}

summary_value() {
  local file="$1"
  local key="$2"
  local count value
  count="$(awk -F= -v key="$key" '$1 == key { count++ } END { print count + 0 }' "$file")"
  [[ "$count" -eq 1 ]] || fail "summary key '$key' count in $file was $count"
  value="$(awk -F= -v key="$key" '$1 == key { print $2 }' "$file")"
  [[ -n "$value" ]] || fail "empty summary value for $key in $file"
  printf '%s\n' "$value"
}

reject_duplicate_keys() {
  local file="$1"
  local duplicate
  duplicate="$(awk -F= '/^[A-Za-z0-9_]+=/ { print $1 }' "$file" | sort | uniq -d)"
  [[ -z "$duplicate" ]] || fail "duplicate summary key(s) in $file: $duplicate"
}

TMPDIR="$tmp_dir" ./tools/x86_64_microkernel_fat32_reproducibility.sh >"$repro_log" 2>&1
repro_summary="$(extract_label_path "$repro_log" "summary")"
reject_duplicate_keys "$repro_summary"

TMPDIR="$tmp_dir" DP_MICROKERNEL_CURL_PROOF=1 \
  ./tools/x86_64_microkernel_fat32_run.sh --curl-proof >"$curl_log" 2>&1
curl_serial_log="$(extract_label_path "$curl_log" "serial log")"
curl_get_body="$(extract_label_path "$curl_log" "curl GET body")"
curl_get_status="$(extract_label_path "$curl_log" "curl GET status")"
curl_head_body="$(extract_label_path_allow_empty "$curl_log" "curl HEAD body")"
curl_head_headers="$(extract_label_path "$curl_log" "curl HEAD headers")"
curl_head_status="$(extract_label_path "$curl_log" "curl HEAD status")"
curl_pcap="$(extract_label_path "$curl_log" "curl pcap")"
cli_hello_body="$log_dir/x86_64-microkernel-fat32-served-byte-repro-$run_id.cli-hello.body"
cli_index_body="$log_dir/x86_64-microkernel-fat32-served-byte-repro-$run_id.cli-index.body"
pcap_bytes="$(wc -c <"$curl_pcap" | tr -d ' ')"
(( pcap_bytes > 24 )) || fail "curl pcap only contains a global header"

TMPDIR="$tmp_dir" ./tools/x86_64_microkernel_fat32_run.sh >"$normal_log" 2>&1
serial_log="$(extract_label_path "$normal_log" "serial log")"

index_sha="$(summary_value "$repro_summary" "fat32_artifact_reproducibility_index_htm_sha256")"
index_bytes="$(summary_value "$repro_summary" "fat32_artifact_reproducibility_index_htm_bytes")"
hello_sha="$(summary_value "$repro_summary" "fat32_artifact_reproducibility_hello_txt_sha256")"
hello_bytes="$(summary_value "$repro_summary" "fat32_artifact_reproducibility_hello_txt_bytes")"

python3 - "$repro_summary" "$serial_log" "$curl_get_body" "$curl_get_status" "$curl_head_body" "$curl_head_headers" "$curl_head_status" "$curl_pcap" "$cli_hello_body" "$cli_index_body" <<'PY'
import hashlib
import sys
from pathlib import Path

(
    repro_summary,
    serial_log,
    get_body,
    get_status,
    head_body,
    head_headers,
    head_status,
    curl_pcap,
    cli_hello_body,
    cli_index_body,
) = map(Path, sys.argv[1:])
values = dict(line.rstrip("\n").split("=", 1) for line in repro_summary.open(encoding="ascii"))
expected_hello = b"hello from dataplane microkernel fat32\r\n"
expected_index = (
    b"<!doctype html><html><head><title>dataplane</title></head>"
    b"<body><h1>dataplane microkernel</h1></body></html>\r\n"
)

def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()

serial = serial_log.read_bytes()
body = get_body.read_bytes()
pcap = curl_pcap.read_bytes()
if get_status.read_bytes().strip() != b"200":
    raise SystemExit("curl GET status was not 200")
if head_status.read_bytes().strip() != b"200":
    raise SystemExit("curl HEAD status was not 200")
if head_body.read_bytes():
    raise SystemExit("curl HEAD unexpectedly served a body")
if b"content-length: 110" not in head_headers.read_bytes().lower():
    raise SystemExit("curl HEAD did not expose Content-Length: 110")
if body != expected_index:
    raise SystemExit("curl GET body does not match expected INDEX.HTM bytes")
if expected_hello not in serial:
    raise SystemExit("serial CLI output did not contain exact HELLO.TXT bytes")
if expected_index not in serial:
    raise SystemExit("serial CLI output did not contain exact INDEX.HTM bytes")
if expected_index not in pcap:
    raise SystemExit("curl pcap did not contain exact INDEX.HTM response body bytes")
cli_hello_body.write_bytes(expected_hello)
cli_index_body.write_bytes(expected_index)
if values.get("fat32_artifact_reproducibility_hello_txt_sha256") != sha256(expected_hello):
    raise SystemExit("HELLO.TXT image digest does not match served CLI bytes")
if values.get("fat32_artifact_reproducibility_index_htm_sha256") != sha256(expected_index):
    raise SystemExit("INDEX.HTM image digest does not match served bytes")
if values.get("fat32_artifact_reproducibility_hello_txt_bytes") != str(len(expected_hello)):
    raise SystemExit("HELLO.TXT byte count mismatch")
if values.get("fat32_artifact_reproducibility_index_htm_bytes") != str(len(expected_index)):
    raise SystemExit("INDEX.HTM byte count mismatch")
if str(len(body)) != values.get("fat32_artifact_reproducibility_index_htm_bytes"):
    raise SystemExit("curl GET byte count does not match image byte count")
PY

bad_summary="$tmp_dir/x86_64-microkernel-fat32-served-byte-repro-$run_id.bad.summary"
sed 's/fat32_artifact_reproducibility_index_htm_sha256=.*/fat32_artifact_reproducibility_index_htm_sha256=deadbeef/' "$repro_summary" >"$bad_summary"
if python3 - "$bad_summary" "$serial_log" "$curl_get_body" "$curl_get_status" "$curl_head_body" "$curl_head_headers" "$curl_head_status" <<'PY' >/dev/null 2>&1
import hashlib
import sys
from pathlib import Path

repro_summary, _serial_log, get_body, *_rest = map(Path, sys.argv[1:])
values = dict(line.rstrip("\n").split("=", 1) for line in repro_summary.open(encoding="ascii"))
body = get_body.read_bytes()
if values.get("fat32_artifact_reproducibility_index_htm_sha256") != hashlib.sha256(body).hexdigest():
    raise SystemExit("digest mismatch")
PY
then
  fail "stale served-byte digest summary was accepted"
fi

{
  echo "fat32_served_byte_reproducibility_summary_status=pass"
  echo "fat32_served_byte_reproducibility_run_id=$run_id"
  echo "fat32_served_byte_reproducibility_repro_log=$repro_log"
  echo "fat32_served_byte_reproducibility_repro_summary=$repro_summary"
  echo "fat32_served_byte_reproducibility_curl_log=$curl_log"
  echo "fat32_served_byte_reproducibility_normal_log=$normal_log"
  echo "fat32_served_byte_reproducibility_serial_log=$serial_log"
  echo "fat32_served_byte_reproducibility_curl_serial_log=$curl_serial_log"
  echo "fat32_served_byte_reproducibility_curl_get_body=$curl_get_body"
  echo "fat32_served_byte_reproducibility_curl_head_body=$curl_head_body"
  echo "fat32_served_byte_reproducibility_curl_head_headers=$curl_head_headers"
  echo "fat32_served_byte_reproducibility_curl_pcap=$curl_pcap"
  echo "fat32_served_byte_reproducibility_curl_pcap_bytes=$pcap_bytes"
  echo "fat32_served_byte_reproducibility_cli_hello_body=$cli_hello_body"
  echo "fat32_served_byte_reproducibility_cli_index_body=$cli_index_body"
  echo "fat32_served_byte_reproducibility_hello_txt_bytes=$hello_bytes"
  echo "fat32_served_byte_reproducibility_hello_txt_sha256=$hello_sha"
  echo "fat32_served_byte_reproducibility_index_htm_bytes=$index_bytes"
  echo "fat32_served_byte_reproducibility_index_htm_sha256=$index_sha"
  echo "fat32_served_byte_reproducibility_cli_hello_matches_image=true"
  echo "fat32_served_byte_reproducibility_cli_index_matches_image=true"
  echo "fat32_served_byte_reproducibility_http_get_index_matches_image=true"
  echo "fat32_served_byte_reproducibility_http_head_body_empty=true"
  echo "fat32_served_byte_reproducibility_http_head_length_matches_image=true"
  echo "fat32_served_byte_reproducibility_pcap_contains_index_body=true"
  echo "fat32_served_byte_reproducibility_stale_digest_rejected=true"
  echo "fat32_served_byte_reproducibility_default_writable=false"
  echo "fat32_served_byte_reproducibility_repair_mode=false"
  echo "fat32_served_byte_reproducibility_fsck=false"
  echo "fat32_served_byte_reproducibility_tls=false"
  echo "fat32_served_byte_reproducibility_https=false"
  echo "fat32_served_byte_reproducibility_benchmark_result=false"
} >"$summary"

reject_duplicate_keys "$summary"

echo "x86_64 microkernel FAT32 served-byte reproducibility passed."
echo "summary: $summary"
