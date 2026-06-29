#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mnt_root="/home/user/mnt/dataplane"
log_dir="$mnt_root/logs"
tmp_dir="$mnt_root/tmp"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
curl_run_log="$log_dir/x86_64-microkernel-fat32-service-errors-$run_id.curl.run.log"
cli_run_log="$log_dir/x86_64-microkernel-fat32-service-errors-$run_id.cli.run.log"
host_client_log="$log_dir/x86_64-microkernel-fat32-service-errors-$run_id.host-client.log"
summary="$log_dir/x86_64-microkernel-fat32-service-errors-$run_id.summary"

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
curl_method_status="$(extract_label_path "$curl_run_log" "curl method-not-allowed status")"
curl_overlong_status="$(extract_label_path "$curl_run_log" "curl overlong-request status")"
curl_pcap="$(extract_label_path "$curl_run_log" "curl pcap")"

TMPDIR="$tmp_dir" DP_MICROKERNEL_CLI_HTTP_OPERATOR_PARITY_PROOF=1 \
  ./tools/x86_64_microkernel_fat32_run.sh --cli-http-operator-parity-proof >"$cli_run_log" 2>&1
cli_serial_log="$(extract_label_path "$cli_run_log" "serial log")"
cli_run_base="$(basename "$cli_serial_log")"
cli_run_id="${cli_run_base#x86_64-microkernel-fat32-}"
cli_run_id="${cli_run_id%.serial.log}"
cli_transcript="$log_dir/x86_64-microkernel-fat32-$cli_run_id.cli-http-operator-parity.cli-transcript.log"
cli_log="$log_dir/x86_64-microkernel-fat32-$cli_run_id.cli-http-operator-parity.cli.log"
parity_http_body="$log_dir/x86_64-microkernel-fat32-$cli_run_id.cli-http-operator-parity.http.body"
parity_http_status="$log_dir/x86_64-microkernel-fat32-$cli_run_id.cli-http-operator-parity.http.status"
parity_cap_negative_status="$log_dir/x86_64-microkernel-fat32-$cli_run_id.cli-http-operator-parity.cap-negative.status"
for derived in "$cli_transcript" "$cli_log" "$parity_http_body" "$parity_http_status" "$parity_cap_negative_status"; do
  [[ -e "$derived" ]] || fail "missing derived parity artifact: $derived"
done

python3 - "$curl_serial_log" "$curl_log" "$curl_get_body" "$curl_get_status" \
  "$curl_missing_status" "$curl_method_status" "$curl_overlong_status" "$curl_pcap" \
  "$cli_transcript" "$cli_log" "$parity_http_status" \
  "$parity_cap_negative_status" "$host_client_log" <<'PY'
import hashlib
import sys
from pathlib import Path

(
    curl_serial_log,
    curl_log,
    curl_get_body,
    curl_get_status,
    curl_missing_status,
    curl_method_status,
    curl_overlong_status,
    curl_pcap,
    cli_transcript,
    cli_log,
    parity_http_status,
    parity_cap_negative_status,
    host_client_log,
) = map(Path, sys.argv[1:])

expected_index = (
    b"<!doctype html><html><head><title>dataplane</title></head>"
    b"<body><h1>dataplane microkernel</h1></body></html>\r\n"
)

serial = curl_serial_log.read_bytes()
cli = cli_transcript.read_bytes()
curl_lines = curl_log.read_text(encoding="ascii", errors="replace")
host_lines = []

def require(condition, message):
    if not condition:
        raise SystemExit(message)

for marker in (
    b"DPMK:FS-SERVICE-ERROR-MATRIX-OK",
    b"DPFSERR:matrix small=ok multi_cluster=ok missing=unsupported-path bad_handle=fs-handle unopened_read=fs-handle bad_offset=fs-range output_cap=fs-capacity unsupported_path_shape=unsupported-path unsupported_write=unsupported-write-shape",
    b"DPFSERR:counters open=2 stat=1 read_at=5 list_root=1 errors=6",
    b"DPMK:FS-SERVICE-HTTP-INDEX-OK",
    b"DPMK:FS-SERVICE-MULTI-CLUSTER-OK",
):
    require(marker in serial, f"missing serial marker {marker!r}")

for marker in (
    b"DPCLI:FS-ERR /MISSING.TXT unsupported-path",
    b"DPCLI:FS-ERR /THISNAMEISTOOLONG.TXT long-filename",
    b"DPCLI:FS-ERR /OUT.TXT unsupported-write-shape",
    b"DPCLI:STAT /INDEX.HTM cluster=4 size=110 readonly=1",
):
    require(marker in cli, f"missing CLI evidence {marker!r}")

