#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mnt_root="/home/user/mnt/dataplane"
log_dir="$mnt_root/logs"
tmp_dir="$mnt_root/tmp"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
curl_run_log="$log_dir/x86_64-microkernel-fat32-directory-index-$run_id.curl.run.log"
cli_run_log="$log_dir/x86_64-microkernel-fat32-directory-index-$run_id.cli.run.log"
host_client_log="$log_dir/x86_64-microkernel-fat32-directory-index-$run_id.host-client.log"
summary="$log_dir/x86_64-microkernel-fat32-directory-index-$run_id.summary"

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
  [[ -e "$value" ]] || fail "missing artifact: $value"
  printf '%s\n' "$value"
}

require_contains() {
  local file="$1"
  local literal="$2"
  grep -Fq -- "$literal" "$file" || fail "$file missing: $literal"
}

TMPDIR="$tmp_dir" DP_MICROKERNEL_CURL_PROOF=1 \
  ./tools/x86_64_microkernel_fat32_run.sh --curl-proof >"$curl_run_log" 2>&1
curl_serial_log="$(extract_label_path "$curl_run_log" "serial log")"
curl_log="$(extract_label_path "$curl_run_log" "curl log")"
curl_get_body="$(extract_label_path "$curl_run_log" "curl GET body")"
curl_get_status="$(extract_label_path "$curl_run_log" "curl GET status")"
curl_missing_status="$(extract_label_path "$curl_run_log" "curl missing status")"
curl_overlong_status="$(extract_label_path "$curl_run_log" "curl overlong-request status")"

TMPDIR="$tmp_dir" \
  ./tools/x86_64_microkernel_fat32_run.sh >"$cli_run_log" 2>&1
cli_serial_log="$(extract_label_path "$cli_run_log" "serial log")"
cli_transcript="$cli_serial_log"
parity_summary="$cli_run_log"
parity_http_status="$curl_get_status"

python3 - \
  "$curl_serial_log" "$cli_serial_log" "$cli_transcript" "$curl_log" \
  "$curl_get_body" "$curl_get_status" "$curl_missing_status" "$curl_overlong_status" \
  "$parity_summary" "$parity_http_status" "$host_client_log" <<'PY'
import hashlib
import sys

(
    curl_serial_path,
    cli_serial_path,
    cli_transcript_path,
    curl_log_path,
    curl_get_body_path,
    curl_get_status_path,
    curl_missing_status_path,
    curl_overlong_status_path,
    parity_summary_path,
    parity_http_status_path,
    host_client_log_path,
) = sys.argv[1:]

expected_index = b"<!doctype html><html><head><title>dataplane</title></head><body><h1>dataplane microkernel</h1></body></html>\r\n"
expected_cli_ls = b"DPCLI:LS / HELLO.TXT 40 INDEX.HTM 110 LARGE.HTM 480 CHAIN.HTM 834"
expected_rows = [
    b"DPFSIDX:row 0 path=/HELLO.TXT status=ok cluster=3 size=40 readonly=1",
    b"DPFSIDX:row 1 path=/INDEX.HTM status=ok cluster=4 size=110 readonly=1",
    b"DPFSIDX:row 2 path=/LARGE.HTM status=ok cluster=6 size=480 readonly=1",
    b"DPFSIDX:row 3 path=/CHAIN.HTM status=ok cluster=7 size=834 readonly=1",
]
expected_order = b"DPFSIDX:rows count=4 cap=4 name_cap=12 response_cap=192 order=/HELLO.TXT,/INDEX.HTM,/LARGE.HTM,/CHAIN.HTM"


def require(condition, message):
    if not condition:
        raise SystemExit(message)


curl_serial = open(curl_serial_path, "rb").read()
cli_serial = open(cli_serial_path, "rb").read()
cli_transcript = open(cli_transcript_path, "rb").read()
curl_log = open(curl_log_path, "r", encoding="ascii", errors="replace").read()
parity_summary = open(parity_summary_path, "r", encoding="ascii", errors="replace").read()

for serial_name, serial in (("curl", curl_serial), ("cli", cli_serial)):
    require(b"DPMK:FS-DIR-INDEX-OK" in serial, f"{serial_name} serial missing directory-index marker")
    require(expected_order in serial, f"{serial_name} serial missing ordered row header")
    for row in expected_rows:
        require(row in serial, f"{serial_name} serial missing row {row!r}")
    require(
        b"DPFSIDX:unsupported_path=unsupported-path entry_cap=enforced name_cap=enforced response_cap=enforced deterministic=ok"
        in serial,
        f"{serial_name} serial missing cap/unsupported policy row",
    )
    require(
        b"DPFSIDX:counters list_root=2 entries=4 errors=1" in serial,
        f"{serial_name} serial missing counters",
    )