require(curl_get_status.read_bytes().strip() == b"200", "HTTP GET status was not 200")
require(curl_missing_status.read_bytes().strip() == b"404", "HTTP missing-file status was not 404")
require(curl_method_status.read_bytes().strip() == b"405", "HTTP unsupported-write/method status was not 405")
require(curl_overlong_status.read_bytes().strip() == b"413", "HTTP output-cap/path-shape status was not 413")
require(curl_get_body.read_bytes() == expected_index, "HTTP body did not match exact INDEX.HTM bytes")
require(parity_http_status.read_bytes().strip() == b"200", "parity HTTP status was not 200")
require(parity_cap_negative_status.read_bytes().strip() == b"413", "parity cap-negative status was not 413")
require(expected_index in curl_pcap.read_bytes(), "curl pcap missing exact INDEX.HTM bytes")
require("curl_missing_status=404" in curl_lines, "curl host log missing 404 evidence")
require("curl_method_not_allowed_status=405" in curl_lines, "curl host log missing 405 evidence")
require("curl_overlong_request_status=413" in curl_lines, "curl host log missing 413 evidence")

host_lines.extend([
    "fat32_service_errors_host_client_status=pass",
    "fat32_service_errors_http_get_index_status=200",
    "fat32_service_errors_http_missing_status=404",
    "fat32_service_errors_http_unsupported_write_status=405",
    "fat32_service_errors_http_output_cap_status=413",
    f"fat32_service_errors_http_index_sha256={hashlib.sha256(expected_index).hexdigest()}",
    "fat32_service_errors_cli_missing=unsupported-path",
    "fat32_service_errors_cli_unsupported_path_shape=long-filename",
    "fat32_service_errors_cli_unsupported_write=unsupported-write-shape",
    "fat32_service_errors_matrix_markers_match=true",
])
host_client_log.write_text("\n".join(host_lines) + "\n", encoding="ascii")
PY

for literal in \
  "fat32_service_errors_host_client_status=pass" \
  "fat32_service_errors_http_missing_status=404" \
  "fat32_service_errors_cli_unsupported_write=unsupported-write-shape" \
  "fat32_service_errors_matrix_markers_match=true"; do
  require_contains "$host_client_log" "$literal"
done

{
  echo "fat32_service_errors_summary_status=pass"
  echo "fat32_service_errors_run_id=$run_id"
  echo "fat32_service_errors_curl_run_log=$curl_run_log"
  echo "fat32_service_errors_cli_run_log=$cli_run_log"
  echo "fat32_service_errors_curl_serial_log=$curl_serial_log"
  echo "fat32_service_errors_cli_transcript=$cli_transcript"
  echo "fat32_service_errors_host_client_log=$host_client_log"
  echo "fat32_service_errors_curl_log=$curl_log"
  echo "fat32_service_errors_curl_get_body=$curl_get_body"
  echo "fat32_service_errors_curl_pcap=$curl_pcap"
  echo "fat32_service_errors_small_file=ok"
  echo "fat32_service_errors_multi_cluster_file=ok"
  echo "fat32_service_errors_missing_file=unsupported-path"
  echo "fat32_service_errors_bad_handle=fs-handle"
  echo "fat32_service_errors_unopened_read=fs-handle"
  echo "fat32_service_errors_bad_offset=fs-range"
  echo "fat32_service_errors_output_cap=fs-capacity"
  echo "fat32_service_errors_unsupported_path_shape=unsupported-path,long-filename"
  echo "fat32_service_errors_unsupported_write=unsupported-write-shape"
  echo "fat32_service_errors_http_cli_same_fs_task=true"
  echo "fat32_service_errors_serial_counters=DPFSERR:counters open=2 stat=1 read_at=5 list_root=1 errors=6"
  echo "fat32_service_errors_default_writable=false"
  echo "fat32_service_errors_arbitrary_file_writes=false"
  echo "fat32_service_errors_long_filenames=false"
  echo "fat32_service_errors_directory_expansion=false"
  echo "fat32_service_errors_journaling=false"
  echo "fat32_service_errors_crash_consistency=false"
  echo "fat32_service_errors_repair_fsck=false"
  echo "fat32_service_errors_restart_reset_replay=false"
  echo "fat32_service_errors_tls_https=false"
  echo "fat32_service_errors_generic_socket_api=false"
  echo "fat32_service_errors_generic_ipc_framework=false"
  echo "fat32_service_errors_heap_mailbox=false"
  echo "fat32_service_errors_benchmark_retuning=false"
  echo "fat32_service_errors_cache_policy=false"
} >"$summary"

echo "x86_64 microkernel FAT32 service-errors proof passed."
echo "summary: $summary"
echo "host client log: $host_client_log"
echo "curl run log: $curl_run_log"
echo "cli run log: $cli_run_log"