require(b"DPMK:FS-DIR-INDEX-HTTP-OK" in curl_serial, "curl serial missing HTTP directory-index marker")
require(b"DPMK:FS-DIR-INDEX-OK" in cli_transcript, "CLI serial missing directory-index marker")
require(expected_cli_ls in cli_transcript, "CLI serial missing four-row DPCLI:LS transcript")
require(open(curl_get_status_path, "rb").read().strip() == b"200", "HTTP INDEX.HTM status was not 200")
require(open(curl_missing_status_path, "rb").read().strip() == b"404", "HTTP missing status was not 404")
require(open(curl_overlong_status_path, "rb").read().strip() == b"413", "HTTP response cap status was not 413")
require(open(parity_http_status_path, "rb").read().strip() == b"200", "second-run HTTP status proxy was not 200")
require(open(curl_get_body_path, "rb").read() == expected_index, "HTTP INDEX.HTM body changed")
require("curl_get_index_status=200" in curl_log, "curl host log missing status 200")
require("curl_missing_status=404" in curl_log, "curl host log missing unsupported-path status")
require("curl_overlong_request_status=413" in curl_log, "curl host log missing response-cap status")

host_lines = [
    "fat32_directory_index_host_client_status=pass",
    "fat32_directory_index_http_status=200",
    "fat32_directory_index_http_missing_status=404",
    "fat32_directory_index_http_response_cap_status=413",
    "fat32_directory_index_http_index_sha256=" + hashlib.sha256(expected_index).hexdigest(),
    "fat32_directory_index_cli_line=DPCLI:LS / HELLO.TXT 40 INDEX.HTM 110 LARGE.HTM 480 CHAIN.HTM 834",
    "fat32_directory_index_cli_rows=HELLO.TXT,INDEX.HTM,LARGE.HTM,CHAIN.HTM",
    "fat32_directory_index_order=/HELLO.TXT,/INDEX.HTM,/LARGE.HTM,/CHAIN.HTM",
    "fat32_directory_index_entry_cap=4",
    "fat32_directory_index_name_cap=12",
    "fat32_directory_index_response_cap=192",
    "fat32_directory_index_unsupported_path=unsupported-path",
    "fat32_directory_index_http_cli_same_fs_task=true",
]
open(host_client_log_path, "w", encoding="ascii").write("\n".join(host_lines) + "\n")
PY

for literal in \
  "fat32_directory_index_host_client_status=pass" \
  "fat32_directory_index_http_status=200" \
  "fat32_directory_index_cli_rows=HELLO.TXT,INDEX.HTM,LARGE.HTM,CHAIN.HTM" \
  "fat32_directory_index_response_cap=192" \
  "fat32_directory_index_http_cli_same_fs_task=true"; do
  require_contains "$host_client_log" "$literal"
done

{
  echo "fat32_directory_index_summary_status=pass"
  echo "fat32_directory_index_run_id=$run_id"
  echo "fat32_directory_index_curl_run_log=$curl_run_log"
  echo "fat32_directory_index_cli_run_log=$cli_run_log"
  echo "fat32_directory_index_curl_serial_log=$curl_serial_log"
  echo "fat32_directory_index_cli_serial_log=$cli_serial_log"
  echo "fat32_directory_index_cli_transcript=$cli_transcript"
  echo "fat32_directory_index_parity_summary=$parity_summary"
  echo "fat32_directory_index_host_client_log=$host_client_log"
  echo "fat32_directory_index_curl_log=$curl_log"
  echo "fat32_directory_index_curl_get_body=$curl_get_body"
  echo "fat32_directory_index_cli_line=DPCLI:LS / HELLO.TXT 40 INDEX.HTM 110 LARGE.HTM 480 CHAIN.HTM 834"
  echo "fat32_directory_index_order=/HELLO.TXT,/INDEX.HTM,/LARGE.HTM,/CHAIN.HTM"
  echo "fat32_directory_index_entry_cap=4"
  echo "fat32_directory_index_name_cap=12"
  echo "fat32_directory_index_response_cap=192"
  echo "fat32_directory_index_unsupported_path=unsupported-path"
  echo "fat32_directory_index_http_cli_same_fs_task=true"
  echo "fat32_directory_index_default_writable=false"
  echo "fat32_directory_index_long_filenames=false"
  echo "fat32_directory_index_directory_expansion=false"
  echo "fat32_directory_index_journaling=false"
  echo "fat32_directory_index_cache_policy=false"
} >"$summary"

echo "x86_64 microkernel FAT32 directory-index proof passed."
echo "summary: $summary"
echo "host client log: $host_client_log"
echo "curl run log: $curl_run_log"
echo "cli run log: $cli_run_log"
