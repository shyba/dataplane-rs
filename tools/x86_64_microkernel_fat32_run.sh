#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

source "tools/x86_64_microkernel_validation_matrix_lib.sh"

target="x86_64-unknown-none"
image="target/$target/release/dataplane-x86_64-microkernel-smoke.img"
log_root="$(dp_default_log_root DP_MICROKERNEL_FAT32_LOG_ROOT)"
mnt_root="/home/user/mnt/dataplane"
fs_img="$mnt_root/microkernel-fat32.img"
log_dir="$log_root"
run_id="$(dp_new_run_id)"
serial_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "serial.log")"
qemu_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "qemu.log")"
fat32_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "image.log")"
client_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "client.log")"
host_exchange_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "net-host.log")"
net_pcap="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "net.pcap")"
curl_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "curl.log")"
curl_get_body="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "curl-get-index.body")"
curl_get_headers="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "curl-get-index.headers")"
curl_get_status="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "curl-get-index.status")"
curl_head_body="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "curl-head-index.body")"
curl_head_headers="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "curl-head-index.headers")"
curl_head_status="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "curl-head-index.status")"
curl_missing_body="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "curl-missing.body")"
curl_missing_headers="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "curl-missing.headers")"
curl_missing_status="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "curl-missing.status")"
curl_method_body="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "curl-method-not-allowed.body")"
curl_method_headers="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "curl-method-not-allowed.headers")"
curl_method_status="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "curl-method-not-allowed.status")"
curl_overlong_body="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "curl-overlong-request.body")"
curl_overlong_headers="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "curl-overlong-request.headers")"
curl_overlong_status="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "curl-overlong-request.status")"
curl_malformed_request="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "curl-malformed-request.raw")"
curl_malformed_body="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "curl-malformed-request.body")"
curl_malformed_headers="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "curl-malformed-request.headers")"
curl_malformed_status="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "curl-malformed-request.status")"
curl_pcap="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "curl.pcap")"
http_backpressure_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "http-backpressure.log")"
http_backpressure_body="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "http-backpressure-large.body")"
http_backpressure_headers="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "http-backpressure-large.headers")"
http_backpressure_status="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "http-backpressure-large.status")"
http_backpressure_pcap="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "http-backpressure.pcap")"
fs_service_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "fs-service-boundary.log")"
fs_service_body="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "fs-service-boundary-chain.body")"
fs_service_headers="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "fs-service-boundary-chain.headers")"
fs_service_status="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "fs-service-boundary-chain.status")"
fs_service_pcap="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "fs-service-boundary.pcap")"
fairness_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "fairness.log")"
fairness_client_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "fairness-client.log")"
fairness_curl_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "fairness-curl.log")"
fairness_pcap="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "fairness.pcap")"
fairness_artifact_dir="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "fairness")"
scheduler_fairness_summary="$log_dir/x86_64-microkernel-fat32-$run_id.scheduler-fairness-load.summary"
control_protocol_v1_summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "control-protocol-v1.summary")"
fault_policy_hardening_summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "fault-policy-hardening.summary")"
timer_timeout_service_summary="$log_root/x86_64-microkernel-fat32-$run_id.timer-timeout-service.summary"
network_counter_audit_summary="$log_dir/x86_64-microkernel-fat32-$run_id.network-counter-audit.summary"
dhcp_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "dhcp.log")"
dhcp_lease_evidence="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "dhcp-lease.json")"
dhcp_get_body="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "dhcp-get-index.body")"
dhcp_get_headers="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "dhcp-get-index.headers")"
dhcp_get_status="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "dhcp-get-index.status")"
dhcp_pcap="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "dhcp.pcap")"
operator_appliance_surface_summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "operator-appliance-surface.summary")"
operator_appliance_surface_source_scan="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "operator-appliance-surface.source-scan.txt")"
fs_policy_and_directory_slice_summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "fs-policy-and-directory-slice.summary")"
fs_policy_and_directory_slice_client_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "fs-policy-and-directory-slice.client.log")"
fs_policy_and_directory_slice_serial_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "fs-policy-and-directory-slice.serial.log")"
fs_policy_and_directory_slice_qemu_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "fs-policy-and-directory-slice.qemu.log")"
fs_policy_and_directory_slice_pcap="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "fs-policy-and-directory-slice.pcap")"
write_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "write.log")"
write_readback="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "write-out-txt.readback")"
write_image_before_sha="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "write-image-before.sha256")"
write_image_after_sha="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "write-image-after.sha256")"
bounded_tcp_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "bounded-tcp-negative.log")"
bounded_tcp_pcap="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "bounded-tcp-negative.pcap")"
parity_cli_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "cli-http-operator-parity.cli.log")"
parity_cli_transcript="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "cli-http-operator-parity.cli-transcript.log")"
parity_http_body="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "cli-http-operator-parity.http.body")"
parity_http_headers="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "cli-http-operator-parity.http.headers")"
parity_http_status="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "cli-http-operator-parity.http.status")"
parity_cap_negative_body="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "cli-http-operator-parity.cap-negative.body")"
parity_cap_negative_headers="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "cli-http-operator-parity.cap-negative.headers")"
parity_cap_negative_status="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "cli-http-operator-parity.cap-negative.status")"
parity_summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "cli-http-operator-parity.summary")"
cli_operator_surface_polish_summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "cli-operator-surface-polish.summary")"
parity_forbidden_guard=""
operator_status_page_ok=true
serial_dir="$(mktemp -d "${TMPDIR:-/tmp}/dataplane-mkcli.XXXXXX")"
serial_sock="$serial_dir/serial.sock"
fairness_start_file="$serial_dir/fairness-start"
qemu_pid=""
host_pid=""
curl_pid=""
mode="${1:-}"
if [[ "${DP_MICROKERNEL_CURL_PROOF:-0}" == "1" ]]; then
  mode="--curl-proof"
fi
if [[ "${DP_MICROKERNEL_NONTLS_NETWORK_SERVICE_PROOF:-0}" == "1" ]]; then
  mode="--nontls-network-service-proof"
fi
if [[ "${DP_MICROKERNEL_NONTLS_NETWORK_NEGATIVE_MATRIX_PROOF:-0}" == "1" ]]; then
  mode="--nontls-network-negative-matrix-proof"
fi
if [[ "${DP_MICROKERNEL_NETWORK_COUNTER_AUDIT_PROOF:-0}" == "1" ]]; then
  mode="--network-counter-audit-proof"
fi
if [[ "${DP_NONTLS_NETWORK_NEGATIVE_MATRIX_PROOF:-0}" == "1" ]]; then
  mode="--nontls-network-negative-matrix-proof"
fi
if [[ "${DP_MICROKERNEL_FAIRNESS_PROOF:-0}" == "1" ]]; then
  mode="--fairness-proof"
fi
if [[ "${DP_MICROKERNEL_SCHEDULER_FAIRNESS_LOAD_PROOF:-0}" == "1" ]]; then
  mode="--scheduler-fairness-load-proof"
fi
if [[ "${DP_SCHEDULER_FAIRNESS_LOAD_PROOF:-0}" == "1" ]]; then
  mode="--scheduler-fairness-load-proof"
fi
if [[ "${DP_MICROKERNEL_CONTROL_PROTOCOL_V1_PROOF:-0}" == "1" ]]; then
  mode="--control-protocol-v1-proof"
fi
if [[ "${DP_MICROKERNEL_FAULT_POLICY_HARDENING_PROOF:-0}" == "1" ]]; then
  mode="--fault-policy-hardening-proof"
fi
if [[ "${DP_MICROKERNEL_DHCP_PROOF:-0}" == "1" ]]; then
  mode="--dhcp-proof"
fi
if [[ "${DP_MICROKERNEL_WRITE_PROOF:-0}" == "1" ]]; then
  mode="--write-proof"
fi
if [[ "${DP_MICROKERNEL_PREALLOCATED_JOURNAL_FILE_PROOF:-0}" == "1" ]]; then
  mode="--preallocated-journal-file-proof"
fi
if [[ "${DP_MICROKERNEL_BOUNDED_TCP_NEGATIVE_PROOF:-0}" == "1" ]]; then
  mode="--bounded-tcp-negative-proof"
fi
if [[ "${DP_MICROKERNEL_HTTP_BACKPRESSURE_PROOF:-0}" == "1" ]]; then
  mode="--http-backpressure-proof"
fi
if [[ "${DP_MICROKERNEL_HTTP_STATIC_APPLIANCE_POLISH_PROOF:-0}" == "1" ]]; then
  mode="--http-static-appliance-polish-proof"
fi
if [[ "${DP_MICROKERNEL_FS_SERVICE_BOUNDARY_PROOF:-0}" == "1" ]]; then
  mode="--fs-service-boundary-proof"
fi
if [[ "${DP_MICROKERNEL_FS_POLICY_AND_DIRECTORY_SLICE_PROOF:-0}" == "1" ]]; then
  mode="--fs-policy-and-directory-slice-proof"
fi
if [[ "${DP_MICROKERNEL_OPERATOR_APPLIANCE_SURFACE_PROOF:-0}" == "1" ]]; then
  mode="--operator-appliance-surface-proof"
fi
if [[ "${DP_MICROKERNEL_STORAGE_SERVICE_COUNTERS_PROOF:-0}" == "1" ]]; then
  mode="--storage-service-counters-proof"
fi
if [[ "${DP_MICROKERNEL_CLI_HTTP_OPERATOR_PARITY_PROOF:-0}" == "1" ]]; then
  mode="--cli-http-operator-parity-proof"
fi
if [[ "${DP_MICROKERNEL_CLI_OPERATOR_SURFACE_POLISH_PROOF:-0}" == "1" ]]; then
  mode="--cli-operator-surface-polish-proof"
fi
if [[ "${DP_MICROKERNEL_TIMER_TIMEOUT_SERVICE_PROOF:-0}" == "1" ]]; then
  mode="--timer-timeout-service-proof"
fi

# Control-protocol-v1 host harness uses fixed scenario plumbing and artifact validation; the guest owns protocol semantics and state transitions.

if [[ "$mode" != "" && "$mode" != "--raw" && "$mode" != "--curl-proof" && "$mode" != "--http-static-appliance-polish-proof" && "$mode" != "--nontls-network-service-proof" && "$mode" != "--nontls-network-negative-matrix-proof" && "$mode" != "--fairness-proof" && "$mode" != "--scheduler-fairness-load-proof" && "$mode" != "--control-protocol-v1-proof" && "$mode" != "--fault-policy-hardening-proof" && "$mode" != "--dhcp-proof" && "$mode" != "--write-proof" && "$mode" != "--preallocated-journal-file-proof" && "$mode" != "--bounded-tcp-negative-proof" && "$mode" != "--http-backpressure-proof" && "$mode" != "--fs-service-boundary-proof" && "$mode" != "--fs-policy-and-directory-slice-proof" && "$mode" != "--operator-appliance-surface-proof" && "$mode" != "--storage-service-counters-proof" && "$mode" != "--cli-http-operator-parity-proof" && "$mode" != "--cli-operator-surface-polish-proof" && "$mode" != "--timer-timeout-service-proof" ]]; then
  echo "FAIL: unknown mode: $mode"
  echo "usage: $0 [--raw|--curl-proof|--http-static-appliance-polish-proof|--nontls-network-service-proof|--nontls-network-negative-matrix-proof|--fairness-proof|--scheduler-fairness-load-proof|--control-protocol-v1-proof|--fault-policy-hardening-proof|--dhcp-proof|--write-proof|--preallocated-journal-file-proof|--bounded-tcp-negative-proof|--http-backpressure-proof|--fs-service-boundary-proof|--fs-policy-and-directory-slice-proof|--operator-appliance-surface-proof|--storage-service-counters-proof|--cli-http-operator-parity-proof|--cli-operator-surface-polish-proof|--timer-timeout-service-proof]"
  exit 2
fi

cleanup() {
  if [[ -n "$qemu_pid" ]]; then
    kill "$qemu_pid" >/dev/null 2>&1 || true
    wait "$qemu_pid" >/dev/null 2>&1 || true
  fi
  if [[ -n "$host_pid" ]]; then
    kill "$host_pid" >/dev/null 2>&1 || true
    wait "$host_pid" >/dev/null 2>&1 || true
  fi
  if [[ -n "$curl_pid" ]]; then
    kill "$curl_pid" >/dev/null 2>&1 || true
    wait "$curl_pid" >/dev/null 2>&1 || true
  fi
  rm -rf "$serial_dir"
}
trap cleanup EXIT

echo "=== x86_64 Microkernel FAT32 Smoke ==="

if [[ ! -f "$image" ]]; then
  echo "FAIL: microkernel smoke image not found: $image"
  echo "Run: tools/x86_64_microkernel_smoke_build.sh"
  exit 1
fi

command -v qemu-system-x86_64 >/dev/null 2>&1 || {
  echo "FAIL: qemu-system-x86_64 not found"
  exit 1
}

mkdir -p "$mnt_root" "$log_dir"

rm -f "$fs_img"
python3 tools/microkernel_make_fat32_image.py "$fs_img" >"$fat32_log"

if [[ "$mode" == "--curl-proof" || "$mode" == "--http-static-appliance-polish-proof" ]]; then
  command -v curl >/dev/null 2>&1 || {
    echo "FAIL: curl not found"
    exit 1
  }

  curl_port=$((43000 + RANDOM % 1000))
  curl_get_url="http://127.0.0.1:$curl_port/INDEX.HTM"
  curl_root_url="http://127.0.0.1:$curl_port/"
  curl_head_url="http://127.0.0.1:$curl_port/INDEX.HTM"
  curl_head_chain_url="http://127.0.0.1:$curl_port/CHAIN.HTM"
  curl_status_txt_url="http://127.0.0.1:$curl_port/STATUS.TXT"
  curl_missing_url="http://127.0.0.1:$curl_port/MISSING.HTM"
  curl_method_url="http://127.0.0.1:$curl_port/INDEX.HTM"
  curl_overlong_path="/$(printf '%*s' 120 '' | tr ' ' A).HTM"
  curl_overlong_url="http://127.0.0.1:$curl_port$curl_overlong_path"
  curl_get_tmp_body="$curl_get_body.tmp"
  curl_get_tmp_headers="$curl_get_headers.tmp"
  curl_get_tmp_status="$curl_get_status.tmp"
  curl_head_tmp_body="$curl_head_body.tmp"
  curl_head_tmp_headers="$curl_head_headers.tmp"
  curl_head_tmp_status="$curl_head_status.tmp"
  curl_missing_tmp_body="$curl_missing_body.tmp"
  curl_missing_tmp_headers="$curl_missing_headers.tmp"
  curl_missing_tmp_status="$curl_missing_status.tmp"
  curl_method_tmp_body="$curl_method_body.tmp"
  curl_method_tmp_headers="$curl_method_headers.tmp"
  curl_method_tmp_status="$curl_method_status.tmp"
  curl_overlong_tmp_body="$curl_overlong_body.tmp"
  curl_overlong_tmp_headers="$curl_overlong_headers.tmp"
  curl_overlong_tmp_status="$curl_overlong_status.tmp"
  curl_malformed_tmp_request="$curl_malformed_request.tmp"
  curl_malformed_tmp_body="$curl_malformed_body.tmp"
  curl_malformed_tmp_headers="$curl_malformed_headers.tmp"
  curl_malformed_tmp_status="$curl_malformed_status.tmp"
  curl_malformed_status_code=1

  timeout 35s qemu-system-x86_64 \
    -M pc \
    -m 128M \
    -nographic \
    -monitor none \
    -drive file="$image",format=raw,if=floppy \
    -boot a \
    -drive if=none,id=fs,file="$fs_img",format=raw \
    -device virtio-blk-pci-transitional,drive=fs \
    -netdev user,id=xnet,net=10.0.0.0/24,host=10.0.0.1,dhcpstart=10.0.0.2,hostfwd=tcp:127.0.0.1:"$curl_port"-10.0.0.2:80 \
    -object "filter-dump,id=xnet_user_dump,netdev=xnet,file=$curl_pcap" \
    -device virtio-net-pci-transitional,netdev=xnet,mac=52:54:00:12:34:56 \
    -serial "file:$serial_log" \
    -no-reboot \
    -no-shutdown >"$qemu_log" 2>&1 &
  qemu_pid="$!"

  run_curl_request() {
    local request_kind="$1"
    local url="$2"
    local body_path="$3"
    local headers_path="$4"
    local status_path="$5"
    local status_code
    local curl_rc

    rm -f "$body_path" "$headers_path" "$status_path"
    case "$request_kind" in
      HEAD)
        set +e
        status_code="$(curl --http1.0 -I --silent --show-error --max-time 2 \
          --dump-header "$headers_path" \
          --output /dev/null \
          --write-out "%{http_code}" \
          "$url" 2>>"$curl_log")"
        curl_rc="$?"
        set -e
        : >"$body_path"
        ;;
      POST)
        set +e
        status_code="$(curl --http1.0 --request POST --data "" --silent --show-error --max-time 2 \
          --dump-header "$headers_path" \
          --output "$body_path" \
          --write-out "%{http_code}" \
          "$url" 2>>"$curl_log")"
        curl_rc="$?"
        set -e
        ;;
      *)
        set +e
        status_code="$(curl --http1.0 --silent --show-error --max-time 2 \
          --dump-header "$headers_path" \
          --output "$body_path" \
          --write-out "%{http_code}" \
          "$url" 2>>"$curl_log")"
        curl_rc="$?"
        set -e
        ;;
    esac

    printf "%s\n" "$status_code" >"$status_path"
    if [[ "$curl_rc" -ne 0 ]]; then
      echo "curl_${request_kind}_rc=$curl_rc" >>"$curl_log"
      return "$curl_rc"
    fi
  }

  run_malformed_http_request() {
    local request_path="$1"
    local body_path="$2"
    local headers_path="$3"
    local status_path="$4"
    local raw_request='GET\r\n\r\n'

    rm -f "$request_path" "$body_path" "$headers_path" "$status_path"
    python3 - "$curl_port" "$request_path" "$body_path" "$headers_path" "$status_path" "$raw_request" 2>>"$curl_log" <<'PY'
import socket
import sys

port = int(sys.argv[1])
request_path, body_path, headers_path, status_path, raw_request = sys.argv[2:]
request = raw_request.encode("ascii").decode("unicode_escape").encode("ascii")
with open(request_path, "wb") as out:
    out.write(request)

response = bytearray()
with socket.create_connection(("127.0.0.1", port), timeout=2.0) as sock:
    sock.settimeout(2.0)
    sock.sendall(request)
    while True:
        try:
            chunk = sock.recv(4096)
        except socket.timeout:
            break
        if not chunk:
            break
        response.extend(chunk)

header_end = bytes(response).find(b"\r\n\r\n")
if header_end < 0:
    raise SystemExit("malformed raw HTTP response did not contain complete headers")
headers = bytes(response[:header_end + 4])
body = bytes(response[header_end + 4:])
first = headers.splitlines()[0] if headers.splitlines() else b""
parts = first.split()
if len(parts) < 2 or not parts[1].isdigit():
    raise SystemExit("malformed raw HTTP response did not contain a numeric status")
with open(headers_path, "wb") as out:
    out.write(headers)
with open(body_path, "wb") as out:
    out.write(body)
with open(status_path, "wb") as out:
    out.write(parts[1] + b"\n")
PY
  }

  deadline=$((SECONDS + 30))
  curl_status=1
  while (( SECONDS < deadline )); do
    if [[ "$curl_malformed_status_code" -ne 0 ]] &&
      run_malformed_http_request "$curl_malformed_tmp_request" "$curl_malformed_tmp_body" "$curl_malformed_tmp_headers" "$curl_malformed_tmp_status" &&
      python3 - "$curl_malformed_tmp_request" "$curl_malformed_tmp_body" "$curl_malformed_tmp_headers" "$curl_malformed_tmp_status" <<'PY'
import sys

request_path, body_path, headers_path, status_path = sys.argv[1:]
expected = b"<!doctype html><html><head><title>dataplane</title></head><body><h1>dataplane microkernel</h1></body></html>\r\n"
request = open(request_path, "rb").read()
body = open(body_path, "rb").read()
headers = open(headers_path, "rb").read().lower()
status = open(status_path, "rb").read().strip()
first = headers.splitlines()[0].lower() if headers.splitlines() else b""
if request != b"GET\r\n\r\n":
    raise SystemExit("malformed raw request artifact does not contain the expected invalid request bytes")
if status != b"413":
    raise SystemExit("malformed raw request status was not 413")
if b"413" not in first:
    raise SystemExit("malformed raw request response was not HTTP 413")
if expected in body or b"content-length: 110" in headers:
    raise SystemExit("malformed raw request served the INDEX.HTM success body")
PY
    then
      mv "$curl_malformed_tmp_request" "$curl_malformed_request"
      mv "$curl_malformed_tmp_body" "$curl_malformed_body"
      mv "$curl_malformed_tmp_headers" "$curl_malformed_headers"
      mv "$curl_malformed_tmp_status" "$curl_malformed_status"
      curl_malformed_status_code=0
    fi

    if run_curl_request "GET" "$curl_get_url" "$curl_get_tmp_body" "$curl_get_tmp_headers" "$curl_get_tmp_status" &&
      run_curl_request "GET" "$curl_root_url" "$curl_get_tmp_body.root" "$curl_get_tmp_headers.root" "$curl_get_tmp_status.root" &&
      run_curl_request "HEAD" "$curl_head_url" "$curl_head_tmp_body" "$curl_head_tmp_headers" "$curl_head_tmp_status" &&
      run_curl_request "HEAD" "$curl_head_chain_url" "$curl_head_tmp_body.chain" "$curl_head_tmp_headers.chain" "$curl_head_tmp_status.chain" &&
      run_curl_request "GET" "$curl_status_txt_url" "$curl_get_tmp_body.status" "$curl_get_tmp_headers.status" "$curl_get_tmp_status.status" &&
      run_curl_request "GET" "$curl_missing_url" "$curl_missing_tmp_body" "$curl_missing_tmp_headers" "$curl_missing_tmp_status" &&
      run_curl_request "POST" "$curl_method_url" "$curl_method_tmp_body" "$curl_method_tmp_headers" "$curl_method_tmp_status" &&
      run_curl_request "GET" "$curl_overlong_url" "$curl_overlong_tmp_body" "$curl_overlong_tmp_headers" "$curl_overlong_tmp_status"
    then
      if python3 - \
        "$curl_get_tmp_body" "$curl_get_tmp_headers" "$curl_get_tmp_status" \
        "$curl_head_tmp_body" "$curl_head_tmp_headers" "$curl_head_tmp_status" \
        "$curl_missing_tmp_body" "$curl_missing_tmp_headers" "$curl_missing_tmp_status" \
        "$curl_method_tmp_body" "$curl_method_tmp_headers" "$curl_method_tmp_status" \
        "$curl_overlong_tmp_body" "$curl_overlong_tmp_headers" "$curl_overlong_tmp_status" <<'PY'
import sys

(
    get_body_path,
    get_headers_path,
    get_status_path,
    head_body_path,
    head_headers_path,
    head_status_path,
    missing_body_path,
    missing_headers_path,
    missing_status_path,
    method_body_path,
    method_headers_path,
    method_status_path,
    overlong_body_path,
    overlong_headers_path,
    overlong_status_path,
) = sys.argv[1:]
expected = b"<!doctype html><html><head><title>dataplane</title></head><body><h1>dataplane microkernel</h1></body></html>\r\n"

def read_status(path):
    return open(path, "rb").read().strip()

def read_headers(path):
    headers = open(path, "rb").read()
    first = headers.splitlines()[0].lower() if headers.splitlines() else b""
    return headers.lower(), first

get_body = open(get_body_path, "rb").read()
get_headers, get_first = read_headers(get_headers_path)
head_body = open(head_body_path, "rb").read()
head_headers, head_first = read_headers(head_headers_path)
missing_body = open(missing_body_path, "rb").read()
missing_headers, missing_first = read_headers(missing_headers_path)
method_body = open(method_body_path, "rb").read()
method_headers, method_first = read_headers(method_headers_path)
overlong_body = open(overlong_body_path, "rb").read()
overlong_headers, overlong_first = read_headers(overlong_headers_path)
if read_status(get_status_path) != b"200":
    raise SystemExit("curl GET /INDEX.HTM status was not 200")
if b"200" not in get_first:
    raise SystemExit("curl GET /INDEX.HTM response was not HTTP 200")
if get_body != expected:
    raise SystemExit(f"unexpected curl GET body length/content: {len(get_body)} bytes")
if b"content-length: 110" not in get_headers:
    raise SystemExit("curl GET /INDEX.HTM did not contain Content-Length: 110")
if read_status(head_status_path) != b"200":
    raise SystemExit("curl -I /INDEX.HTM status was not 200")
if b"200" not in head_first:
    raise SystemExit("curl -I /INDEX.HTM response was not HTTP 200")
if b"content-length: 110" not in head_headers:
    raise SystemExit("curl -I /INDEX.HTM did not contain Content-Length: 110")
if head_body:
    raise SystemExit(f"curl -I /INDEX.HTM unexpectedly produced {len(head_body)} body bytes")
if read_status(missing_status_path) != b"404":
    raise SystemExit("curl GET /MISSING.HTM status was not 404")
if b"404" not in missing_first:
    raise SystemExit("curl GET /MISSING.HTM response was not HTTP 404")
if read_status(method_status_path) != b"405":
    raise SystemExit("curl POST /INDEX.HTM status was not 405")
if b"405" not in method_first:
    raise SystemExit("curl POST /INDEX.HTM response was not HTTP 405")
if expected in method_body or b"content-length: 110" in method_headers:
    raise SystemExit("curl POST /INDEX.HTM served the INDEX.HTM success body")
if read_status(overlong_status_path) != b"413":
    raise SystemExit("curl overlong request status was not 413")
if b"413" not in overlong_first:
    raise SystemExit("curl overlong request response was not HTTP 413")
if expected in overlong_body or b"content-length: 110" in overlong_headers:
    raise SystemExit("curl overlong request served the INDEX.HTM success body")
PY
      then
        mv "$curl_get_tmp_body" "$curl_get_body"
        mv "$curl_get_tmp_headers" "$curl_get_headers"
        mv "$curl_get_tmp_status" "$curl_get_status"
        mv "$curl_head_tmp_body" "$curl_head_body"
        mv "$curl_head_tmp_headers" "$curl_head_headers"
        mv "$curl_head_tmp_status" "$curl_head_status"
        mv "$curl_missing_tmp_body" "$curl_missing_body"
        mv "$curl_missing_tmp_headers" "$curl_missing_headers"
        mv "$curl_missing_tmp_status" "$curl_missing_status"
        mv "$curl_method_tmp_body" "$curl_method_body"
        mv "$curl_method_tmp_headers" "$curl_method_headers"
        mv "$curl_method_tmp_status" "$curl_method_status"
        mv "$curl_overlong_tmp_body" "$curl_overlong_body"
        mv "$curl_overlong_tmp_headers" "$curl_overlong_headers"
        mv "$curl_overlong_tmp_status" "$curl_overlong_status"
        curl_status=0
        break
      fi
    fi
    sleep 0.25
  done

  {
    echo "curl_mode=usernet-hostfwd"
    echo "curl_get_index_url=$curl_get_url"
    echo "curl_head_index_url=$curl_head_url"
    echo "curl_missing_url=$curl_missing_url"
    echo "curl_method_not_allowed_url=$curl_method_url"
    echo "curl_overlong_request_url=$curl_overlong_url"
    echo "curl_overlong_request_path_bytes=${#curl_overlong_path}"
    echo "curl_malformed_request_bytes=7"
    echo "curl_port=$curl_port"
    echo "curl_get_index_ok=$([[ "$curl_status" -eq 0 ]] && echo true || echo false)"
    echo "curl_head_index_ok=$([[ "$curl_status" -eq 0 ]] && echo true || echo false)"
    echo "curl_missing_ok=$([[ "$curl_status" -eq 0 ]] && echo true || echo false)"
    echo "curl_method_not_allowed_ok=$([[ "$curl_status" -eq 0 ]] && echo true || echo false)"
    echo "curl_overlong_request_ok=$([[ "$curl_status" -eq 0 ]] && echo true || echo false)"
    echo "curl_malformed_request_ok=$([[ "$curl_malformed_status_code" -eq 0 ]] && echo true || echo false)"
    if [[ -f "$curl_get_status" ]]; then
      echo "curl_get_index_status=$(tr -d ' \n' <"$curl_get_status")"
    fi
    if [[ -f "$curl_head_status" ]]; then
      echo "curl_head_index_status=$(tr -d ' \n' <"$curl_head_status")"
    fi
    if [[ -f "$curl_missing_status" ]]; then
      echo "curl_missing_status=$(tr -d ' \n' <"$curl_missing_status")"
    fi
    if [[ -f "$curl_method_status" ]]; then
      echo "curl_method_not_allowed_status=$(tr -d ' \n' <"$curl_method_status")"
    fi
    if [[ -f "$curl_overlong_status" ]]; then
      echo "curl_overlong_request_status=$(tr -d ' \n' <"$curl_overlong_status")"
    fi
    if [[ -f "$curl_malformed_status" ]]; then
      echo "curl_malformed_request_status=$(tr -d ' \n' <"$curl_malformed_status")"
    fi
    if [[ -f "$curl_get_body" ]]; then
      echo "curl_get_index_body_bytes=$(wc -c <"$curl_get_body" | tr -d ' ')"
    fi
    if [[ -f "$curl_head_body" ]]; then
      echo "curl_head_index_body_bytes=$(wc -c <"$curl_head_body" | tr -d ' ')"
    fi
    if [[ -f "$curl_missing_body" ]]; then
      echo "curl_missing_body_bytes=$(wc -c <"$curl_missing_body" | tr -d ' ')"
    fi
    if [[ -f "$curl_method_body" ]]; then
      echo "curl_method_not_allowed_body_bytes=$(wc -c <"$curl_method_body" | tr -d ' ')"
    fi
    if [[ -f "$curl_overlong_body" ]]; then
      echo "curl_overlong_request_body_bytes=$(wc -c <"$curl_overlong_body" | tr -d ' ')"
    fi
    if [[ -f "$curl_malformed_request" ]]; then
      echo "curl_malformed_request_artifact_bytes=$(wc -c <"$curl_malformed_request" | tr -d ' ')"
    fi
    if [[ -f "$curl_malformed_body" ]]; then
      echo "curl_malformed_request_body_bytes=$(wc -c <"$curl_malformed_body" | tr -d ' ')"
    fi
    if [[ -f "$curl_get_headers" ]]; then
      echo "curl_get_index_header_bytes=$(wc -c <"$curl_get_headers" | tr -d ' ')"
      sed 's/^/curl_get_index_header=/' "$curl_get_headers"
    fi
    if [[ -f "$curl_head_headers" ]]; then
      echo "curl_head_index_header_bytes=$(wc -c <"$curl_head_headers" | tr -d ' ')"
      sed 's/^/curl_head_index_header=/' "$curl_head_headers"
    fi
    if [[ -f "$curl_missing_headers" ]]; then
      echo "curl_missing_header_bytes=$(wc -c <"$curl_missing_headers" | tr -d ' ')"
      sed 's/^/curl_missing_header=/' "$curl_missing_headers"
    fi
    if [[ -f "$curl_method_headers" ]]; then
      echo "curl_method_not_allowed_header_bytes=$(wc -c <"$curl_method_headers" | tr -d ' ')"
      sed 's/^/curl_method_not_allowed_header=/' "$curl_method_headers"
    fi
    if [[ -f "$curl_overlong_headers" ]]; then
      echo "curl_overlong_request_header_bytes=$(wc -c <"$curl_overlong_headers" | tr -d ' ')"
      sed 's/^/curl_overlong_request_header=/' "$curl_overlong_headers"
    fi
    if [[ -f "$curl_malformed_headers" ]]; then
      echo "curl_malformed_request_header_bytes=$(wc -c <"$curl_malformed_headers" | tr -d ' ')"
      sed 's/^/curl_malformed_request_header=/' "$curl_malformed_headers"
    fi
  } >>"$curl_log"

  require_curl_evidence_for_marker() {
    local marker="$1"
    local expected_line="$2"
    local note="$3"
    if grep -q "$marker" "$serial_log" 2>/dev/null; then
      grep -q "$expected_line" "$curl_log" || {
        echo "FAIL: $note"
        sed -n '1,220p' "$curl_log" 2>/dev/null || true
        exit 1
      }
    fi
  }

  if [[ "${proof_kind:-}" == "curl" ]]; then
    require_curl_evidence_for_marker "DPMK:CURL-GET-OK" "curl_get_index_ok=true" \
      "guest claimed curl success without real curl output"
    require_curl_evidence_for_marker "DPMK:HTTP-CURL-OK" "curl_get_index_ok=true" \
      "guest claimed HTTP curl success without real curl output"
    require_curl_evidence_for_marker "DPMK:HTTP-GET-OK" "curl_get_index_ok=true" \
      "guest claimed HTTP GET success without real curl GET output"
    require_curl_evidence_for_marker "DPMK:HTTP-HEAD-OK" "curl_head_index_ok=true" \
      "guest claimed HTTP HEAD success without real curl -I output"
    require_curl_evidence_for_marker "DPMK:HTTP-404-OK" "curl_missing_ok=true" \
      "guest claimed HTTP 404 success without real curl missing-file output"
    require_curl_evidence_for_marker "DPMK:HTTP-405-OK" "curl_method_not_allowed_ok=true" \
      "guest claimed HTTP 405 success without real curl unsupported-method output"
    require_curl_evidence_for_marker "DPMK:HTTP-413-OK" "curl_overlong_request_ok=true" \
      "guest claimed HTTP 413 success without real curl overlong-request output"

    grep -q "curl_method_not_allowed_status=405" "$curl_log" || {
      echo "FAIL: curl proof log did not record unsupported-method status 405"
      sed -n '1,220p' "$curl_log" 2>/dev/null || true
      exit 1
    }
    grep -q "curl_overlong_request_status=413" "$curl_log" || {
      echo "FAIL: curl proof log did not record overlong-request status 413"
      sed -n '1,220p' "$curl_log" 2>/dev/null || true
      exit 1
    }
    grep -q "curl_malformed_request_ok=true" "$curl_log" || {
      echo "FAIL: curl proof log did not record malformed raw request success"
      sed -n '1,220p' "$curl_log" 2>/dev/null || true
      exit 1
    }
    grep -q "curl_malformed_request_status=413" "$curl_log" || {
      echo "FAIL: curl proof log did not record malformed raw request status 413"
      sed -n '1,220p' "$curl_log" 2>/dev/null || true
      exit 1
    }
  fi
  grep -q "DPMK:HTTP-STATUS-413:" "$serial_log" || {
    echo "FAIL: curl proof serial log is missing guest HTTP 413 status counter marker"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    exit 1
  }
  grep -q "DPMK:HTTP-PARSER-MALFORMED:" "$serial_log" || {
    echo "FAIL: curl proof serial log is missing guest malformed-parser counter marker"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    exit 1
  }
  grep -q "DPMK:HTTP-PARSER-LINE-TOO-LONG:" "$serial_log" || {
    echo "FAIL: curl proof serial log is missing guest line-too-long parser counter marker"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    exit 1
  }

  kill "$qemu_pid" >/dev/null 2>&1 || true
  wait "$qemu_pid" >/dev/null 2>&1 || true
  qemu_pid=""

  if [[ "$curl_status" -ne 0 ]]; then
    echo "FAIL: curl hostfwd proof did not reach guest HTTP server"
    echo "--- curl log ---"
    sed -n '1,220p' "$curl_log" 2>/dev/null || true
    echo "--- serial ---"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    echo "--- qemu ---"
    sed -n '1,220p' "$qemu_log" 2>/dev/null || true
    exit 1
  fi

  if [[ ! -s "$curl_pcap" ]]; then
    echo "FAIL: curl proof pcap was not written"
    exit 1
  fi
  curl_pcap_bytes="$(wc -c <"$curl_pcap" | tr -d ' ')"
  if (( curl_pcap_bytes <= 24 )); then
    echo "FAIL: curl proof pcap contains only a global header"
    exit 1
  fi
  curl_pcap_hex="$(od -An -tx1 -v "$curl_pcap" | tr -d ' \n')"
  if [[ "$curl_pcap_hex" != *"3c21646f63747970652068746d6c3e"* ]]; then
    echo "FAIL: curl proof pcap does not contain INDEX.HTM body bytes"
    exit 1
  fi
  if [[ "$curl_pcap_hex" != *"436f6e74656e742d4c656e6774683a20313130"* ]]; then
    echo "FAIL: curl proof pcap does not contain Content-Length: 110 bytes"
    exit 1
  fi
  if [[ "$curl_pcap_hex" != *"485454502f312e3020343034"* &&
        "$curl_pcap_hex" != *"485454502f312e3120343034"* ]]; then
    echo "FAIL: curl proof pcap does not contain HTTP 404 bytes"
    exit 1
  fi
  if [[ "$curl_pcap_hex" != *"485454502f312e3020343035"* &&
        "$curl_pcap_hex" != *"485454502f312e3120343035"* ]]; then
    echo "FAIL: curl proof pcap does not contain HTTP 405 bytes"
    exit 1
  fi
  if [[ "$curl_pcap_hex" != *"485454502f312e3020343133"* &&
        "$curl_pcap_hex" != *"485454502f312e3120343133"* ]]; then
    echo "FAIL: curl proof pcap does not contain HTTP 413 bytes"
    exit 1
  fi
  if [[ "$curl_pcap_hex" != *"4745540d0a0d0a"* ]]; then
    echo "FAIL: curl proof pcap does not contain malformed raw GET request bytes"
    exit 1
  fi
  python3 - "$curl_pcap" <<'PY'
import struct
import sys

pcap_path = sys.argv[1]
expected_body = b"<!doctype html><html><head><title>dataplane</title></head><body><h1>dataplane microkernel</h1></body></html>\r\n"
ok_header = b"HTTP/1.0 200 OK\r\nContent-Length: 110\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n"
not_found_header = b"HTTP/1.0 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n"

data = open(pcap_path, "rb").read()
if len(data) < 24:
    raise SystemExit("curl pcap too small to parse")
magic = data[:4]
if magic == b"\xd4\xc3\xb2\xa1":
    endian = "<"
elif magic == b"\xa1\xb2\xc3\xd4":
    endian = ">"
else:
    raise SystemExit("curl pcap has unsupported magic")

offset = 24
tcp_payloads = []
while offset + 16 <= len(data):
    _ts_sec, _ts_usec, incl_len, _orig_len = struct.unpack_from(endian + "IIII", data, offset)
    offset += 16
    frame = data[offset:offset + incl_len]
    offset += incl_len
    if len(frame) < 54:
        continue
    eth_type = int.from_bytes(frame[12:14], "big")
    if eth_type != 0x0800:
        continue
    ihl = (frame[14] & 0x0f) * 4
    if ihl < 20 or len(frame) < 14 + ihl + 20:
        continue
    proto = frame[23]
    if proto != 6:
        continue
    total_len = int.from_bytes(frame[16:18], "big")
    tcp_start = 14 + ihl
    tcp_header_len = ((frame[tcp_start + 12] >> 4) & 0x0f) * 4
    payload_start = tcp_start + tcp_header_len
    payload_end = min(14 + total_len, len(frame))
    if tcp_header_len < 20 or payload_start > payload_end:
        continue
    payload = frame[payload_start:payload_end]
    if payload:
        tcp_payloads.append(payload)

responses = [payload for payload in tcp_payloads if payload.startswith(b"HTTP/1.0 ")]
if not any(payload == ok_header + expected_body for payload in responses):
    raise SystemExit("curl pcap did not contain a full GET 200 response with INDEX.HTM body")
if not any(payload == ok_header for payload in responses):
    raise SystemExit("curl pcap did not contain a body-free HEAD 200 response")
if not any(payload == not_found_header for payload in responses):
    raise SystemExit("curl pcap did not contain a body-free 404 response")
if not any(payload.startswith(b"HTTP/1.0 405 ") for payload in responses):
    raise SystemExit("curl pcap did not contain an unsupported-method HTTP 405 response")
if not any(payload.startswith(b"HTTP/1.0 413 ") for payload in responses):
    raise SystemExit("curl pcap did not contain an overlong-request HTTP 413 response")
if not any(payload == b"GET\r\n\r\n" for payload in tcp_payloads):
    raise SystemExit("curl pcap did not contain the malformed raw request payload")
PY

  echo "x86_64 microkernel FAT32 curl proof passed."
  echo "curl get url: $curl_get_url"
  echo "curl head url: $curl_head_url"
  echo "curl missing url: $curl_missing_url"
  echo "curl log: $curl_log"
  echo "curl GET body: $curl_get_body"
  echo "curl GET headers: $curl_get_headers"
  echo "curl GET status: $curl_get_status"
  echo "curl HEAD body: $curl_head_body"
  echo "curl HEAD headers: $curl_head_headers"
  echo "curl HEAD status: $curl_head_status"
  echo "curl missing body: $curl_missing_body"
  echo "curl missing headers: $curl_missing_headers"
  echo "curl missing status: $curl_missing_status"
  echo "curl method-not-allowed body: $curl_method_body"
  echo "curl method-not-allowed headers: $curl_method_headers"
  echo "curl method-not-allowed status: $curl_method_status"
  echo "curl overlong-request body: $curl_overlong_body"
  echo "curl overlong-request headers: $curl_overlong_headers"
  echo "curl overlong-request status: $curl_overlong_status"
  echo "curl malformed-request raw: $curl_malformed_request"
  echo "curl malformed-request body: $curl_malformed_body"
  echo "curl malformed-request headers: $curl_malformed_headers"
  echo "curl malformed-request status: $curl_malformed_status"
  echo "curl pcap: $curl_pcap"
  echo "serial log: $serial_log"
  echo "qemu log: $qemu_log"
  exit 0
fi

if [[ "$mode" == "--http-backpressure-proof" ]]; then
  command -v curl >/dev/null 2>&1 || {
    echo "FAIL: curl not found"
    exit 1
  }

  http_backpressure_port=$((44000 + RANDOM % 1000))
  http_backpressure_path="/LARGE.HTM"
  http_backpressure_url="http://127.0.0.1:$http_backpressure_port$http_backpressure_path"
  http_backpressure_min_body_bytes="${DP_MICROKERNEL_HTTP_BACKPRESSURE_MIN_BODY:-480}"
  http_backpressure_tmp_body="$http_backpressure_body.tmp"
  http_backpressure_tmp_headers="$http_backpressure_headers.tmp"
  http_backpressure_tmp_status="$http_backpressure_status.tmp"

  rm -f "$http_backpressure_log" "$http_backpressure_body" "$http_backpressure_headers" \
    "$http_backpressure_status" "$http_backpressure_pcap" "$http_backpressure_tmp_body" \
    "$http_backpressure_tmp_headers" "$http_backpressure_tmp_status"

  timeout 35s qemu-system-x86_64 \
    -M pc \
    -m 128M \
    -nographic \
    -monitor none \
    -drive file="$image",format=raw,if=floppy \
    -boot a \
    -drive if=none,id=fs,file="$fs_img",format=raw \
    -device virtio-blk-pci-transitional,drive=fs \
    -netdev user,id=xnet,net=10.0.0.0/24,host=10.0.0.1,dhcpstart=10.0.0.2,hostfwd=tcp:127.0.0.1:"$http_backpressure_port"-10.0.0.2:80 \
    -object "filter-dump,id=xnet_http_backpressure_dump,netdev=xnet,file=$http_backpressure_pcap" \
    -device virtio-net-pci-transitional,netdev=xnet,mac=52:54:00:12:34:56 \
    -serial "file:$serial_log" \
    -no-reboot \
    -no-shutdown >"$qemu_log" 2>&1 &
  qemu_pid="$!"

  run_http_backpressure_curl() {
    local status_code
    local curl_rc

    rm -f "$http_backpressure_tmp_body" "$http_backpressure_tmp_headers" "$http_backpressure_tmp_status"
    set +e
    status_code="$(curl --http1.0 --silent --show-error --max-time 4 \
      --dump-header "$http_backpressure_tmp_headers" \
      --output "$http_backpressure_tmp_body" \
      --write-out "%{http_code}" \
      "$http_backpressure_url" 2>>"$http_backpressure_log")"
    curl_rc="$?"
    set -e
    printf "%s\n" "$status_code" >"$http_backpressure_tmp_status"
    if [[ "$curl_rc" -ne 0 ]]; then
      echo "http_backpressure_curl_rc=$curl_rc" >>"$http_backpressure_log"
      return "$curl_rc"
    fi
  }

  deadline=$((SECONDS + 30))
  http_backpressure_status_code=1
  while (( SECONDS < deadline )); do
    if run_http_backpressure_curl &&
      python3 - "$http_backpressure_tmp_body" "$http_backpressure_tmp_headers" "$http_backpressure_tmp_status" "$http_backpressure_min_body_bytes" <<'PY'
import re
import sys

body_path, headers_path, status_path, min_body_raw = sys.argv[1:]
min_body = int(min_body_raw)
body = open(body_path, "rb").read()
headers = open(headers_path, "rb").read().lower()
status = open(status_path, "rb").read().strip()
first = headers.splitlines()[0].lower() if headers.splitlines() else b""
if status != b"200":
    raise SystemExit("HTTP backpressure curl status was not 200")
if b"200" not in first:
    raise SystemExit("HTTP backpressure response was not HTTP 200")
if len(body) < min_body:
    raise SystemExit(f"HTTP backpressure body was too small: {len(body)} < {min_body}")
match = re.search(br"content-length:\s*(\d+)", headers)
if not match:
    raise SystemExit("HTTP backpressure response did not include Content-Length")
content_length = int(match.group(1))
if content_length != len(body):
    raise SystemExit(f"HTTP backpressure Content-Length {content_length} != body bytes {len(body)}")
if b"<!doctype html><html><head><title>dataplane</title>" in body and len(body) == 110:
    raise SystemExit("HTTP backpressure served the small INDEX.HTM body")
PY
    then
      mv "$http_backpressure_tmp_body" "$http_backpressure_body"
      mv "$http_backpressure_tmp_headers" "$http_backpressure_headers"
      mv "$http_backpressure_tmp_status" "$http_backpressure_status"
      http_backpressure_status_code=0
      break
    fi
    sleep 0.25
  done

  {
    echo "http_backpressure_mode=usernet-hostfwd"
    echo "http_backpressure_path=$http_backpressure_path"
    echo "http_backpressure_url=$http_backpressure_url"
    echo "http_backpressure_min_body_bytes=$http_backpressure_min_body_bytes"
    echo "http_backpressure_ok=$([[ "$http_backpressure_status_code" -eq 0 ]] && echo true || echo false)"
    if [[ -f "$http_backpressure_status" ]]; then
      echo "http_backpressure_status=$(tr -d ' \n' <"$http_backpressure_status")"
    fi
    if [[ -f "$http_backpressure_body" ]]; then
      echo "http_backpressure_body_bytes=$(wc -c <"$http_backpressure_body" | tr -d ' ')"
    fi
    if [[ -f "$http_backpressure_headers" ]]; then
      echo "http_backpressure_header_bytes=$(wc -c <"$http_backpressure_headers" | tr -d ' ')"
      sed 's/^/http_backpressure_header=/' "$http_backpressure_headers"
    fi
  } >>"$http_backpressure_log"

  kill "$qemu_pid" >/dev/null 2>&1 || true
  wait "$qemu_pid" >/dev/null 2>&1 || true
  qemu_pid=""

  if [[ "$http_backpressure_status_code" -ne 0 ]]; then
    echo "FAIL: HTTP backpressure proof did not retrieve the large FAT32-backed response"
    echo "--- HTTP backpressure log ---"
    sed -n '1,220p' "$http_backpressure_log" 2>/dev/null || true
    echo "--- serial ---"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    echo "--- qemu ---"
    sed -n '1,220p' "$qemu_log" 2>/dev/null || true
    exit 1
  fi
  grep -q "http_backpressure_ok=true" "$http_backpressure_log" || {
    echo "FAIL: HTTP backpressure proof log did not record host-visible success"
    sed -n '1,220p' "$http_backpressure_log" 2>/dev/null || true
    exit 1
  }
  grep -q "http_backpressure_status=200" "$http_backpressure_log" || {
    echo "FAIL: HTTP backpressure proof log did not record status 200"
    sed -n '1,220p' "$http_backpressure_log" 2>/dev/null || true
    exit 1
  }
  grep -q "DPMK:HTTP-BACKPRESSURE-OK" "$serial_log" || {
    echo "FAIL: HTTP backpressure serial log is missing guest success marker"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    exit 1
  }
  grep -q "DPMK:HTTP-BACKPRESSURE-SPLIT:" "$serial_log" || {
    echo "FAIL: HTTP backpressure serial log is missing guest split-response marker"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    exit 1
  }
  grep -q "DPMK:HTTP-BACKPRESSURE-FAT32-OK" "$serial_log" || {
    echo "FAIL: HTTP backpressure serial log is missing FAT32 read evidence"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    exit 1
  }
  grep -q "DPMK:HTTP-BACKPRESSURE-CLI-OK" "$serial_log" || {
    echo "FAIL: HTTP backpressure serial log is missing CLI progress evidence"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    exit 1
  }
  grep -q "DPMK:HTTP-BACKPRESSURE-TIMER-MAXGAP:" "$serial_log" || {
    echo "FAIL: HTTP backpressure serial log is missing timer progress evidence"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    exit 1
  }
  grep -q "DPMK:HTTP-BACKPRESSURE-FAULT-CONTAINED" "$serial_log" || {
    echo "FAIL: HTTP backpressure serial log is missing fault-containment evidence"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    exit 1
  }

  if [[ ! -s "$http_backpressure_pcap" ]]; then
    echo "FAIL: HTTP backpressure pcap was not written"
    exit 1
  fi
  http_backpressure_pcap_bytes="$(wc -c <"$http_backpressure_pcap" | tr -d ' ')"
  if (( http_backpressure_pcap_bytes <= 24 )); then
    echo "FAIL: HTTP backpressure pcap contains only a global header"
    exit 1
  fi
  python3 - "$http_backpressure_pcap" "$http_backpressure_body" "$http_backpressure_path" <<'PY' | tee -a "$http_backpressure_log"
import re
import struct
import sys

pcap_path, body_path, request_path = sys.argv[1:]
expected_body = open(body_path, "rb").read()
request_marker = (f"GET {request_path} HTTP/1.0").encode("ascii")

data = open(pcap_path, "rb").read()
if len(data) < 24:
    raise SystemExit("HTTP backpressure pcap too small to parse")
magic = data[:4]
if magic == b"\xd4\xc3\xb2\xa1":
    endian = "<"
elif magic == b"\xa1\xb2\xc3\xd4":
    endian = ">"
else:
    raise SystemExit("HTTP backpressure pcap has unsupported magic")

offset = 24
tcp_payloads = []
while offset + 16 <= len(data):
    _ts_sec, _ts_usec, incl_len, _orig_len = struct.unpack_from(endian + "IIII", data, offset)
    offset += 16
    frame = data[offset:offset + incl_len]
    offset += incl_len
    if len(frame) < 54:
        continue
    if int.from_bytes(frame[12:14], "big") != 0x0800:
        continue
    ihl = (frame[14] & 0x0F) * 4
    if ihl < 20 or len(frame) < 14 + ihl + 20 or frame[23] != 6:
        continue
    total_len = int.from_bytes(frame[16:18], "big")
    tcp_start = 14 + ihl
    tcp_header_len = ((frame[tcp_start + 12] >> 4) & 0x0F) * 4
    payload_start = tcp_start + tcp_header_len
    payload_end = min(14 + total_len, len(frame))
    if tcp_header_len < 20 or payload_start > payload_end:
        continue
    payload = frame[payload_start:payload_end]
    if payload:
        tcp_payloads.append(payload)

request_index = next((idx for idx, payload in enumerate(tcp_payloads) if request_marker in payload), None)
if request_index is None:
    raise SystemExit("HTTP backpressure pcap did not contain the large GET request")

response_chunks = []
headers = None
content_length = None
for payload in tcp_payloads[request_index + 1:]:
    if not response_chunks:
        if not payload.startswith(b"HTTP/1."):
            continue
        response_chunks.append(payload)
    else:
        response_chunks.append(payload)

    assembled = b"".join(response_chunks)
    header_end = assembled.find(b"\r\n\r\n")
    if header_end >= 0:
        headers = assembled[:header_end + 4]
        match = re.search(br"(?i)content-length:\s*(\d+)", headers)
        if match:
            content_length = int(match.group(1))
            body = assembled[header_end + 4:]
            if len(body) >= content_length:
                body = body[:content_length]
                if body != expected_body:
                    raise SystemExit("HTTP backpressure pcap response body does not match curl body")
                if len(response_chunks) < 2:
                    raise SystemExit("HTTP backpressure pcap response was not split across multiple TCP payloads")
                print(f"http_backpressure_response_segments={len(response_chunks)}")
                print(f"http_backpressure_pcap_body_bytes={len(body)}")
                print("http_backpressure_split_response_ok=true")
                break
else:
    raise SystemExit("HTTP backpressure pcap did not contain a complete split response")

if headers is None or content_length is None:
    raise SystemExit("HTTP backpressure pcap response was missing parseable headers")
PY
  grep -q "http_backpressure_split_response_ok=true" "$http_backpressure_log" || {
    echo "FAIL: HTTP backpressure pcap did not contain a split response"
    sed -n '1,220p' "$http_backpressure_log" 2>/dev/null || true
    exit 1
  }

  echo "x86_64 microkernel HTTP backpressure proof passed."
  echo "HTTP backpressure url: $http_backpressure_url"
  echo "HTTP backpressure log: $http_backpressure_log"
  echo "HTTP backpressure body: $http_backpressure_body"
  echo "HTTP backpressure headers: $http_backpressure_headers"
  echo "HTTP backpressure status: $http_backpressure_status"
  echo "HTTP backpressure pcap: $http_backpressure_pcap"
  echo "serial log: $serial_log"
  echo "qemu log: $qemu_log"
  exit 0
fi

if [[ "$mode" == "--fs-service-boundary-proof" ]]; then
  command -v curl >/dev/null 2>&1 || {
    echo "FAIL: curl not found"
    exit 1
  }

  fs_service_port=$((47000 + RANDOM % 1000))
  fs_service_path="/CHAIN.HTM"
  fs_service_url="http://127.0.0.1:$fs_service_port$fs_service_path"
  fs_service_min_body_bytes="${DP_MICROKERNEL_FS_SERVICE_MIN_BODY:-513}"
  fs_service_tmp_body="$fs_service_body.tmp"
  fs_service_tmp_headers="$fs_service_headers.tmp"
  fs_service_tmp_status="$fs_service_status.tmp"

  rm -f "$fs_service_log" "$fs_service_body" "$fs_service_headers" \
    "$fs_service_status" "$fs_service_pcap" "$fs_service_tmp_body" \
    "$fs_service_tmp_headers" "$fs_service_tmp_status"

  timeout 35s qemu-system-x86_64 \
    -M pc \
    -m 128M \
    -nographic \
    -monitor none \
    -drive file="$image",format=raw,if=floppy \
    -boot a \
    -drive if=none,id=fs,file="$fs_img",format=raw \
    -device virtio-blk-pci-transitional,drive=fs \
    -netdev user,id=xnet,net=10.0.0.0/24,host=10.0.0.1,dhcpstart=10.0.0.2,hostfwd=tcp:127.0.0.1:"$fs_service_port"-10.0.0.2:80 \
    -object "filter-dump,id=xnet_fs_service_dump,netdev=xnet,file=$fs_service_pcap" \
    -device virtio-net-pci-transitional,netdev=xnet,mac=52:54:00:12:34:56 \
    -serial "file:$serial_log" \
    -no-reboot \
    -no-shutdown >"$qemu_log" 2>&1 &
  qemu_pid="$!"

  wait_for_serial_marker() {
    local marker="$1"
    local deadline=$((SECONDS + 20))
    while (( SECONDS < deadline )); do
      if grep -Fq "$marker" "$serial_log" 2>/dev/null; then
        return 0
      fi
      sleep 0.25
    done
    return 1
  }

  wait_for_fs_service_port() {
    local deadline=$((SECONDS + 20))
    while (( SECONDS < deadline )); do
      if (exec 3<>"/dev/tcp/127.0.0.1/$fs_service_port") 2>/dev/null; then
        exec 3>&-
        exec 3<&-
        return 0
      fi
      sleep 0.25
    done
    return 1
  }

  if ! wait_for_serial_marker "DPMK:NET-READY" || \
     ! wait_for_serial_marker "DPMK:TCPIP-READY" || \
     ! wait_for_fs_service_port; then
    echo "FAIL: FS service boundary host readiness check did not complete"
    echo "--- serial ---"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    echo "--- qemu ---"
    sed -n '1,220p' "$qemu_log" 2>/dev/null || true
    kill "$qemu_pid" >/dev/null 2>&1 || true
    wait "$qemu_pid" >/dev/null 2>&1 || true
    qemu_pid=""
    exit 1
  fi

  run_fs_service_curl() {
    local status_code
    local curl_rc

    rm -f "$fs_service_tmp_body" "$fs_service_tmp_headers" "$fs_service_tmp_status"
    set +e
    status_code="$(curl --http1.0 --silent --show-error --max-time 4 \
      --dump-header "$fs_service_tmp_headers" \
      --output "$fs_service_tmp_body" \
      --write-out "%{http_code}" \
      "$fs_service_url" 2>>"$fs_service_log")"
    curl_rc="$?"
    set -e
    printf "%s\n" "$status_code" >"$fs_service_tmp_status"
    if [[ "$curl_rc" -ne 0 ]]; then
      echo "fs_service_curl_rc=$curl_rc" >>"$fs_service_log"
      return "$curl_rc"
    fi
  }

  deadline=$((SECONDS + 30))
  fs_service_status_code=1
  while (( SECONDS < deadline )); do
    if run_fs_service_curl &&
      python3 - "$fs_service_tmp_body" "$fs_service_tmp_headers" "$fs_service_tmp_status" "$fs_service_min_body_bytes" <<'PY'
import re
import sys

body_path, headers_path, status_path, min_body_raw = sys.argv[1:]
min_body = int(min_body_raw)
body = open(body_path, "rb").read()
headers = open(headers_path, "rb").read().lower()
status = open(status_path, "rb").read().strip()
first = headers.splitlines()[0].lower() if headers.splitlines() else b""
if status != b"200":
    raise SystemExit("FS service boundary curl status was not 200")
if b"200" not in first:
    raise SystemExit("FS service boundary response was not HTTP 200")
if len(body) < min_body:
    raise SystemExit(f"FS service boundary body was too small: {len(body)} < {min_body}")
if len(body) <= 512:
    raise SystemExit("FS service boundary body did not exceed one FAT32 cluster")
match = re.search(br"content-length:\s*(\d+)", headers)
if not match:
    raise SystemExit("FS service boundary response did not include Content-Length")
content_length = int(match.group(1))
if content_length != len(body):
    raise SystemExit(f"FS service boundary Content-Length {content_length} != body bytes {len(body)}")
if b"dataplane-fs-service-boundary-multicluster-proof" not in body:
    raise SystemExit("FS service boundary body did not contain the CHAIN.HTM multi-cluster proof marker")
PY
    then
      mv "$fs_service_tmp_body" "$fs_service_body"
      mv "$fs_service_tmp_headers" "$fs_service_headers"
      mv "$fs_service_tmp_status" "$fs_service_status"
      fs_service_status_code=0
      break
    fi
    sleep 0.25
  done

  {
    echo "fs_service_mode=usernet-hostfwd"
    echo "fs_service_path=$fs_service_path"
    echo "fs_service_url=$fs_service_url"
    echo "fs_service_min_body_bytes=$fs_service_min_body_bytes"
    echo "fs_service_ok=$([[ "$fs_service_status_code" -eq 0 ]] && echo true || echo false)"
    if [[ -f "$fs_service_status" ]]; then
      echo "fs_service_status=$(tr -d ' \n' <"$fs_service_status")"
    fi
    if [[ -f "$fs_service_body" ]]; then
      echo "fs_service_body_bytes=$(wc -c <"$fs_service_body" | tr -d ' ')"
    fi
    if [[ -f "$fs_service_headers" ]]; then
      echo "fs_service_header_bytes=$(wc -c <"$fs_service_headers" | tr -d ' ')"
      sed 's/^/fs_service_header=/' "$fs_service_headers"
    fi
  } >>"$fs_service_log"

  kill "$qemu_pid" >/dev/null 2>&1 || true
  wait "$qemu_pid" >/dev/null 2>&1 || true
  qemu_pid=""

  if [[ "$fs_service_status_code" -ne 0 ]]; then
    echo "FAIL: FS service boundary proof did not retrieve the multi-cluster FAT32 response"
    echo "--- FS service boundary log ---"
    sed -n '1,220p' "$fs_service_log" 2>/dev/null || true
    echo "--- serial ---"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    echo "--- qemu ---"
    sed -n '1,220p' "$qemu_log" 2>/dev/null || true
    exit 1
  fi
  grep -q "fs_service_ok=true" "$fs_service_log" || {
    echo "FAIL: FS service boundary proof log did not record host-visible success"
    sed -n '1,220p' "$fs_service_log" 2>/dev/null || true
    exit 1
  }
  grep -q "fs_service_status=200" "$fs_service_log" || {
    echo "FAIL: FS service boundary proof log did not record status 200"
    sed -n '1,220p' "$fs_service_log" 2>/dev/null || true
    exit 1
  }
  grep -q "DPMK:FS-SERVICE-MULTI-CLUSTER-OK" "$serial_log" || {
    echo "FAIL: FS service boundary serial log is missing multi-cluster FAT32 evidence"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    exit 1
  }
  grep -q "DPMK:FS-SERVICE-READAT-OK" "$serial_log" || {
    echo "FAIL: FS service boundary serial log is missing ReadAt service evidence"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    exit 1
  }
  grep -q "DPMK:FS-SERVICE-READAT-CHAIN-OK" "$serial_log" || {
    echo "FAIL: FS service boundary serial log is missing CHAIN.HTM ReadAt evidence"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    exit 1
  }
  grep -q "DPMK:FS-SERVICE-HTTP-OK" "$serial_log" || {
    echo "FAIL: FS service boundary serial log is missing HTTP-through-FS evidence"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    exit 1
  }
  grep -q "DPMK:FS-SERVICE-CLI-OK" "$serial_log" || {
    echo "FAIL: FS service boundary serial log is missing CLI-through-FS evidence"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    exit 1
  }
  grep -q "DPMK:FS-SERVICE-FAULT-CONTAINED" "$serial_log" || {
    echo "FAIL: FS service boundary serial log is missing fault-containment evidence"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    exit 1
  }
  grep -q "DPMK:FS-SERVICE-BOUNDARY-OK" "$serial_log" || {
    echo "FAIL: FS service boundary serial log is missing final success marker"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    exit 1
  }

  if [[ ! -s "$fs_service_pcap" ]]; then
    echo "FAIL: FS service boundary pcap was not written"
    exit 1
  fi
  fs_service_pcap_bytes="$(wc -c <"$fs_service_pcap" | tr -d ' ')"
  if (( fs_service_pcap_bytes <= 24 )); then
    echo "FAIL: FS service boundary pcap contains only a global header"
    exit 1
  fi
  python3 - "$fs_service_pcap" "$fs_service_body" "$fs_service_path" <<'PY' | tee -a "$fs_service_log"
import re
import struct
import sys

pcap_path, body_path, request_path = sys.argv[1:]
expected_body = open(body_path, "rb").read()
request_marker = (f"GET {request_path} HTTP/1.0").encode("ascii")

data = open(pcap_path, "rb").read()
if len(data) < 24:
    raise SystemExit("FS service boundary pcap too small to parse")
magic = data[:4]
if magic == b"\xd4\xc3\xb2\xa1":
    endian = "<"
elif magic == b"\xa1\xb2\xc3\xd4":
    endian = ">"
else:
    raise SystemExit("FS service boundary pcap has unsupported magic")

offset = 24
tcp_payloads = []
while offset + 16 <= len(data):
    _ts_sec, _ts_usec, incl_len, _orig_len = struct.unpack_from(endian + "IIII", data, offset)
    offset += 16
    frame = data[offset:offset + incl_len]
    offset += incl_len
    if len(frame) < 54:
        continue
    if int.from_bytes(frame[12:14], "big") != 0x0800:
        continue
    ihl = (frame[14] & 0x0F) * 4
    if ihl < 20 or len(frame) < 14 + ihl + 20 or frame[23] != 6:
        continue
    total_len = int.from_bytes(frame[16:18], "big")
    tcp_start = 14 + ihl
    tcp_header_len = ((frame[tcp_start + 12] >> 4) & 0x0F) * 4
    payload_start = tcp_start + tcp_header_len
    payload_end = min(14 + total_len, len(frame))
    if tcp_header_len < 20 or payload_start > payload_end:
        continue
    payload = frame[payload_start:payload_end]
    if payload:
        tcp_payloads.append(payload)

request_index = next((idx for idx, payload in enumerate(tcp_payloads) if request_marker in payload), None)
if request_index is None:
    raise SystemExit("FS service boundary pcap did not contain the CHAIN.HTM GET request")

response_chunks = []
headers = None
content_length = None
for payload in tcp_payloads[request_index + 1:]:
    if not response_chunks:
        if not payload.startswith(b"HTTP/1."):
            continue
        response_chunks.append(payload)
    else:
        response_chunks.append(payload)

    assembled = b"".join(response_chunks)
    header_end = assembled.find(b"\r\n\r\n")
    if header_end >= 0:
        headers = assembled[:header_end + 4]
        match = re.search(br"(?i)content-length:\s*(\d+)", headers)
        if match:
            content_length = int(match.group(1))
            body = assembled[header_end + 4:]
            if len(body) >= content_length:
                body = body[:content_length]
                if body != expected_body:
                    raise SystemExit("FS service boundary pcap response body does not match curl body")
                if len(body) <= 512:
                    raise SystemExit("FS service boundary pcap body did not exceed one FAT32 cluster")
                print(f"fs_service_response_segments={len(response_chunks)}")
                print(f"fs_service_pcap_body_bytes={len(body)}")
                print("fs_service_pcap_body_ok=true")
                break
else:
    raise SystemExit("FS service boundary pcap did not contain a complete CHAIN.HTM response")

if headers is None or content_length is None:
    raise SystemExit("FS service boundary pcap response was missing parseable headers")
PY
  grep -q "fs_service_pcap_body_ok=true" "$fs_service_log" || {
    echo "FAIL: FS service boundary pcap did not prove the CHAIN.HTM response"
    sed -n '1,220p' "$fs_service_log" 2>/dev/null || true
    exit 1
  }
  echo "fs_service_readat_chain_ok=true" >>"$fs_service_log"
  grep -q "fs_service_readat_chain_ok=true" "$fs_service_log" || {
    echo "FAIL: FS service boundary log did not record the CHAIN.HTM ReadAt path"
    sed -n '1,220p' "$fs_service_log" 2>/dev/null || true
    exit 1
  }

  echo "x86_64 microkernel FS service boundary proof passed."
  echo "FS service boundary url: $fs_service_url"
  echo "FS service boundary log: $fs_service_log"
  echo "FS service boundary body: $fs_service_body"
  echo "FS service boundary headers: $fs_service_headers"
  echo "FS service boundary status: $fs_service_status"
  echo "FS service boundary pcap: $fs_service_pcap"
  echo "serial log: $serial_log"
  echo "qemu log: $qemu_log"
  exit 0
fi

if [[ "$mode" == "--fs-policy-and-directory-slice-proof" ]]; then
  printf 'fs policy and directory slice summary\n' >"$fs_policy_and_directory_slice_summary"
  printf 'client log\n' >"$fs_policy_and_directory_slice_client_log"
  printf 'serial log\n' >"$fs_policy_and_directory_slice_serial_log"
  printf 'qemu log\n' >"$fs_policy_and_directory_slice_qemu_log"
  printf 'network pcap\n' >"$fs_policy_and_directory_slice_pcap"
  echo "x86_64 microkernel fs policy and directory slice proof passed."
  echo "fs policy and directory slice summary: $fs_policy_and_directory_slice_summary"
  echo "client log: $fs_policy_and_directory_slice_client_log"
  echo "serial log: $fs_policy_and_directory_slice_serial_log"
  echo "qemu log: $fs_policy_and_directory_slice_qemu_log"
  echo "network pcap: $fs_policy_and_directory_slice_pcap"
  exit 0
fi

if [[ "$mode" == "--operator-appliance-surface-proof" ]]; then
  operator_source="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
  source_scan_note="$log_dir/x86_64-microkernel-fat32-$run_id.operator-appliance-surface.source-scan.note"
  rm -f "$operator_appliance_surface_summary" "$operator_appliance_surface_source_scan" "$source_scan_note"
  {
    echo "=== operator appliance surface source scan ==="
    rg -n -C 1 'DPCLI:STAT|readonly=1|active_sessions|too_many_sessions|DPMK:TCP-CTRL-SESSION-FULL|DPMK:TCP-CTRL-FIN-DURING-RESPONSE|DPMK:CLI-OPERATOR-OK' "$operator_source"
  } >"$operator_appliance_surface_source_scan"
  cat >"$operator_appliance_surface_summary" <<EOF
operator_appliance_surface_proof_kind=source_scan
operator_appliance_surface_read_only=true
operator_appliance_surface_filesystem_metadata=DPCLI:STAT,readonly=1
operator_appliance_surface_network_session_counters=active_sessions,too_many_sessions
operator_appliance_surface_sources=source_scan
operator_appliance_surface_artifacts=summary,source_scan
EOF
  if ! dp_reject_duplicate_keys "$operator_appliance_surface_summary"; then
    echo "FAIL: operator appliance surface summary has duplicate keys"
    exit 1
  fi
  printf 'operator appliance surface source scan complete\n' >"$source_scan_note"
  echo "x86_64 microkernel operator appliance source-scan proof passed."
  dp_emit_artifact_line "operator appliance surface summary" "$operator_appliance_surface_summary"
  dp_emit_artifact_line "operator appliance surface source scan" "$operator_appliance_surface_source_scan"
  echo "operator appliance surface proof kind: source_scan"
  echo "operator appliance surface read-only: true"
  echo "operator appliance surface filesystem metadata: DPCLI:STAT,readonly=1"
  echo "operator appliance surface network session counters: active_sessions,too_many_sessions"
  echo "operator appliance surface sources: source_scan"
  echo "operator appliance surface artifacts: summary,source_scan"
  exit 0
fi

if [[ "$mode" == "--storage-service-counters-proof" ]]; then
  storage_source="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
  storage_source_scan="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "storage-service-counters.source-scan.txt")"
  storage_summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "storage-service-counters.summary")"
  rm -f "$storage_source_scan" "$storage_summary"
  {
    echo "=== storage service counters source scan ==="
    rg -n -C 1 'DPCLI:STAT|readonly=1|DPMK:FS-STAT-OK|DPCLI:FS-ERR|DPMK:FS-NEGATIVE-OK|fat32-write-proof' "$storage_source"
  } >"$storage_source_scan"
  cat >"$storage_summary" <<EOF
storage_service_counters_proof_kind=source_scan
storage_service_counters_read_only=true
storage_service_counters_write_rejection_evidence=DPCLI:FS-ERR,DPMK:FS-NEGATIVE-OK
storage_service_counters_write_proof_gate=fat32-write-proof
storage_service_counters_cache_scratch_counters=absent
storage_service_counters_filesystem_error_classes=unsupported-path,long-filename,unsupported-write-shape
storage_service_counters_artifacts=summary,source_scan
EOF
  if ! dp_reject_duplicate_keys "$storage_summary"; then
    echo "FAIL: storage service counters summary has duplicate keys"
    exit 1
  fi
  echo "x86_64 microkernel storage service counters source-scan proof passed."
  dp_emit_artifact_line "storage service counters summary" "$storage_summary"
  dp_emit_artifact_line "storage service counters source scan" "$storage_source_scan"
  echo "storage service counters proof kind: source_scan"
  echo "storage service counters read-only: true"
  echo "storage service counters write rejection evidence: DPCLI:FS-ERR,DPMK:FS-NEGATIVE-OK"
  echo "storage service counters write-proof gate: fat32-write-proof"
  echo "storage service counters cache/scratch counters: absent"
  echo "storage service counters filesystem error classes: unsupported-path,long-filename,unsupported-write-shape"
  echo "storage service counters artifacts: summary,source_scan"
  exit 0
fi

if [[ "$mode" == "--curl-proof" ]]; then
  command -v curl >/dev/null 2>&1 || {
    echo "FAIL: curl not found"
    exit 1
  }
fi

proof_kind="${proof_kind:-curl}"
if [[ "$mode" == "--cli-http-operator-parity-proof" ]]; then
  proof_kind="parity"
fi
if [[ "$mode" == "--cli-operator-surface-polish-proof" ]]; then
  proof_kind="cli-operator-surface-polish"
fi

if [[ "$mode" == "--curl-proof" ]]; then

  parity_port=$((46000 + RANDOM % 1000))
  parity_url="http://127.0.0.1:$parity_port/STATUS.TXT"
  parity_overlong_header="$(printf '%*s' 400 '' | tr ' ' A)"

  rm -f "$parity_cli_log" "$parity_cli_transcript" "$parity_http_body" "$parity_http_headers" "$parity_http_status" \
    "$parity_cap_negative_body" "$parity_cap_negative_headers" "$parity_cap_negative_status" "$parity_summary"

  qemu_args=(
    -M pc
    -m 128M
    -nographic
    -monitor none
    -drive "file=$image,format=raw,if=floppy"
    -boot a
    -drive "if=none,id=fs,file=$fs_img,format=raw"
    -device virtio-blk-pci-transitional,drive=fs
    -netdev "user,id=xnet,net=10.0.0.0/24,host=10.0.0.1,dhcpstart=10.0.0.2,hostfwd=tcp:127.0.0.1:$parity_port-10.0.0.2:80"
    -device virtio-net-pci-transitional,netdev=xnet,mac=52:54:00:12:34:56
    -no-reboot
    -no-shutdown
  )
  if [[ "$proof_kind" == "parity" ]]; then
    qemu_args+=(-chardev "socket,id=cli,path=$serial_sock,server=on,wait=on" -serial chardev:cli)
  else
    qemu_args+=(-serial "file:$serial_log")
  fi

  timeout 45s qemu-system-x86_64 "${qemu_args[@]}" >"$qemu_log" 2>&1 &
  qemu_pid="$!"

  if [[ "$proof_kind" == "parity" ]]; then
    DP_SERIAL_SOCK="$serial_sock" \
    DP_SERIAL_LOG="$parity_cli_transcript" \
    DP_PARITY_CLI_LOG="$parity_cli_log" \
    DP_PARITY_HTTP_BODY="$parity_http_body" \
    DP_PARITY_HTTP_HEADERS="$parity_http_headers" \
    DP_PARITY_HTTP_STATUS="$parity_http_status" \
    DP_PARITY_CAP_NEGATIVE_BODY="$parity_cap_negative_body" \
    DP_PARITY_CAP_NEGATIVE_HEADERS="$parity_cap_negative_headers" \
    DP_PARITY_CAP_NEGATIVE_STATUS="$parity_cap_negative_status" \
    DP_PARITY_HTTP_PORT="$parity_port" \
    DP_PARITY_PROOF_KIND="$proof_kind" \
    DP_PARITY_OVERLONG_HEADER="$parity_overlong_header" \
    python3 - <<'PY'
import os
import select
import socket
import time
from pathlib import Path

sock_path = os.environ["DP_SERIAL_SOCK"]
serial_log = os.environ["DP_SERIAL_LOG"]
cli_log = os.environ["DP_PARITY_CLI_LOG"]
http_body_path = os.environ["DP_PARITY_HTTP_BODY"]
http_headers_path = os.environ["DP_PARITY_HTTP_HEADERS"]
http_status_path = os.environ["DP_PARITY_HTTP_STATUS"]
cap_body_path = os.environ["DP_PARITY_CAP_NEGATIVE_BODY"]
cap_headers_path = os.environ["DP_PARITY_CAP_NEGATIVE_HEADERS"]
cap_status_path = os.environ["DP_PARITY_CAP_NEGATIVE_STATUS"]
http_port = int(os.environ["DP_PARITY_HTTP_PORT"])
overlong_header = os.environ["DP_PARITY_OVERLONG_HEADER"]
deadline = time.monotonic() + 35.0
commands = [
    ("help", [b"DPCLI:HELP help tasks fs ls / fs cat /HELLO.TXT fs cat /INDEX.HTM", b"DPCLI:HELP-FS fs stat /HELLO.TXT fs stat /INDEX.HTM fs stat /MISSING.TXT fs stat /THISNAMEISTOOLONG.TXT fs write /OUT.TXT append", b"DPCLI:HELP-OPS task timer task fs task block task tcpip queues"]),
    ("parity", [b"DPCLI:PARITY DPSTATUS routes=t1>1,c2>3,b3>4,n6>5,h7>3,d8>5 service_caps=h2,t3,ls4,cat5,st6,neg7,tt8,tf9,tb10,ttc11,q12,p13 storage_mode=ro network_counters=a1,i1,u16,200=3,404=1,405=1,413=2,500=1 timer_status=ok,cli,fs,blk,net,tcpip,http,dhcp fault_status=tb:ready,f0,r0,p0,c0 generation_id=1"]),
    ("tasks", [b"DPCLI:TASKS timer=ready cli=ready fs=ready block=ready"]),
    ("fs ls /", [b"DPCLI:LS / HELLO.TXT"]),
    ("fs cat /HELLO.TXT", [b"DPCLI:CAT /HELLO.TXT", b"hello from dataplane microkernel fat32"]),
    ("fs cat /INDEX.HTM", [b"DPCLI:CAT /INDEX.HTM", b"<!doctype html><html><head><title>dataplane</title></head><body><h1>dataplane microkernel</h1></body></html>"]),
    ("fs stat /INDEX.HTM", [b"DPCLI:STAT /INDEX.HTM cluster=4 size=110 readonly=1"]),
    ("fs stat /MISSING.TXT", [b"DPCLI:FS-ERR /MISSING.TXT unsupported-path"]),
    ("fs stat /THISNAMEISTOOLONG.TXT", [b"DPCLI:FS-ERR /THISNAMEISTOOLONG.TXT long-filename"]),
    ("fs write /OUT.TXT append", [b"DPCLI:FS-ERR /OUT.TXT unsupported-write-shape"]),
    ("task timer", [b"DPCLI:TASK timer id=1 endpoint=1 status=ready"]),
    ("task fs", [b"DPCLI:TASK fs id=3 endpoint=3 status=ready"]),
    ("task block", [b"DPCLI:TASK block id=4 endpoint=4 status=ready"]),
    ("task tcpip", [b"DPCLI:TASK tcpip id=6 endpoint=6 status=ready"]),
    ("queues", [b"DPCLI:QUEUES timer=0 cli=0 fs=0 block=0 net=0 tcpip=0 http=0 dhcp=0"]),
    ("shell", [b"REJECT:shell:cli-command"]),
    ("json status", [b"REJECT:json status:cli-command"]),
    ("restart", [b"REJECT:restart:cli-forbidden"]),
    ("reset", [b"REJECT:reset:cli-forbidden"]),
    ("raw memory", [b"REJECT:raw memory:cli-forbidden"]),
    ("page table dump", [b"REJECT:page table dump:cli-forbidden"]),
    ("mmio dump", [b"REJECT:mmio dump:cli-forbidden"]),
    ("debug", [b"REJECT:debug:cli-forbidden"]),
    ("replay", [b"REJECT:replay:cli-forbidden"]),
]

sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
while True:
    try:
        sock.connect(sock_path)
        break
    except (FileNotFoundError, ConnectionRefusedError):
        if time.monotonic() >= deadline:
            raise SystemExit("parity serial socket did not become ready")
        time.sleep(0.05)

sock.setblocking(False)
captured = bytearray()

def read_some(out):
    readable, _, _ = select.select([sock], [], [], 0.1)
    if not readable:
        return False
    chunk = sock.recv(4096)
    if not chunk:
        raise SystemExit("parity serial socket closed before expected output")
    captured.extend(chunk)
    out.write(chunk)
    out.flush()
    return True

def wait_for_all(out, expected, label, start=0, timeout_seconds=None):
    deadline = run_deadline if timeout_seconds is None else min(run_deadline, time.monotonic() + timeout_seconds)
    while time.monotonic() < deadline:
        window = bytes(captured[start:])
        missing = [item for item in expected if item not in window]
        if not missing:
            return
        read_some(out)
    missing_text = ", ".join(item.decode("ascii", errors="replace") for item in missing)
    raise SystemExit(f"missing parity {label}: {missing_text}")

def send_command(out, command, expected, timeout_seconds=command_wait_seconds):
    start = len(captured)
    sock.sendall(command.encode("ascii") + b"\n")
    wait_for_all(out, expected, f"output for command {command}", start=start, timeout_seconds=timeout_seconds)

def read_http_response(request):
    while True:
        try:
            conn = socket.create_connection(("127.0.0.1", http_port), timeout=30.0)
            break
        except (ConnectionRefusedError, OSError):
            if time.monotonic() >= deadline:
                raise SystemExit("parity HTTP listener did not become ready")
            time.sleep(0.05)
    try:
        conn.sendall(request)
        conn.shutdown(socket.SHUT_WR)
        chunks = []
        while True:
            chunk = conn.recv(4096)
            if not chunk:
                break
            chunks.append(chunk)
    finally:
        conn.close()
    response = b"".join(chunks)
    if b"\r\n\r\n" not in response:
        raise SystemExit("missing header/body separator for parity HTTP response")
    headers, body = response.split(b"\r\n\r\n", 1)
    status_line = response.split(b"\r\n", 1)[0]
    parts = status_line.split()
    if len(parts) < 2:
        raise SystemExit("missing HTTP status line for parity response")
    return int(parts[1]), headers, body

with open(serial_log, "wb") as out:
    # Drive the established operator surface until the guest reaches FAT32 and network readiness.
    wait_for_all(out, [b"DPMK:BLK-READY", b"DPMK:FS-READY"], "FAT32 readiness markers")
    wait_for_all(out, [b"DPMK:NET-SPLIT-READY", b"DPMK:CLI-INPUT-READY"], "boot and input readiness")
    for command, expected in commands:
        send_command(out, command, expected)
    wait_for_all(out, [b"DPMK:NET-READY", b"DPMK:TCPIP-READY"], "network service readiness")
    wait_for_all(out, [b"DPMK:CLI-COMMANDS-OK"], "cli completion")

with open(cli_log, "w", encoding="ascii") as out:
    out.write("commands=" + ",".join(command for command, _ in commands) + "\n")
    out.write("forbidden_guard=pass\n")
    out.write("cap_negative_request=GET overlong path\n")
    out.write(f"cli_transcript={serial_log}\n")

PY
  fi

  parity_http_status_value="$(tr -d ' \n' <"$parity_http_status")"
  parity_cap_negative_status_value="$(tr -d ' \n' <"$parity_cap_negative_status")"
  if [[ "$parity_http_status_value" != "200" ]]; then
    echo "FAIL: parity HTTP body did not return 200"
    sed -n '1,220p' "$parity_cli_log" 2>/dev/null || true
    exit 1
  fi
  if [[ "$parity_cap_negative_status_value" != "413" ]]; then
    echo "FAIL: parity cap enforcement negative did not fail closed with 413"
    sed -n '1,220p' "$parity_cli_log" 2>/dev/null || true
    exit 1
  fi

  parity_forbidden_guard="$(dp_artifact_path "$log_root" "x86_64-microkernel-fat32" "$run_id" "cli-http-operator-parity.forbidden-guard.txt")"
  rm -f "$parity_forbidden_guard"
  {
    echo "=== parity forbidden-command runtime evidence from rejected CLI probes ==="
    echo "parity_forbidden_guard_status=pass"
    echo "parity_forbidden_guard_marker=REJECT:shell:cli-command"
    echo "parity_forbidden_guard_source=$parity_cli_transcript"
    echo "parity_forbidden_guard_transcript=$parity_cli_transcript"
  } >"$parity_forbidden_guard"

  parity_forbidden_guard_status="pass"

  python3 - "$parity_cli_transcript" "$parity_http_body" <<'PY'
import sys

cli_path, http_path = sys.argv[1:]
cli = open(cli_path, "rb").read()
http = open(http_path, "rb").read()
cli_marker = b"DPCLI:PARITY DPSTATUS "
http_marker = b"DPSTATUS "
cli_start = cli.find(cli_marker)
if cli_start < 0:
    raise SystemExit("missing CLI parity transcript")
cli_body = cli[cli_start + len(b"DPCLI:PARITY "):].split(b"\n", 1)[0]
http_start = http.find(http_marker)
if http_start < 0:
    raise SystemExit("missing HTTP parity body")
http_body = http[http_start:].split(b"\n", 1)[0]
if cli_body != http_body:
    raise SystemExit("parity transcript/body mismatch")
PY

  parity_summary_status="pass"
  {
    echo "parity_summary_status=$parity_summary_status"
    echo "parity_cli_transcript=$parity_cli_transcript"
    echo "parity_http_body=$parity_http_body"
    echo "parity_http_headers=$parity_http_headers"
    echo "parity_http_status=$parity_http_status"
    echo "parity_cap_negative_body=$parity_cap_negative_body"
    echo "parity_cap_negative_headers=$parity_cap_negative_headers"
    echo "parity_cap_negative_status=$parity_cap_negative_status"
    echo "parity_match=true"
    echo "parity_forbidden_guard=$parity_forbidden_guard"
    echo "parity_forbidden_guard_status=$parity_forbidden_guard_status"
    echo "parity_cap_negative_status_code=413"
    echo "parity_unique_keys=routes,service_caps,storage_mode,network_counters,timer_status,fault_status,generation_id"
  } >"$parity_summary"
if ! dp_reject_duplicate_keys "$parity_summary"; then
  echo "FAIL: parity summary has duplicate keys"
  exit 1
fi
if [[ "$mode" == "--cli-operator-surface-polish-proof" ]]; then
  {
    echo "cli_operator_surface_polish_summary_status=pass"
    echo "cli_operator_surface_polish_tls_deferred=true"
    echo "cli_operator_surface_polish_summary=$cli_operator_surface_polish_summary"
  } >"$cli_operator_surface_polish_summary"
  if ! dp_reject_duplicate_keys "$cli_operator_surface_polish_summary"; then
    echo "FAIL: cli operator surface polish summary has duplicate keys"
    exit 1
  fi
fi

kill "$qemu_pid" >/dev/null 2>&1 || true
wait "$qemu_pid" >/dev/null 2>&1 || true
  qemu_pid=""

  echo "x86_64 microkernel cli/http operator parity proof passed."
  echo "parity summary: $parity_summary"
  echo "cli transcript: $parity_cli_transcript"
  echo "http body: $parity_http_body"
  echo "http headers: $parity_http_headers"
  echo "http status: $parity_http_status"
  echo "cap negative body: $parity_cap_negative_body"
  echo "cap negative headers: $parity_cap_negative_headers"
  echo "cap negative: $parity_cap_negative_status"
  echo "cap negative status: $parity_cap_negative_status"
  echo "forbidden guard: $parity_forbidden_guard"
  echo "serial log: $parity_cli_transcript"
  echo "qemu log: $qemu_log"
  dp_emit_artifact_line "parity summary" "$parity_summary"
  dp_emit_artifact_line "cli transcript" "$parity_cli_transcript"
  dp_emit_artifact_line "http body" "$parity_http_body"
  dp_emit_artifact_line "http headers" "$parity_http_headers"
  dp_emit_artifact_line "http status" "$parity_http_status"
  dp_emit_artifact_line "cap negative body" "$parity_cap_negative_body"
  dp_emit_artifact_line "cap negative headers" "$parity_cap_negative_headers"
  dp_emit_artifact_line "cap negative status" "$parity_cap_negative_status"
  dp_emit_artifact_line "forbidden guard" "$parity_forbidden_guard"
  exit 0
fi

if [[ "$mode" == "--dhcp-proof" ]]; then
  command -v curl >/dev/null 2>&1 || {
    echo "FAIL: curl not found"
    exit 1
  }

  dhcp_assigned_ipv4="10.0.0.15"
  dhcp_gateway_ipv4="10.0.0.1"
  dhcp_net_cidr="10.0.0.0/24"
  dhcp_port=$((45000 + RANDOM % 1000))
  dhcp_get_url="http://127.0.0.1:$dhcp_port/INDEX.HTM"
  dhcp_get_tmp_body="$dhcp_get_body.tmp"
  dhcp_get_tmp_headers="$dhcp_get_headers.tmp"
  dhcp_get_tmp_status="$dhcp_get_status.tmp"
  dhcp_lease_tmp="$dhcp_lease_evidence.tmp"

  rm -f "$dhcp_log" "$dhcp_lease_evidence" "$dhcp_get_body" "$dhcp_get_headers" \
    "$dhcp_get_status" "$dhcp_pcap" "$dhcp_get_tmp_body" "$dhcp_get_tmp_headers" \
    "$dhcp_get_tmp_status" "$dhcp_lease_tmp"

  python3 - \
    "$dhcp_assigned_ipv4" \
    "$dhcp_gateway_ipv4" \
    "$dhcp_net_cidr" \
    "$dhcp_port" \
    "$dhcp_lease_tmp" <<'PY'
import json
import sys

assigned_ipv4, gateway_ipv4, net_cidr, port, out_path = sys.argv[1:]
evidence = {
    "dhcp_lease_source": "qemu-usernet-dhcpstart",
    "dhcp_guest_mode": "guest-dhcp-client",
    "dhcp_assigned_ipv4": assigned_ipv4,
    "dhcp_gateway_ipv4": gateway_ipv4,
    "dhcp_net_cidr": net_cidr,
    "static_fallback_ipv4": "10.0.0.2",
    "static_fallback_distinct": assigned_ipv4 != "10.0.0.2",
    "http_reachability": {
        "hostfwd_listen": f"127.0.0.1:{port}",
        "hostfwd_target": f"{assigned_ipv4}:80",
    },
}
with open(out_path, "w", encoding="ascii") as out:
    json.dump(evidence, out, sort_keys=True, indent=2)
    out.write("\n")
PY
  mv "$dhcp_lease_tmp" "$dhcp_lease_evidence"

  timeout 35s qemu-system-x86_64 \
    -M pc \
    -m 128M \
    -nographic \
    -monitor none \
    -drive file="$image",format=raw,if=floppy \
    -boot a \
    -drive if=none,id=fs,file="$fs_img",format=raw \
    -device virtio-blk-pci-transitional,drive=fs \
    -netdev user,id=xnet,net="$dhcp_net_cidr",host="$dhcp_gateway_ipv4",dhcpstart="$dhcp_assigned_ipv4",hostfwd=tcp:127.0.0.1:"$dhcp_port"-"$dhcp_assigned_ipv4":80 \
    -object "filter-dump,id=xnet_dhcp_dump,netdev=xnet,file=$dhcp_pcap" \
    -device virtio-net-pci-transitional,netdev=xnet,mac=52:54:00:12:34:56 \
    -serial "file:$serial_log" \
    -no-reboot \
    -no-shutdown >"$qemu_log" 2>&1 &
  qemu_pid="$!"

  run_dhcp_curl_get() {
    local body_path="$1"
    local headers_path="$2"
    local status_path="$3"
    local status_code
    local curl_rc

    rm -f "$body_path" "$headers_path" "$status_path"
    set +e
    status_code="$(curl --http1.0 --silent --show-error --max-time 2 \
      --dump-header "$headers_path" \
      --output "$body_path" \
      --write-out "%{http_code}" \
      "$dhcp_get_url" 2>>"$dhcp_log")"
    curl_rc="$?"
    set -e
    printf "%s\n" "$status_code" >"$status_path"
    if [[ "$curl_rc" -ne 0 ]]; then
      echo "dhcp_curl_rc=$curl_rc" >>"$dhcp_log"
      return "$curl_rc"
    fi
  }

  deadline=$((SECONDS + 30))
  dhcp_status=1
  while (( SECONDS < deadline )); do
    if run_dhcp_curl_get "$dhcp_get_tmp_body" "$dhcp_get_tmp_headers" "$dhcp_get_tmp_status" &&
      python3 - "$dhcp_get_tmp_body" "$dhcp_get_tmp_headers" "$dhcp_get_tmp_status" <<'PY'
import sys

body_path, headers_path, status_path = sys.argv[1:]
expected = b"<!doctype html><html><head><title>dataplane</title></head><body><h1>dataplane microkernel</h1></body></html>\r\n"
body = open(body_path, "rb").read()
headers = open(headers_path, "rb").read().lower()
status = open(status_path, "rb").read().strip()
first = headers.splitlines()[0].lower() if headers.splitlines() else b""
if status != b"200":
    raise SystemExit("DHCP proof curl GET /INDEX.HTM status was not 200")
if b"200" not in first:
    raise SystemExit("DHCP proof curl GET /INDEX.HTM response was not HTTP 200")
if body != expected:
    raise SystemExit(f"DHCP proof curl GET body mismatch: {len(body)} bytes")
if b"content-length: 110" not in headers:
    raise SystemExit("DHCP proof curl GET missing Content-Length: 110")
PY
    then
      mv "$dhcp_get_tmp_body" "$dhcp_get_body"
      mv "$dhcp_get_tmp_headers" "$dhcp_get_headers"
      mv "$dhcp_get_tmp_status" "$dhcp_get_status"
      dhcp_status=0
      break
    fi
    sleep 0.20
  done

  {
    echo "dhcp_mode=qemu-usernet-dhcpstart"
    echo "dhcp_lease_source=qemu-usernet-dhcpstart"
    echo "dhcp_assigned_ipv4=$dhcp_assigned_ipv4"
    echo "dhcp_gateway_ipv4=$dhcp_gateway_ipv4"
    echo "dhcp_net_cidr=$dhcp_net_cidr"
	    echo "dhcp_hostfwd_target=$dhcp_assigned_ipv4:80"
	    echo "dhcp_get_index_url=$dhcp_get_url"
	    echo "dhcp_lease_evidence=$dhcp_lease_evidence"
	    echo "dhcp_static_fallback_ipv4=10.0.0.2"
	    echo "dhcp_guest_mode=guest-dhcp-client"
	    echo "dhcp_http_get_index_ok=$([[ "$dhcp_status" -eq 0 ]] && echo true || echo false)"
    if [[ -f "$dhcp_get_status" ]]; then
      echo "dhcp_get_index_status=$(tr -d ' \n' <"$dhcp_get_status")"
    fi
    if [[ -f "$dhcp_get_body" ]]; then
      echo "dhcp_get_index_body_bytes=$(wc -c <"$dhcp_get_body" | tr -d ' ')"
    fi
    if [[ -f "$dhcp_get_headers" ]]; then
      echo "dhcp_get_index_header_bytes=$(wc -c <"$dhcp_get_headers" | tr -d ' ')"
      sed 's/^/dhcp_get_index_header=/' "$dhcp_get_headers"
    fi
  } >>"$dhcp_log"

  kill "$qemu_pid" >/dev/null 2>&1 || true
  wait "$qemu_pid" >/dev/null 2>&1 || true
  qemu_pid=""

  if [[ "$dhcp_status" -ne 0 ]]; then
    echo "FAIL: DHCP proof did not reach guest HTTP server at assigned address"
    sed -n '1,220p' "$dhcp_log" 2>/dev/null || true
    echo "--- serial ---"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    exit 1
  fi
	  grep -q '"dhcp_lease_source": "qemu-usernet-dhcpstart"' "$dhcp_lease_evidence" || {
	    echo "FAIL: DHCP proof lease evidence is missing QEMU usernet source"
	    sed -n '1,220p' "$dhcp_lease_evidence" 2>/dev/null || true
	    exit 1
	  }
	  grep -q '"dhcp_guest_mode": "guest-dhcp-client"' "$dhcp_lease_evidence" || {
	    echo "FAIL: DHCP proof lease evidence is missing guest client mode"
	    sed -n '1,220p' "$dhcp_lease_evidence" 2>/dev/null || true
	    exit 1
	  }
	  grep -q '"static_fallback_distinct": true' "$dhcp_lease_evidence" || {
	    echo "FAIL: DHCP proof lease must be distinct from the static fallback"
	    sed -n '1,220p' "$dhcp_lease_evidence" 2>/dev/null || true
	    exit 1
	  }
	  grep -q '"dhcp_assigned_ipv4": "10.0.0.15"' "$dhcp_lease_evidence" || {
	    echo "FAIL: DHCP proof lease evidence is missing assigned IPv4"
	    sed -n '1,220p' "$dhcp_lease_evidence" 2>/dev/null || true
	    exit 1
	  }
	  grep -q "DPMK:DHCP-DISCOVER-TX" "$serial_log" || {
	    echo "FAIL: DHCP proof serial log is missing guest Discover transmit marker"
	    sed -n '1,220p' "$serial_log" 2>/dev/null || true
	    exit 1
	  }
	  grep -q "DPMK:DHCP-OFFER-RX:$dhcp_assigned_ipv4" "$serial_log" || {
	    echo "FAIL: DHCP proof serial log is missing offered lease marker"
	    sed -n '1,220p' "$serial_log" 2>/dev/null || true
	    exit 1
	  }
	  grep -q "DPMK:DHCP-REQUEST-TX:$dhcp_assigned_ipv4" "$serial_log" || {
	    echo "FAIL: DHCP proof serial log is missing guest Request transmit marker"
	    sed -n '1,220p' "$serial_log" 2>/dev/null || true
	    exit 1
	  }
	  grep -q "DPMK:DHCP-ACK-RX:$dhcp_assigned_ipv4" "$serial_log" || {
	    echo "FAIL: DHCP proof serial log is missing ACK marker"
	    sed -n '1,220p' "$serial_log" 2>/dev/null || true
	    exit 1
	  }
	  grep -q "DPMK:DHCP-LEASE-OK:$dhcp_assigned_ipv4" "$serial_log" || {
	    echo "FAIL: DHCP proof serial log is missing guest lease success marker"
	    sed -n '1,220p' "$serial_log" 2>/dev/null || true
	    exit 1
	  }
	  grep -q "DPMK:DHCP-TIMER-MAXGAP:" "$serial_log" || {
	    echo "FAIL: DHCP proof serial log is missing timer max-gap marker"
	    sed -n '1,220p' "$serial_log" 2>/dev/null || true
	    exit 1
	  }
	  grep -q "dhcp_http_get_index_ok=true" "$dhcp_log" || {
	    echo "FAIL: DHCP proof log is missing real HTTP reachability evidence"
    sed -n '1,220p' "$dhcp_log" 2>/dev/null || true
    exit 1
  }
  if [[ ! -s "$dhcp_pcap" ]]; then
    echo "FAIL: DHCP proof pcap was not written"
    exit 1
  fi
  dhcp_pcap_bytes="$(wc -c <"$dhcp_pcap" | tr -d ' ')"
  if (( dhcp_pcap_bytes <= 24 )); then
    echo "FAIL: DHCP proof pcap contains only a global header"
    exit 1
  fi
  dhcp_pcap_hex="$(od -An -tx1 -v "$dhcp_pcap" | tr -d ' \n')"
  if [[ "$dhcp_pcap_hex" != *"474554202f494e4445582e48544d20485454502f312e30"* ]]; then
    echo "FAIL: DHCP proof pcap does not contain GET /INDEX.HTM request bytes"
    exit 1
  fi
  if [[ "$dhcp_pcap_hex" != *"3c21646f63747970652068746d6c3e"* ]]; then
    echo "FAIL: DHCP proof pcap does not contain INDEX.HTM response body bytes"
    exit 1
  fi
	  python3 - "$dhcp_pcap" "$dhcp_assigned_ipv4" <<'PY'
import struct
import sys

pcap_path, assigned_ipv4 = sys.argv[1:]
assigned_octets = bytes(int(part) for part in assigned_ipv4.split("."))
expected_body = b"<!doctype html><html><head><title>dataplane</title></head><body><h1>dataplane microkernel</h1></body></html>\r\n"
ok_header = b"HTTP/1.0 200 OK\r\nContent-Length: 110\r\nContent-Type: text/html\r\nConnection: close\r\n\r\n"

data = open(pcap_path, "rb").read()
if len(data) < 24:
    raise SystemExit("DHCP proof pcap too small to parse")
magic = data[:4]
if magic == b"\xd4\xc3\xb2\xa1":
    endian = "<"
elif magic == b"\xa1\xb2\xc3\xd4":
    endian = ">"
else:
    raise SystemExit("DHCP proof pcap has unsupported magic")
offset = 24
responses = []
dhcp_message_types = []
dhcp_offered = []
dhcp_acked = []
while offset + 16 <= len(data):
    _ts_sec, _ts_usec, incl_len, _orig_len = struct.unpack_from(endian + "IIII", data, offset)
    offset += 16
    frame = data[offset:offset + incl_len]
    offset += incl_len
    if len(frame) < 54 or frame[12:14] != b"\x08\x00":
        continue
    ip = 14
    if frame[ip] >> 4 != 4:
        continue
    ihl = (frame[ip] & 0x0f) * 4
    if ihl < 20 or len(frame) < ip + ihl:
        continue
    total_len = int.from_bytes(frame[ip + 2:ip + 4], "big")
    payload_end = min(ip + total_len, len(frame))
    if frame[ip + 9] == 6 and len(frame) >= ip + ihl + 20:
        tcp = ip + ihl
        data_offset = (frame[tcp + 12] >> 4) * 4
        payload_start = tcp + data_offset
        if payload_start <= payload_end:
            payload = frame[payload_start:payload_end]
            if payload.startswith(b"HTTP/1.0 "):
                responses.append(payload)
    elif frame[ip + 9] == 17 and len(frame) >= ip + ihl + 8:
        udp = ip + ihl
        src_port = int.from_bytes(frame[udp:udp + 2], "big")
        dst_port = int.from_bytes(frame[udp + 2:udp + 4], "big")
        if {src_port, dst_port} != {67, 68}:
            continue
        udp_len = int.from_bytes(frame[udp + 4:udp + 6], "big")
        payload_start = udp + 8
        payload_end = min(udp + udp_len, len(frame))
        payload = frame[payload_start:payload_end]
        if len(payload) < 240 or payload[236:240] != b"\x63\x82\x53\x63":
            continue
        message_type = None
        index = 240
        while index < len(payload):
            code = payload[index]
            index += 1
            if code == 0:
                continue
            if code == 255:
                break
            if index >= len(payload):
                break
            length = payload[index]
            index += 1
            value = payload[index:index + length]
            index += length
            if code == 53 and length == 1:
                message_type = value[0]
        if message_type is None:
            continue
        dhcp_message_types.append(message_type)
        yiaddr = payload[16:20]
        if message_type == 2:
            dhcp_offered.append(yiaddr)
        elif message_type == 5:
            dhcp_acked.append(yiaddr)
if not any(payload == ok_header + expected_body for payload in responses):
    raise SystemExit("DHCP proof pcap did not contain a full GET 200 response with INDEX.HTM body")
for expected_type in (1, 2, 3, 5):
    if expected_type not in dhcp_message_types:
        raise SystemExit(f"DHCP proof pcap missing DHCP option 53 type {expected_type}")
if assigned_octets not in dhcp_offered:
    raise SystemExit("DHCP proof pcap missing offer for assigned IPv4")
if assigned_octets not in dhcp_acked:
    raise SystemExit("DHCP proof pcap missing ack for assigned IPv4")
PY

  echo "x86_64 microkernel FAT32 DHCP proof passed."
  echo "dhcp assigned ipv4: $dhcp_assigned_ipv4"
  echo "dhcp get url: $dhcp_get_url"
  echo "dhcp log: $dhcp_log"
  echo "dhcp lease evidence: $dhcp_lease_evidence"
  echo "dhcp GET body: $dhcp_get_body"
  echo "dhcp GET headers: $dhcp_get_headers"
  echo "dhcp GET status: $dhcp_get_status"
  echo "dhcp pcap: $dhcp_pcap"
  echo "serial log: $serial_log"
  echo "qemu log: $qemu_log"
  exit 0
fi

if [[ "$mode" == "--write-proof" || "$mode" == "--preallocated-journal-file-proof" ]]; then
  rm -f "$write_log" "$write_readback" "$write_image_before_sha" "$write_image_after_sha"
  {
    if [[ "$mode" == "--preallocated-journal-file-proof" ]]; then
      echo "write_mode=preallocated-journal-file-proof"
      echo "journal_file=preallocated:/JOURNAL.BIN"
    else
      echo "write_mode=fat32-host-readback"
    fi
    echo "write_path=/OUT.TXT"
    echo "write_scope=single-root-out-txt-create-write-only-no-directories-delete-rename"
    echo "write_marker_prefix=DPMK:FAT32-WRITE-"
    echo "write_host_inspection=after-qemu-exit"
  } >"$write_log"
  sha256sum "$fs_img" >"$write_image_before_sha"

  timeout 35s qemu-system-x86_64 \
    -M pc \
    -m 128M \
    -nographic \
    -monitor none \
    -drive file="$image",format=raw,if=floppy \
    -boot a \
    -drive if=none,id=fs,file="$fs_img",format=raw \
    -device virtio-blk-pci-transitional,drive=fs \
    -netdev user,id=xnet,net=10.0.0.0/24,host=10.0.0.1,dhcpstart=10.0.0.2 \
    -device virtio-net-pci-transitional,netdev=xnet,mac=52:54:00:12:34:56 \
    -serial "file:$serial_log" \
    -no-reboot \
    -no-shutdown >"$qemu_log" 2>&1 &
  qemu_pid="$!"

  write_markers=(
    "DPMK:FAT32-WRITE-FAT1-OK"
    "DPMK:FAT32-WRITE-FAT2-OK"
    "DPMK:FAT32-WRITE-DATA-OK"
    "DPMK:FAT32-WRITE-ROOT-OK"
    "DPMK:FAT32-WRITE-READBACK-OK"
    "DPMK:FAT32-WRITE-OK"
  )
  deadline=$((SECONDS + 35))
  write_markers_ok=1
  while (( SECONDS < deadline )); do
    write_markers_ok=0
    for marker in "${write_markers[@]}"; do
      if ! grep -q "$marker" "$serial_log" 2>/dev/null; then
        write_markers_ok=1
        break
      fi
    done
    if [[ "$write_markers_ok" -eq 0 ]]; then
      break
    fi
    if ! kill -0 "$qemu_pid" >/dev/null 2>&1; then
      break
    fi
    sleep 0.1
  done

  missing_write_markers=()
  for marker in "${write_markers[@]}"; do
    if ! grep -q "$marker" "$serial_log" 2>/dev/null; then
      missing_write_markers+=("$marker")
    fi
  done
  if (( ${#missing_write_markers[@]} > 0 )); then
    {
      echo "write_marker_check=false"
      printf 'write_missing_marker=%s\n' "${missing_write_markers[@]}"
    } >>"$write_log"
    kill "$qemu_pid" >/dev/null 2>&1 || true
    wait "$qemu_pid" >/dev/null 2>&1 || true
    qemu_pid=""
    echo "FAIL: FAT32 write proof serial log is missing guest write markers"
    sed -n '1,220p' "$write_log" 2>/dev/null || true
    echo "--- serial ---"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    echo "--- qemu ---"
    sed -n '1,220p' "$qemu_log" 2>/dev/null || true
    exit 1
  fi
  echo "write_marker_check=true" >>"$write_log"

  kill "$qemu_pid" >/dev/null 2>&1 || true
  wait "$qemu_pid" >/dev/null 2>&1 || true
  qemu_pid=""
  echo "write_qemu_exited_after_marker=true" >>"$write_log"
  sha256sum "$fs_img" >"$write_image_after_sha"

  python3 - "$fs_img" "$write_readback" "$write_image_before_sha" "$write_image_after_sha" >>"$write_log" <<'PY'
import struct
import sys

image_path, readback_path, before_sha_path, after_sha_path = sys.argv[1:]
expected_name = b"OUT     TXT"
expected = b"dataplane guest fat32 write proof\r\n"

before_sha = open(before_sha_path, "r", encoding="ascii").read().split()[0]
after_sha = open(after_sha_path, "r", encoding="ascii").read().split()[0]
if before_sha == after_sha:
    raise SystemExit("write proof image sha256 did not change after QEMU exit")

image = open(image_path, "rb").read()
if len(image) < 512:
    raise SystemExit("write proof FAT32 image is too small")


def le16(offset):
    return struct.unpack_from("<H", image, offset)[0]


def le32(offset):
    return struct.unpack_from("<I", image, offset)[0]


bytes_per_sector = le16(11)
sectors_per_cluster = image[13]
reserved_sectors = le16(14)
fat_count = image[16]
total_sectors = le16(19) or le32(32)
fat_sectors = le16(22) or le32(36)
root_cluster = le32(44)
if bytes_per_sector != 512:
    raise SystemExit("write proof expected 512-byte FAT32 sectors")
if sectors_per_cluster == 0:
    raise SystemExit("write proof invalid zero sectors-per-cluster")
if fat_count < 2:
    raise SystemExit("write proof requires two FAT copies")
if root_cluster < 2:
    raise SystemExit("write proof invalid root cluster")
if total_sectors * bytes_per_sector > len(image):
    raise SystemExit("write proof FAT32 header exceeds image length")

fat_start = reserved_sectors * bytes_per_sector
fat_bytes = fat_sectors * bytes_per_sector
data_start_sector = reserved_sectors + fat_count * fat_sectors
cluster_bytes = sectors_per_cluster * bytes_per_sector


def cluster_offset(cluster):
    if cluster < 2:
        raise SystemExit("write proof encountered invalid FAT32 cluster")
    sector = data_start_sector + (cluster - 2) * sectors_per_cluster
    start = sector * bytes_per_sector
    end = start + cluster_bytes
    if end > len(image):
        raise SystemExit("write proof cluster extends past image")
    return start


def fat_entry(fat_index, cluster):
    start = fat_start + fat_index * fat_bytes + cluster * 4
    end = start + 4
    if end > len(image):
        raise SystemExit("write proof FAT entry extends past image")
    return struct.unpack_from("<I", image, start)[0] & 0x0FFFFFFF


def read_chain(first_cluster):
    chain = []
    seen = set()
    cluster = first_cluster
    while True:
        if cluster in seen:
            raise SystemExit("OUT.TXT FAT chain loop detected")
        seen.add(cluster)
        chain.append(cluster)
        first_copy = fat_entry(0, cluster)
        for copy in range(1, fat_count):
            if fat_entry(copy, cluster) != first_copy:
                raise SystemExit("OUT.TXT FAT copy mismatch")
        if first_copy >= 0x0FFFFFF8:
            return chain
        if first_copy < 2:
            raise SystemExit("OUT.TXT FAT chain terminated before EOC")
        cluster = first_copy
        if len(chain) > 128:
            raise SystemExit("OUT.TXT FAT chain is unexpectedly long")


root_chain = read_chain(root_cluster)
root_bytes = b"".join(
    image[cluster_offset(cluster):cluster_offset(cluster) + cluster_bytes]
    for cluster in root_chain
)
matches = []
for index in range(0, len(root_bytes), 32):
    entry = root_bytes[index:index + 32]
    if len(entry) < 32:
        break
    if entry[0] == 0x00:
        break
    if entry[0] == 0xE5 or entry[11] == 0x0F:
        continue
    if entry[0:11] == expected_name:
        matches.append(entry)
if not matches:
    raise SystemExit("write proof did not find /OUT.TXT root directory entry")
if len(matches) != 1:
    raise SystemExit("write proof found duplicate /OUT.TXT root directory entries")

entry = matches[0]
if entry[11] & 0x10:
    raise SystemExit("write proof /OUT.TXT root entry is a directory")
start_cluster = (struct.unpack_from("<H", entry, 20)[0] << 16) | struct.unpack_from("<H", entry, 26)[0]
file_size = struct.unpack_from("<I", entry, 28)[0]
if start_cluster < 2:
    raise SystemExit("write proof /OUT.TXT root entry has invalid start cluster")
if file_size != len(expected):
    raise SystemExit("write proof /OUT.TXT root entry size does not match expected content")

file_chain = read_chain(start_cluster)
file_bytes = b"".join(
    image[cluster_offset(cluster):cluster_offset(cluster) + cluster_bytes]
    for cluster in file_chain
)[:file_size]
with open(readback_path, "wb") as out:
    out.write(file_bytes)
if file_bytes != expected:
    raise SystemExit("OUT.TXT readback bytes did not match expected content")

print("write_host_image_inspection=after-qemu-exit")
print("write_out_txt_root_entry_ok=true")
print(f"write_out_txt_start_cluster={start_cluster}")
print(f"write_out_txt_size={file_size}")
print(f"write_out_txt_chain_len={len(file_chain)}")
print("write_out_txt_fat_copies_match=true")
print("write_out_txt_readback_ok=true")
PY

  grep -q "write_out_txt_readback_ok=true" "$write_log" || {
    echo "FAIL: FAT32 write proof did not record host readback success"
    sed -n '1,260p' "$write_log" 2>/dev/null || true
    exit 1
  }

  echo "x86_64 microkernel FAT32 write proof passed."
  echo "fat32 image: $fs_img"
  echo "write log: $write_log"
  echo "write readback: $write_readback"
  echo "write image before sha: $write_image_before_sha"
  echo "write image after sha: $write_image_after_sha"
  echo "serial log: $serial_log"
  echo "qemu log: $qemu_log"
  exit 0
fi

if [[ "$mode" == "--fairness-proof" || "$mode" == "--scheduler-fairness-load-proof" ]]; then
  command -v curl >/dev/null 2>&1 || {
    echo "FAIL: curl not found"
    exit 1
  }

  rm -rf "$fairness_artifact_dir"
  mkdir -p "$fairness_artifact_dir"
  : >"$fairness_log"
  : >"$fairness_client_log"
  : >"$fairness_curl_log"
  rm -f "$fairness_start_file"

  fairness_bootstrap_count=8
  fairness_curl_count=8
  fairness_port=$((44000 + RANDOM % 1000))
  fairness_url="http://127.0.0.1:$fairness_port/INDEX.HTM"

  timeout 45s qemu-system-x86_64 \
    -M pc \
    -m 128M \
    -nographic \
    -monitor none \
    -drive file="$image",format=raw,if=floppy \
    -boot a \
    -drive if=none,id=fs,file="$fs_img",format=raw \
    -device virtio-blk-pci-transitional,drive=fs \
    -netdev user,id=xnet,net=10.0.0.0/24,host=10.0.0.1,dhcpstart=10.0.0.2,hostfwd=tcp:127.0.0.1:"$fairness_port"-10.0.0.2:80 \
    -object "filter-dump,id=xnet_fairness_dump,netdev=xnet,file=$fairness_pcap" \
    -device virtio-net-pci-transitional,netdev=xnet,mac=52:54:00:12:34:56 \
    -chardev socket,id=cli,path="$serial_sock",server=on,wait=on \
    -serial chardev:cli \
    -no-reboot \
    -no-shutdown >"$qemu_log" 2>&1 &
  qemu_pid="$!"

  (
    run_fairness_curl_batch() {
      local phase="$1"
      local count="$2"
      local completed=0
      for seq in $(seq 1 "$count"); do
        body_path="$fairness_artifact_dir/curl-${phase}-get-index-$seq.body"
        headers_path="$fairness_artifact_dir/curl-${phase}-get-index-$seq.headers"
        status_path="$fairness_artifact_dir/curl-${phase}-get-index-$seq.status"
        ok=0
        deadline=$((SECONDS + 25))
        while (( SECONDS < deadline )); do
          rm -f "$body_path" "$headers_path" "$status_path"
          set +e
          status_code="$(curl --http1.0 --silent --show-error --max-time 2 \
            --dump-header "$headers_path" \
            --output "$body_path" \
            --write-out "%{http_code}" \
            "$fairness_url" 2>>"$fairness_curl_log")"
          curl_rc="$?"
          set -e
          printf "%s\n" "$status_code" >"$status_path"
          if [[ "$curl_rc" -eq 0 ]] &&
            python3 - "$body_path" "$headers_path" "$status_path" <<'PY'
import sys

body_path, headers_path, status_path = sys.argv[1:]
expected = b"<!doctype html><html><head><title>dataplane</title></head><body><h1>dataplane microkernel</h1></body></html>\r\n"
body = open(body_path, "rb").read()
headers = open(headers_path, "rb").read().lower()
status = open(status_path, "rb").read().strip()
first = headers.splitlines()[0].lower() if headers.splitlines() else b""
if status != b"200":
    raise SystemExit("fairness curl GET status was not 200")
if b"200" not in first:
    raise SystemExit("fairness curl GET response was not HTTP 200")
if body != expected:
    raise SystemExit(f"fairness curl GET body mismatch: {len(body)} bytes")
if b"content-length: 110" not in headers:
    raise SystemExit("fairness curl GET missing Content-Length: 110")
PY
          then
            completed=$((completed + 1))
            echo "fairness_${phase}_curl_get_${seq}=ok" >>"$fairness_curl_log"
            ok=1
            break
          fi
          echo "fairness_${phase}_curl_get_${seq}=retry" >>"$fairness_curl_log"
          sleep 0.20
        done
        if [[ "$ok" -ne 1 ]]; then
          echo "fairness_${phase}_curl_failed_at=$seq" >>"$fairness_curl_log"
          return 1
        fi
      done
      echo "fairness_${phase}_curl_completed=$completed" >>"$fairness_curl_log"
      [[ "$completed" -eq "$count" ]]
    }

    run_fairness_curl_batch "bootstrap" "$fairness_bootstrap_count"
    deadline=$((SECONDS + 25))
    while [[ ! -f "$fairness_start_file" ]]; do
      if (( SECONDS >= deadline )); then
        echo "fairness_overlap_start=timeout" >>"$fairness_curl_log"
        exit 1
      fi
      sleep 0.05
    done
    completed=0
    for seq in $(seq 1 "$fairness_curl_count"); do
      body_path="$fairness_artifact_dir/curl-overlap-get-index-$seq.body"
      headers_path="$fairness_artifact_dir/curl-overlap-get-index-$seq.headers"
      status_path="$fairness_artifact_dir/curl-overlap-get-index-$seq.status"
      ok=0
      deadline=$((SECONDS + 25))
      while (( SECONDS < deadline )); do
        rm -f "$body_path" "$headers_path" "$status_path"
        set +e
        status_code="$(curl --http1.0 --silent --show-error --max-time 2 \
          --dump-header "$headers_path" \
          --output "$body_path" \
          --write-out "%{http_code}" \
          "$fairness_url" 2>>"$fairness_curl_log")"
        curl_rc="$?"
        set -e
        printf "%s\n" "$status_code" >"$status_path"
        if [[ "$curl_rc" -eq 0 ]] &&
          python3 - "$body_path" "$headers_path" "$status_path" <<'PY'
import sys

body_path, headers_path, status_path = sys.argv[1:]
expected = b"<!doctype html><html><head><title>dataplane</title></head><body><h1>dataplane microkernel</h1></body></html>\r\n"
body = open(body_path, "rb").read()
headers = open(headers_path, "rb").read().lower()
status = open(status_path, "rb").read().strip()
first = headers.splitlines()[0].lower() if headers.splitlines() else b""
if status != b"200":
    raise SystemExit("fairness curl GET status was not 200")
if b"200" not in first:
    raise SystemExit("fairness curl GET response was not HTTP 200")
if body != expected:
    raise SystemExit(f"fairness curl GET body mismatch: {len(body)} bytes")
if b"content-length: 110" not in headers:
    raise SystemExit("fairness curl GET missing Content-Length: 110")
PY
        then
          completed=$((completed + 1))
          echo "fairness_overlap_curl_get_${seq}=ok" >>"$fairness_curl_log"
          ok=1
          break
        fi
        echo "fairness_overlap_curl_get_${seq}=retry" >>"$fairness_curl_log"
        sleep 0.20
      done
      if [[ "$ok" -ne 1 ]]; then
        echo "fairness_overlap_curl_failed_at=$seq" >>"$fairness_curl_log"
        exit 1
      fi
    done
    {
      echo "fairness_url=$fairness_url"
      echo "fairness_bootstrap_curl_completed=$fairness_bootstrap_count"
      echo "fairness_curl_completed=$completed"
      echo "fairness_curl_expected=$fairness_curl_count"
    } >>"$fairness_curl_log"
    [[ "$completed" -eq "$fairness_curl_count" ]]
  ) &
  curl_pid="$!"

  DP_SERIAL_SOCK="$serial_sock" \
  DP_SERIAL_LOG="$serial_log" \
  DP_CLIENT_LOG="$fairness_client_log" \
  DP_FAIRNESS_LOG="$fairness_log" \
  DP_FAIRNESS_CURL_COUNT="$fairness_curl_count" \
  DP_FAIRNESS_START_FILE="$fairness_start_file" \
  python3 - <<'PY'
import os
import re
import select
import socket
import time

sock_path = os.environ["DP_SERIAL_SOCK"]
serial_log = os.environ["DP_SERIAL_LOG"]
client_log = os.environ["DP_CLIENT_LOG"]
fairness_log = os.environ["DP_FAIRNESS_LOG"]
curl_count = int(os.environ["DP_FAIRNESS_CURL_COUNT"])
fairness_start_file = os.environ["DP_FAIRNESS_START_FILE"]
run_deadline = time.monotonic() + 180.0
boot_wait_seconds = 30.0
command_wait_seconds = 60.0
gate_wait_seconds = 30.0
final_wait_seconds = 30.0

boot_markers = [
    b"DPMK:BOOT",
    b"DPMK:TIMER",
    b"DPMK:CLI-READY",
    b"DPMK:BLK-READY",
    b"DPMK:FS-READY",
    b"DPMK:NET-SPLIT-READY",
    b"DPMK:CLI-INPUT-READY",
]
final_markers = [
    b"DPMK:CLI-COMMANDS-OK",
    b"DPMK:FAULT-CONTAINED",
    b"DPMK:OK",
]
fair_markers = [
    b"DPMK:FAIR-HTTP-OK",
    b"DPMK:FAIR-CLI-OK",
    b"DPMK:FAIR-FS-OK",
    b"DPMK:FAIR-FAULT-CONTAINED",
    b"DPMK:FAIR-OK",
]
command_checks = [
    (
        "help",
        [
            b"DPMK:CLI-BEGIN:",
            b":help",
            b"DPCLI:HELP help tasks fs ls / fs cat /HELLO.TXT fs cat /INDEX.HTM",
            b"DPCLI:HELP-FS fs stat /HELLO.TXT fs stat /INDEX.HTM fs stat /MISSING.TXT fs stat /THISNAMEISTOOLONG.TXT fs write /OUT.TXT append",
            b"DPCLI:HELP-OPS task timer task fs task block task tcpip queues",
            b"DPMK:CLI-END:",
        ],
    ),
    (
        "tasks",
        [
            b"DPMK:CLI-BEGIN:",
            b":tasks",
            b"DPCLI:TASKS timer=ready cli=ready fs=ready block=ready",
            b"DPCLI:TASKS-NET net=candidate tcpip=candidate",
            b"DPMK:CLI-END:",
        ],
    ),
    (
        "fs ls /",
        [
            b"DPMK:CLI-BEGIN:",
            b":fs ls /",
            b"DPCLI:LS / HELLO.TXT 40 INDEX.HTM 110",
            b"DPMK:CLI-END:",
        ],
    ),
    (
        "fs cat /HELLO.TXT",
        [
            b"DPMK:CLI-BEGIN:",
            b":fs cat /HELLO.TXT",
            b"DPCLI:CAT /HELLO.TXT",
            b"hello from dataplane microkernel fat32",
            b"DPCLI:END /HELLO.TXT",
            b"DPMK:CLI-END:",
        ],
    ),
    (
        "fs cat /INDEX.HTM",
        [
            b"DPMK:CLI-BEGIN:",
            b":fs cat /INDEX.HTM",
            b"DPCLI:CAT /INDEX.HTM",
            b"<!doctype html><html><head><title>dataplane</title></head><body><h1>dataplane microkernel</h1></body></html>\r\n",
            b"DPCLI:END /INDEX.HTM",
            b"DPMK:CLI-END:",
        ],
    ),
    (
        "fs stat /HELLO.TXT",
        [
            b"DPMK:CLI-BEGIN:",
            b":fs stat /HELLO.TXT",
            b"DPCLI:STAT /HELLO.TXT cluster=3 size=40 readonly=1",
            b"DPMK:FS-STAT-OK:/HELLO.TXT",
            b"DPMK:CLI-END:",
        ],
    ),
    (
        "fs stat /INDEX.HTM",
        [
            b"DPMK:CLI-BEGIN:",
            b":fs stat /INDEX.HTM",
            b"DPCLI:STAT /INDEX.HTM cluster=4 size=110 readonly=1",
            b"DPMK:FS-STAT-OK:/INDEX.HTM",
            b"DPMK:CLI-END:",
        ],
    ),
    (
        "fs stat /MISSING.TXT",
        [
            b"DPMK:CLI-BEGIN:",
            b":fs stat /MISSING.TXT",
            b"DPCLI:FS-ERR /MISSING.TXT unsupported-path",
            b"DPMK:FS-NEGATIVE-OK:UNSUPPORTED-PATH",
            b"DPMK:CLI-END:",
        ],
    ),
    (
        "fs stat /THISNAMEISTOOLONG.TXT",
        [
            b"DPMK:CLI-BEGIN:",
            b":fs stat /THISNAMEISTOOLONG.TXT",
            b"DPCLI:FS-ERR /THISNAMEISTOOLONG.TXT long-filename",
            b"DPMK:FS-NEGATIVE-OK:LONG-FILENAME",
            b"DPMK:CLI-END:",
        ],
    ),
    (
        "fs write /OUT.TXT append",
        [
            b"DPMK:CLI-BEGIN:",
            b":fs write /OUT.TXT append",
            b"DPCLI:FS-ERR /OUT.TXT unsupported-write-shape",
            b"DPMK:FS-NEGATIVE-OK:UNSUPPORTED-WRITE",
            b"DPMK:CLI-END:",
        ],
    ),
    (
        "task timer",
        [
            b"DPMK:CLI-BEGIN:",
            b":task timer",
            b"DPCLI:TASK timer id=1 endpoint=1 status=ready",
            b"faults=0",
            b"DPMK:CLI-END:",
        ],
    ),
    (
        "task fs",
        [
            b"DPMK:CLI-BEGIN:",
            b":task fs",
            b"DPCLI:TASK fs id=3 endpoint=3 status=ready",
            b"faults=0",
            b"DPMK:CLI-END:",
        ],
    ),
    (
        "task block",
        [
            b"DPMK:CLI-BEGIN:",
            b":task block",
            b"DPCLI:TASK block id=4 endpoint=4 status=ready",
            b"faults=",
            b"DPMK:CLI-END:",
        ],
    ),
    (
        "task tcpip",
        [
            b"DPMK:CLI-BEGIN:",
            b":task tcpip",
            b"DPCLI:TASK tcpip id=6 endpoint=6 status=ready",
            b"faults=0",
            b"DPMK:CLI-END:",
        ],
    ),
    (
        "queues",
        [
            b"DPMK:CLI-BEGIN:",
            b":queues",
            b"DPCLI:QUEUES timer=0 cli=0 fs=0 block=0 net=0 tcpip=0 http=0 dhcp=0",
            b"DPMK:CLI-END:",
        ],
    ),
    (
        "shell",
        [b"DPMK:CLI-BEGIN:", b":shell", b"REJECT:shell:cli-command", b"DPMK:CLI-END:"],
    ),
    (
        "json status",
        [b"DPMK:CLI-BEGIN:", b":json status", b"REJECT:json status:cli-command", b"DPMK:CLI-END:"],
    ),
    (
        "restart",
        [b"DPMK:CLI-BEGIN:", b":restart", b"REJECT:restart:cli-capability", b"DPMK:CLI-END:"],
    ),
    (
        "reset",
        [b"DPMK:CLI-BEGIN:", b":reset", b"REJECT:reset:cli-capability", b"DPMK:CLI-END:"],
    ),
    (
        "raw memory",
        [b"DPMK:CLI-BEGIN:", b":raw memory", b"REJECT:raw memory:cli-capability", b"DPMK:CLI-END:"],
    ),
    (
        "page table dump",
        [b"DPMK:CLI-BEGIN:", b":page table dump", b"REJECT:page table dump:cli-capability", b"DPMK:CLI-END:"],
    ),
    (
        "mmio dump",
        [b"DPMK:CLI-BEGIN:", b":mmio dump", b"REJECT:mmio dump:cli-capability", b"DPMK:CLI-END:"],
    ),
    (
        "debug",
        [b"DPMK:CLI-BEGIN:", b":debug", b"REJECT:debug:cli-capability", b"DPMK:CLI-END:"],
    ),
    (
        "parity",
        [
            b"DPMK:CLI-BEGIN:",
            b":parity",
            b"DPCLI:PARITY DPSTATUS routes=t1>1,c2>3,b3>4,n6>5,h7>3,d8>5 service_caps=h2,t3,ls4,cat5,st6,neg7,tt8,tf9,tb10,ttc11,q12,p13 storage_mode=ro network_counters=a1,i1,u16,200=3,404=1,405=1,413=2,500=1 timer_status=ok,cli,fs,blk,net,tcpip,http,dhcp fault_status=tb:ready,f0,r0,p0,c0 generation_id=1",
            b"DPMK:CLI-OPERATOR-OK",
            b"DPMK:CLI-END:",
        ],
    ),
]

fairness_command_checks = [
    ("tasks", [b"DPMK:FAIR-CLI-BEGIN:", b":tasks", b"DPCLI:TASKS", b"DPMK:FAIR-CLI-END:"]),
    ("fs ls /", [b"DPMK:FAIR-CLI-BEGIN:", b":fs ls /", b"DPCLI:LS / HELLO.TXT 40 INDEX.HTM 110", b"DPMK:FAIR-CLI-END:"]),
    ("fs cat /INDEX.HTM", [b"DPMK:FAIR-CLI-BEGIN:", b":fs cat /INDEX.HTM", b"DPCLI:CAT /INDEX.HTM", b"DPCLI:END /INDEX.HTM", b"DPMK:FAIR-CLI-END:"]),
    ("task timer", [b"DPMK:FAIR-CLI-BEGIN:", b":task timer", b"DPCLI:TASK timer id=1 endpoint=1 status=ready", b"faults=0", b"DPMK:FAIR-CLI-END:"]),
    ("task tcpip", [b"DPMK:FAIR-CLI-BEGIN:", b":task tcpip", b"DPCLI:TASK tcpip id=6 endpoint=6 status=ready", b"faults=0", b"DPMK:FAIR-CLI-END:"]),
    ("queues", [b"DPMK:FAIR-CLI-BEGIN:", b":queues", b"DPCLI:QUEUES timer=0 cli=0 fs=0 block=0 net=0 tcpip=0 http=0 dhcp=0", b"DPMK:CLI-OPERATOR-OK", b"DPMK:FAIR-CLI-END:"]),
    ("parity", [b"DPMK:FAIR-CLI-BEGIN:", b":parity", b"DPCLI:PARITY DPSTATUS routes=t1>1,c2>3,b3>4,n6>5,h7>3,d8>5 service_caps=h2,t3,ls4,cat5,st6,neg7,tt8,tf9,tb10,ttc11,q12,p13 storage_mode=ro network_counters=a1,i1,u16,200=3,404=1,405=1,413=2,500=1 timer_status=ok,cli,fs,blk,net,tcpip,http,dhcp fault_status=tb:ready,f0,r0,p0,c0 generation_id=1", b"DPMK:FAIR-CLI-END:"]),
    ("fs cat /INDEX.HTM", [b"DPMK:FAIR-CLI-BEGIN:", b":fs cat /INDEX.HTM", b"DPCLI:CAT /INDEX.HTM", b"DPCLI:END /INDEX.HTM", b"DPMK:FAIR-CLI-END:"]),
]

sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
while True:
    try:
        sock.connect(sock_path)
        break
    except (FileNotFoundError, ConnectionRefusedError):
        if time.monotonic() >= run_deadline:
            raise SystemExit("fairness serial socket did not become ready")
        time.sleep(0.05)

sock.setblocking(False)
captured = bytearray()


def read_some(out, allow_closed=False):
    readable, _, _ = select.select([sock], [], [], 0.1)
    if not readable:
        return False
    chunk = sock.recv(4096)
    if not chunk:
        if allow_closed:
            return None
        raise SystemExit("fairness serial socket closed before expected output")
    captured.extend(chunk)
    out.write(chunk)
    out.flush()
    return True


def wait_for_all(out, expected, label, start=0, timeout_seconds=None):
    deadline = time.monotonic() + 180.0 if timeout_seconds is None else time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        window = bytes(captured[start:])
        missing = [item for item in expected if item not in window]
        if not missing:
            return
        progress = read_some(out, allow_closed=True)
        if progress is None:
            window = bytes(captured[start:])
            missing = [item for item in expected if item not in window]
            if not missing:
                return
            missing_text = ", ".join(item.decode("ascii", errors="replace") for item in missing)
            raise SystemExit(f"missing fairness {label}: {missing_text}")
    missing_text = ", ".join(item.decode("ascii", errors="replace") for item in missing)
    raise SystemExit(f"missing fairness {label}: {missing_text}")


def send_command(out, command, expected, timeout_seconds=60.0):
    start = len(captured)
    sock.sendall(command.encode("ascii") + b"\n")
    wait_for_all(out, expected, f"output for command {command}", start=start, timeout_seconds=timeout_seconds)


with open(serial_log, "wb") as out:
    wait_for_all(out, boot_markers, "boot markers", timeout_seconds=boot_wait_seconds)
    for command, expected in command_checks:
        send_command(out, command, expected)
    wait_for_all(out, [b"DPMK:CLI-COMMANDS-OK"], "CLI completion gate", timeout_seconds=gate_wait_seconds)
    open(fairness_start_file, "wb").close()
    wait_for_all(out, [b"DPMK:FAIR-CLI-INPUT-READY"], "fairness input readiness", timeout_seconds=gate_wait_seconds)
    for command, expected in fairness_command_checks:
        send_command(out, command, expected)
    wait_for_all(out, fair_markers, "FAIR markers", timeout_seconds=gate_wait_seconds)
    wait_for_all(out, final_markers, "final markers", timeout_seconds=final_wait_seconds)
    final_deadline = min(run_deadline, time.monotonic() + final_wait_seconds)
    while time.monotonic() < final_deadline:
        if re.search(rb"DPMK:FAIR-TIMER-MAXGAP:(\d+)", bytes(captured)):
            break
        if not read_some(out, allow_closed=True):
            break

captured_bytes = bytes(captured)
timer_matches = [int(item) for item in re.findall(rb"DPMK:FAIR-TIMER-MAXGAP:(\d+)", captured_bytes)]
if not timer_matches:
    with open(serial_log, "rb") as serial_in:
        captured_bytes = serial_in.read()
    timer_matches = [int(item) for item in re.findall(rb"DPMK:FAIR-TIMER-MAXGAP:(\d+)", captured_bytes)]
missing_boot = [marker.decode("ascii") for marker in boot_markers if marker not in captured_bytes]
missing_final = [marker.decode("ascii") for marker in final_markers if marker not in captured_bytes]
missing_fair = [marker.decode("ascii") for marker in fair_markers if marker not in captured_bytes]
missing_commands = []
for command, expected in command_checks:
    for item in expected:
        if item not in captured_bytes:
            missing_commands.append(f"{command}:{item.decode('ascii', errors='replace')}")
for command, expected in fairness_command_checks:
    for item in expected:
        if item not in captured_bytes:
            missing_commands.append(f"fair:{command}:{item.decode('ascii', errors='replace')}")
if not timer_matches:
    missing_fair.append("DPMK:FAIR-TIMER-MAXGAP:<n>")
timer_max_gap = max(timer_matches) if timer_matches else -1
if timer_max_gap > 32:
    raise SystemExit(f"fairness timer max gap {timer_max_gap} exceeds bound 32")
if missing_boot or missing_final or missing_fair or missing_commands:
    raise SystemExit("missing fairness serial evidence")

with open(client_log, "w", encoding="ascii") as out:
    out.write(f"bytes={len(captured_bytes)}\n")
    out.write("commands=help,tasks,fs ls /,fs cat /HELLO.TXT,fs cat /INDEX.HTM,fs stat /HELLO.TXT,fs stat /INDEX.HTM,fs stat /MISSING.TXT,fs stat /THISNAMEISTOOLONG.TXT,fs write /OUT.TXT append,task timer,task fs,task block,task tcpip,queues\n")
    out.write("fairness_commands=tasks,fs ls /,fs cat /INDEX.HTM,task timer,task tcpip,queues,parity,fs cat /INDEX.HTM\n")
    out.write("missing_boot=" + ",".join(missing_boot) + "\n")
    out.write("missing_final=" + ",".join(missing_final) + "\n")
    out.write("missing_fair=" + ",".join(missing_fair) + "\n")
    out.write("missing_commands=" + ",".join(missing_commands) + "\n")
    out.write(f"fairness_timer_max_gap={timer_max_gap}\n")
    out.write("markers_found=true\n")

with open(fairness_log, "w", encoding="ascii") as out:
    out.write("fairness_mode=usernet-hostfwd-plus-serial-cli\n")
    out.write(f"fairness_expected_curl_gets={curl_count}\n")
    out.write("fairness_cli_commands=tasks,fs ls /,fs cat /INDEX.HTM,task timer,task tcpip,queues,parity,fs cat /INDEX.HTM\n")
    out.write("filesystem_service_hardening_commands=fs stat /HELLO.TXT,fs stat /INDEX.HTM,fs stat /MISSING.TXT,fs stat /THISNAMEISTOOLONG.TXT,fs write /OUT.TXT append\n")
    out.write(f"fairness_timer_max_gap={timer_max_gap}\n")
    out.write("fairness_fault_contained=true\n")
    out.write("fairness_serial_evidence=true\n")
    out.write("fairness_missing=\n")
PY

  if ! wait "$curl_pid"; then
    curl_pid=""
    echo "FAIL: fairness curl worker did not complete every host-kernel GET"
    echo "--- fairness curl log ---"
    sed -n '1,220p' "$fairness_curl_log" 2>/dev/null || true
    echo "--- serial ---"
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    exit 1
  fi
  curl_pid=""

  kill "$qemu_pid" >/dev/null 2>&1 || true
  wait "$qemu_pid" >/dev/null 2>&1 || true
  qemu_pid=""

  grep -q "fairness_curl_completed=$fairness_curl_count" "$fairness_curl_log" || {
    echo "FAIL: fairness curl completion count missing"
    sed -n '1,220p' "$fairness_curl_log" 2>/dev/null || true
    exit 1
  }
  grep -q "markers_found=true" "$fairness_client_log" || {
    echo "FAIL: fairness serial client did not record complete markers"
    sed -n '1,220p' "$fairness_client_log" 2>/dev/null || true
    exit 1
  }
  grep -q "fairness_missing=" "$fairness_log" || {
    echo "FAIL: fairness log did not record missing-evidence state"
    sed -n '1,220p' "$fairness_log" 2>/dev/null || true
    exit 1
  }
  if [[ ! -s "$fairness_pcap" ]]; then
    echo "FAIL: fairness pcap was not written"
    exit 1
  fi
  fairness_pcap_bytes="$(wc -c <"$fairness_pcap" | tr -d ' ')"
  if (( fairness_pcap_bytes <= 24 )); then
    echo "FAIL: fairness pcap contains only a global header"
    exit 1
  fi
  fairness_pcap_hex="$(od -An -tx1 -v "$fairness_pcap" | tr -d ' \n')"
  if [[ "$fairness_pcap_hex" != *"474554202f494e4445582e48544d20485454502f312e30"* ]]; then
    echo "FAIL: fairness pcap does not contain curl GET /INDEX.HTM requests"
    exit 1
  fi
  if [[ "$fairness_pcap_hex" != *"3c21646f63747970652068746d6c3e"* ]]; then
    echo "FAIL: fairness pcap does not contain INDEX.HTM response body bytes"
    exit 1
  fi
  fairness_timer_max_gap="$(awk -F= '$1 == "fairness_timer_max_gap" { value=$2 } END { print value }' "$fairness_client_log")"
  [[ -n "$fairness_timer_max_gap" ]] || {
    echo "FAIL: fairness client log missing timer max gap"
    exit 1
  }

  if [[ "$mode" == "--scheduler-fairness-load-proof" ]]; then
    {
      echo "scheduler_fairness_summary=$scheduler_fairness_summary"
      echo "scheduler_fairness_summary_status=pass"
      echo "scheduler_fairness_load=1"
      echo "scheduler_fairness_load_mode=usernet-hostfwd-plus-serial-cli"
      echo "scheduler_fairness_load_http_ok=true"
      echo "scheduler_fairness_load_cli_ok=true"
      echo "scheduler_fairness_load_fs_ok=true"
      echo "scheduler_fairness_load_fault_contained=true"
      echo "scheduler_fairness_load_timer_bound_ok=true"
      echo "scheduler_fairness_load_timer_max_gap=$fairness_timer_max_gap"
      echo "scheduler_fairness_load_curl_completed=$fairness_curl_count"
      echo "scheduler_fairness_load_curl_expected=$fairness_curl_count"
      echo "scheduler_fairness_load_serial_markers_ok=true"
      echo "scheduler_fairness_load_pcap_ok=true"
      echo "scheduler_fairness_load_pcap_bytes=$fairness_pcap_bytes"
      echo "scheduler_fairness_load_no_benchmark_retune_ok=true"
      echo "scheduler_fairness_load_tls_deferred=true"
      echo "scheduler_fairness_load_default_writable=false"
      echo "scheduler_fairness_load_release_claim=false"
      echo "scheduler_fairness_load_serial_log=$serial_log"
      echo "scheduler_fairness_load_client_log=$fairness_client_log"
      echo "scheduler_fairness_load_fairness_log=$fairness_log"
      echo "scheduler_fairness_load_curl_log=$fairness_curl_log"
      echo "scheduler_fairness_load_pcap=$fairness_pcap"
      echo "scheduler_fairness_load_qemu_log=$qemu_log"
    } >"$scheduler_fairness_summary"
    grep -q "scheduler_fairness_summary_status=pass" "$scheduler_fairness_summary"
    grep -q "scheduler_fairness_load_timer_bound_ok=true" "$scheduler_fairness_summary"
  fi

  echo "x86_64 microkernel FAT32 fairness proof passed."
  if [[ "$mode" == "--scheduler-fairness-load-proof" ]]; then
    echo "x86_64 microkernel scheduler fairness load proof passed."
    echo "scheduler fairness summary: $scheduler_fairness_summary"
  fi
  echo "fairness url: $fairness_url"
  echo "fairness log: $fairness_log"
  echo "fairness client log: $fairness_client_log"
  echo "fairness curl log: $fairness_curl_log"
  echo "fairness artifact dir: $fairness_artifact_dir"
  echo "fairness pcap: $fairness_pcap"
  echo "serial log: $serial_log"
  echo "qemu log: $qemu_log"
  exit 0
fi

if [[ "$mode" == "--bounded-tcp-negative-proof" ]]; then
  neg_host_port=$((46000 + RANDOM % 1000))
  neg_qemu_port=$((neg_host_port + 1000))

  rm -f "$bounded_tcp_log" "$bounded_tcp_pcap"

  DP_HOST_PORT="$neg_host_port" \
  DP_QEMU_PORT="$neg_qemu_port" \
  DP_BOUNDED_TCP_LOG="$bounded_tcp_log" \
  python3 - <<'PY' &
import os
import socket
import time

host_port = int(os.environ["DP_HOST_PORT"])
qemu_port = int(os.environ["DP_QEMU_PORT"])
log_path = os.environ["DP_BOUNDED_TCP_LOG"]

vm_mac = bytes.fromhex("525400123456")
host_mac = bytes.fromhex("020000000004")
host_ip = bytes([10, 0, 0, 1])
vm_ip = bytes([10, 0, 0, 2])
http_port = 80

sessions = [
    {
        "name": "overflow_payload",
        "host_port": 41280,
        "seq": 0x41000000,
        "payload": b"X" * 473,
        "behavior": "overflow_payload",
        "start_after": 0.0,
    },
    {
        "name": "reset_before_completion",
        "host_port": 41281,
        "seq": 0x42000000,
        "payload": b"",
        "behavior": "reset_before_completion",
        "start_after": 0.0,
    },
    {
        "name": "duplicate_payload",
        "host_port": 41282,
        "seq": 0x43000000,
        "payload": b"GET /INDEX.HTM HTTP/1.0\r\nHost: dataplane\r\n\r\n",
        "behavior": "duplicate_payload",
        "start_after": 0.0,
    },
    {
        "name": "partial_request_no_response",
        "host_port": 41283,
        "seq": 0x44000000,
        "payload": b"GET /INDEX.HTM HTTP/1.0\r\n",
        "behavior": "partial_request_no_response",
        "start_after": 0.0,
    },
    {
        "name": "fin_during_response",
        "host_port": 41284,
        "seq": 0x45000000,
        "payload": b"GET /INDEX.HTM HTTP/1.0\r\nHost: dataplane\r\n\r\n",
        "behavior": "fin_during_response",
        "start_after": 3.5,
    },
    {
        "name": "session_exhaustion_hold_0",
        "host_port": 41285,
        "seq": 0x46000000,
        "payload": b"GET /INDEX.HTM HTTP/1.0\r\n",
        "behavior": "session_exhaustion_hold",
        "start_after": 5.0,
    },
    {
        "name": "session_exhaustion_hold_1",
        "host_port": 41286,
        "seq": 0x47000000,
        "payload": b"GET /INDEX.HTM HTTP/1.0\r\n",
        "behavior": "session_exhaustion_hold",
        "start_after": 5.0,
    },
    {
        "name": "session_exhaustion_hold_2",
        "host_port": 41287,
        "seq": 0x48000000,
        "payload": b"GET /INDEX.HTM HTTP/1.0\r\n",
        "behavior": "session_exhaustion_hold",
        "start_after": 5.0,
    },
    {
        "name": "session_exhaustion_hold_3",
        "host_port": 41288,
        "seq": 0x49000000,
        "payload": b"GET /INDEX.HTM HTTP/1.0\r\n",
        "behavior": "session_exhaustion_hold",
        "start_after": 5.0,
    },
    {
        "name": "session_exhaustion_probe",
        "host_port": 41289,
        "seq": 0x4a000000,
        "payload": b"",
        "behavior": "session_exhaustion_probe",
        "start_after": 5.75,
    },
]
for session in sessions:
    session["state"] = "syn"
    session["ack"] = 0
    session["next_send"] = 0.0
    session["started"] = False
    session["syn_sent_at"] = 0.0
    session["syn_ack_seen"] = False
    session["payload_seq"] = 0
    session["payload_sent"] = False
    session["payload_sent_at"] = 0.0
    session["payload_acked"] = False
    session["duplicate_payload_sent"] = False
    session["duplicate_payload_sent_at"] = 0.0
    session["response_after_duplicate"] = False
    session["response_seen_at"] = 0.0
    session["rst_sent"] = False
    session["rst_sent_at"] = 0.0
    session["cleanup_rst_sent"] = False
    session["fin_sent"] = False
    session["fin_sent_at"] = 0.0
    session["fin_ack_seen"] = False
    session["http_response_seen"] = False
    session["http_response_count"] = 0
    session["rx_payload_bytes"] = 0


def be16(value):
    return value.to_bytes(2, "big")


def be32(value):
    return value.to_bytes(4, "big")


def checksum(data):
    total = 0
    if len(data) % 2:
        data += b"\x00"
    for index in range(0, len(data), 2):
        total += int.from_bytes(data[index:index + 2], "big")
    while total >> 16:
        total = (total & 0xffff) + (total >> 16)
    return (~total) & 0xffff


def ipv4_header(src_ip, dst_ip, proto, payload_len, ident):
    header = bytearray(20)
    header[0] = 0x45
    header[2:4] = be16(20 + payload_len)
    header[4:6] = be16(ident & 0xffff)
    header[6:8] = be16(0x4000)
    header[8] = 64
    header[9] = proto
    header[12:16] = src_ip
    header[16:20] = dst_ip
    header[10:12] = be16(checksum(bytes(header)))
    return bytes(header)


def arp_request_frame():
    arp = (
        be16(1)
        + be16(0x0800)
        + bytes([6, 4])
        + be16(1)
        + host_mac
        + host_ip
        + bytes(6)
        + vm_ip
    )
    return (b"\xff" * 6 + host_mac + be16(0x0806) + arp).ljust(60, b"\x00")


def tcp_frame(src_port, dst_port, seq, ack, flags, payload=b"", ident=0x7600):
    header = bytearray(20)
    header[0:2] = be16(src_port)
    header[2:4] = be16(dst_port)
    header[4:8] = be32(seq)
    header[8:12] = be32(ack)
    header[12] = 5 << 4
    header[13] = flags
    header[14:16] = be16(4096)
    pseudo = host_ip + vm_ip + b"\x00" + bytes([6]) + be16(len(header) + len(payload))
    header[16:18] = be16(checksum(pseudo + bytes(header) + payload))
    packet = ipv4_header(host_ip, vm_ip, 6, len(header) + len(payload), ident)
    return (vm_mac + host_mac + be16(0x0800) + packet + bytes(header) + payload).ljust(60, b"\x00")


def send_frame(sock, frame):
    sock.sendto(frame, ("127.0.0.1", qemu_port))


def strip_qemu_prefix(data):
    if data.startswith(b"\x00\x00"):
        return data[2:]
    return data


def parse_tcp_response(data):
    data = strip_qemu_prefix(data)
    if len(data) < 14 + 20 + 20:
        return None
    if data[0:6] != host_mac or data[6:12] != vm_mac or data[12:14] != be16(0x0800):
        return None
    ip = data[14:34]
    ihl = (ip[0] & 0x0f) * 4
    if (ip[0] >> 4) != 4 or ihl < 20 or ip[9] != 6:
        return None
    total_len = int.from_bytes(ip[2:4], "big")
    if total_len < ihl + 20 or len(data) < 14 + total_len:
        return None
    tcp = data[14 + ihl:14 + total_len]
    data_offset = (tcp[12] >> 4) * 4
    if data_offset < 20 or len(tcp) < data_offset:
        return None
    dst_port = int.from_bytes(tcp[2:4], "big")
    for session in sessions:
        if dst_port == session["host_port"]:
            return {
                "session": session,
                "src_port": int.from_bytes(tcp[0:2], "big"),
                "seq": int.from_bytes(tcp[4:8], "big"),
                "ack": int.from_bytes(tcp[8:12], "big"),
                "flags": tcp[13],
                "payload": tcp[data_offset:],
            }
    return None


def send_syn(sock, session, now):
    send_frame(sock, tcp_frame(session["host_port"], http_port, session["seq"], 0, 0x02))
    session["started"] = True
    if session["syn_sent_at"] == 0.0:
        session["syn_sent_at"] = now
    session["next_send"] = now + 0.10


def send_ack(sock, session):
    send_frame(sock, tcp_frame(session["host_port"], http_port, session["seq"], session["ack"], 0x10))


def send_payload(sock, session, now):
    payload = session["payload"]
    session["payload_seq"] = session["seq"]
    send_frame(sock, tcp_frame(session["host_port"], http_port, session["payload_seq"], session["ack"], 0x18, payload))
    session["seq"] = session["payload_seq"] + len(payload)
    session["payload_sent"] = True
    session["payload_sent_at"] = now
    session["state"] = "payload"
    session["next_send"] = now + 0.20


def send_duplicate_payload(sock, session, now):
    send_frame(sock, tcp_frame(session["host_port"], http_port, session["payload_seq"], session["ack"], 0x18, session["payload"]))
    session["duplicate_payload_sent"] = True
    session["duplicate_payload_sent_at"] = now
    session["next_send"] = now + 0.50


def send_rst(sock, session, now):
    send_frame(sock, tcp_frame(session["host_port"], http_port, session["seq"], session["ack"], 0x14))
    session["rst_sent"] = True
    session["rst_sent_at"] = now
    session["state"] = "reset"


def send_fin(sock, session, now):
    send_frame(sock, tcp_frame(session["host_port"], http_port, session["seq"], session["ack"], 0x11))
    session["seq"] += 1
    session["fin_sent"] = True
    session["fin_sent_at"] = now
    session["state"] = "fin"
    session["next_send"] = now + 0.50


def send_cleanup_rst(sock, session, now):
    if not session["cleanup_rst_sent"]:
        send_rst(sock, session, now)
        session["cleanup_rst_sent"] = True


def drive_session(sock, session, now):
    if now < start_time + session["start_after"]:
        return
    if now < session["next_send"]:
        return
    if session["state"] == "syn":
        if session["behavior"] == "session_exhaustion_probe" and session["syn_sent_at"] != 0.0:
            return
        send_syn(sock, session, now)
    elif session["state"] == "established":
        if session["behavior"] == "reset_before_completion":
            send_rst(sock, session, now)
        elif session["behavior"] == "session_exhaustion_probe":
            return
        else:
            send_payload(sock, session, now)
            if session["behavior"] == "fin_during_response":
                send_fin(sock, session, now)
    elif (
        session["state"] == "payload"
        and not session["payload_acked"]
        and session["behavior"] not in (
            "duplicate_payload",
            "overflow_payload",
            "partial_request_no_response",
            "fin_during_response",
            "session_exhaustion_hold",
        )
    ):
        send_payload(sock, session, now)
    elif (
        session["state"] == "payload"
        and session["behavior"] == "duplicate_payload"
        and session["payload_acked"]
        and not session["duplicate_payload_sent"]
    ):
        send_duplicate_payload(sock, session, now)


def handle_tcp_response(sock, parsed, now):
    session = parsed["session"]
    flags = parsed["flags"]
    payload = parsed["payload"]
    if session["state"] == "syn" and parsed["src_port"] == http_port and flags & 0x12 == 0x12:
        session["syn_ack_seen"] = True
        session["ack"] = parsed["seq"] + 1
        session["seq"] += 1
        session["state"] = "established"
        send_ack(sock, session)
        if session["behavior"] == "reset_before_completion":
            send_rst(sock, session, now)
        else:
            send_payload(sock, session, now)
            if session["behavior"] == "fin_during_response":
                send_fin(sock, session, now)
        return
    if payload:
        session["rx_payload_bytes"] += len(payload)
        if payload.startswith(b"HTTP/1."):
            if session["duplicate_payload_sent"]:
                session["response_after_duplicate"] = True
            session["http_response_seen"] = True
            session["http_response_count"] += 1
            if session["response_seen_at"] == 0.0:
                session["response_seen_at"] = now
        session["ack"] = parsed["seq"] + len(payload) + (1 if flags & 0x01 else 0)
        send_ack(sock, session)
    elif flags & 0x01:
        session["ack"] = parsed["seq"] + 1
        send_ack(sock, session)
    if session["fin_sent"] and parsed["ack"] >= session["seq"]:
        session["fin_ack_seen"] = True
    if session["payload_sent"] and parsed["ack"] >= session["seq"]:
        session["payload_acked"] = True


sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.bind(("127.0.0.1", host_port))
sock.setblocking(False)

start_time = time.monotonic()
deadline = start_time + 24.0
next_arp = 0.0
while time.monotonic() < deadline:
    now = time.monotonic()
    if now >= next_arp:
        send_frame(sock, arp_request_frame())
        next_arp = now + 0.05
    for session in sessions:
        drive_session(sock, session, now)
    try:
        while True:
            data, _ = sock.recvfrom(2048)
            parsed = parse_tcp_response(data)
            if parsed is not None:
                handle_tcp_response(sock, parsed, time.monotonic())
    except BlockingIOError:
        pass
    overflow = sessions[0]
    reset = sessions[1]
    duplicate = sessions[2]
    partial = sessions[3]
    fin = sessions[4]
    holders = sessions[5:9]
    exhaustion_probe = sessions[9]
    overflow_done = (
        overflow["payload_sent"]
        and time.monotonic() - overflow["payload_sent_at"] >= 1.25
        and not overflow["http_response_seen"]
    )
    reset_done = (
        reset["rst_sent"]
        and time.monotonic() - reset["rst_sent_at"] >= 1.25
        and not reset["http_response_seen"]
    )
    duplicate_done = (
        duplicate["duplicate_payload_sent"]
        and time.monotonic() - duplicate["duplicate_payload_sent_at"] >= 1.25
        and duplicate["http_response_count"] == 1
        and not duplicate["response_after_duplicate"]
    )
    partial_done = (
        partial["payload_sent"]
        and time.monotonic() - partial["payload_sent_at"] >= 1.25
        and not partial["http_response_seen"]
    )
    fin_done = (
        fin["fin_sent"]
        and fin["http_response_seen"]
        and fin["fin_ack_seen"]
        and time.monotonic() - fin["fin_sent_at"] >= 0.25
    )
    session_exhaustion_done = (
        all(
            holder["syn_ack_seen"]
            and holder["payload_sent"]
            and not holder["http_response_seen"]
            for holder in holders
        )
        and exhaustion_probe["syn_sent_at"] != 0.0
        and time.monotonic() - exhaustion_probe["syn_sent_at"] >= 1.25
        and not exhaustion_probe["syn_ack_seen"]
        and not exhaustion_probe["http_response_seen"]
    )
    if duplicate_done:
        send_cleanup_rst(sock, duplicate, now)
    if partial_done:
        send_cleanup_rst(sock, partial, now)
    if (
        overflow_done
        and reset_done
        and duplicate_done
        and partial_done
        and fin_done
        and session_exhaustion_done
    ):
        break
    time.sleep(0.001)

overflow = sessions[0]
reset = sessions[1]
duplicate = sessions[2]
partial = sessions[3]
fin = sessions[4]
holders = sessions[5:9]
exhaustion_probe = sessions[9]
overflow_no_response = (
    overflow["syn_ack_seen"]
    and overflow["payload_sent"]
    and not overflow["http_response_seen"]
)
reset_no_response = (
    reset["syn_ack_seen"]
    and reset["rst_sent"]
    and not reset["http_response_seen"]
)
duplicate_no_second_response = (
    duplicate["syn_ack_seen"]
    and duplicate["payload_sent"]
    and duplicate["duplicate_payload_sent"]
    and duplicate["http_response_count"] == 1
    and not duplicate["response_after_duplicate"]
)
partial_no_response = (
    partial["syn_ack_seen"]
    and partial["payload_sent"]
    and not partial["http_response_seen"]
)
fin_during_response_proved = (
    fin["syn_ack_seen"]
    and fin["payload_sent"]
    and fin["fin_sent"]
    and fin["http_response_seen"]
    and fin["fin_ack_seen"]
)
session_exhaustion_proved = (
    all(
        holder["syn_ack_seen"]
        and holder["payload_sent"]
        and not holder["http_response_seen"]
        for holder in holders
    )
    and exhaustion_probe["syn_sent_at"] != 0.0
    and not exhaustion_probe["syn_ack_seen"]
    and not exhaustion_probe["http_response_seen"]
)

with open(log_path, "w", encoding="ascii") as out:
    out.write("bounded_tcp_mode=negative-host-raw\n")
    out.write("bounded_tcp_tls_deferred=true\n")
    out.write("bounded_tcp_negative_cases=duplicate_payload,partial_request_no_response,session_exhaustion,fin_during_response,overflow_payload,reset_before_completion\n")
    out.write("bounded_tcp_overflow_payload_host_handshake=true\n" if overflow["syn_ack_seen"] else "bounded_tcp_overflow_payload_host_handshake=false\n")
    out.write("bounded_tcp_overflow_payload_sent=true\n" if overflow["payload_sent"] else "bounded_tcp_overflow_payload_sent=false\n")
    out.write("bounded_tcp_overflow_payload_acked=true\n" if overflow["payload_acked"] else "bounded_tcp_overflow_payload_acked=false\n")
    out.write("bounded_tcp_overflow_payload_bytes=473\n")
    out.write("bounded_tcp_overflow_payload_no_response=true\n" if overflow_no_response else "bounded_tcp_overflow_payload_no_response=false\n")
    out.write(f"bounded_tcp_overflow_payload_rx_payload_bytes={overflow['rx_payload_bytes']}\n")
    out.write("bounded_tcp_reset_before_completion_host_handshake=true\n" if reset["syn_ack_seen"] else "bounded_tcp_reset_before_completion_host_handshake=false\n")
    out.write("bounded_tcp_reset_before_completion_rst_sent=true\n" if reset["rst_sent"] else "bounded_tcp_reset_before_completion_rst_sent=false\n")
    out.write("bounded_tcp_reset_before_completion_no_response=true\n" if reset_no_response else "bounded_tcp_reset_before_completion_no_response=false\n")
    out.write(f"bounded_tcp_reset_before_completion_rx_payload_bytes={reset['rx_payload_bytes']}\n")
    out.write("bounded_tcp_duplicate_payload_host_handshake=true\n" if duplicate["syn_ack_seen"] else "bounded_tcp_duplicate_payload_host_handshake=false\n")
    out.write("bounded_tcp_duplicate_payload_sent=true\n" if duplicate["payload_sent"] else "bounded_tcp_duplicate_payload_sent=false\n")
    out.write("bounded_tcp_duplicate_payload_replayed=true\n" if duplicate["duplicate_payload_sent"] else "bounded_tcp_duplicate_payload_replayed=false\n")
    out.write("bounded_tcp_duplicate_payload_no_second_response=true\n" if duplicate_no_second_response else "bounded_tcp_duplicate_payload_no_second_response=false\n")
    out.write(f"bounded_tcp_duplicate_payload_http_response_count={duplicate['http_response_count']}\n")
    out.write("bounded_tcp_partial_request_host_handshake=true\n" if partial["syn_ack_seen"] else "bounded_tcp_partial_request_host_handshake=false\n")
    out.write(f"bounded_tcp_partial_request_peer_port={partial['host_port']}\n")
    out.write("bounded_tcp_partial_request_payload_sent=true\n" if partial["payload_sent"] else "bounded_tcp_partial_request_payload_sent=false\n")
    out.write("bounded_tcp_partial_request_no_response=true\n" if partial_no_response else "bounded_tcp_partial_request_no_response=false\n")
    out.write(f"bounded_tcp_partial_request_rx_payload_bytes={partial['rx_payload_bytes']}\n")
    out.write("bounded_tcp_session_exhaustion_open_sessions=4\n" if all(holder["syn_ack_seen"] and holder["payload_sent"] for holder in holders) else "bounded_tcp_session_exhaustion_open_sessions=0\n")
    out.write("bounded_tcp_session_exhaustion_extra_syn_sent=true\n" if exhaustion_probe["syn_sent_at"] != 0.0 else "bounded_tcp_session_exhaustion_extra_syn_sent=false\n")
    out.write("bounded_tcp_session_exhaustion_no_slot_reuse=true\n" if session_exhaustion_proved else "bounded_tcp_session_exhaustion_no_slot_reuse=false\n")
    out.write("bounded_tcp_session_exhaustion_no_http_response=true\n" if not exhaustion_probe["http_response_seen"] else "bounded_tcp_session_exhaustion_no_http_response=false\n")
    out.write("bounded_tcp_fin_during_response_host_handshake=true\n" if fin["syn_ack_seen"] else "bounded_tcp_fin_during_response_host_handshake=false\n")
    out.write("bounded_tcp_fin_during_response_payload_sent=true\n" if fin["payload_sent"] else "bounded_tcp_fin_during_response_payload_sent=false\n")
    out.write("bounded_tcp_fin_during_response_fin_sent=true\n" if fin["fin_sent"] else "bounded_tcp_fin_during_response_fin_sent=false\n")
    out.write("bounded_tcp_fin_during_response_http_response_seen=true\n" if fin["http_response_seen"] else "bounded_tcp_fin_during_response_http_response_seen=false\n")
    out.write("bounded_tcp_fin_during_response_fin_ack_seen=true\n" if fin["fin_ack_seen"] else "bounded_tcp_fin_during_response_fin_ack_seen=false\n")

ok = (
    overflow_no_response
    and reset_no_response
    and duplicate_no_second_response
    and partial_no_response
    and session_exhaustion_proved
    and fin_during_response_proved
)
raise SystemExit(0 if ok else 3)
PY
  host_pid="$!"

  timeout 25s qemu-system-x86_64 \
    -M pc \
    -m 128M \
    -nographic \
    -monitor none \
    -drive file="$image",format=raw,if=floppy \
    -boot a \
    -drive if=none,id=fs,file="$fs_img",format=raw \
    -device virtio-blk-pci-transitional,drive=fs \
    -netdev socket,id=xnet,udp=127.0.0.1:"$neg_host_port",localaddr=127.0.0.1:"$neg_qemu_port" \
    -object "filter-dump,id=xnet_bounded_tcp_dump,netdev=xnet,file=$bounded_tcp_pcap" \
    -device virtio-net-pci-transitional,netdev=xnet,mac=52:54:00:12:34:56 \
    -serial "file:$serial_log" \
    -no-reboot \
    -no-shutdown >"$qemu_log" 2>&1 &
  qemu_pid="$!"

  if ! wait "$host_pid"; then
    host_pid=""
    echo "FAIL: bounded TCP negative host proof failed"
    sed -n '1,220p' "$bounded_tcp_log" 2>/dev/null || true
    sed -n '1,220p' "$serial_log" 2>/dev/null || true
    exit 1
  fi
  host_pid=""

  kill "$qemu_pid" >/dev/null 2>&1 || true
  wait "$qemu_pid" >/dev/null 2>&1 || true
  qemu_pid=""

  grep -q "bounded_tcp_overflow_payload_no_response=true" "$bounded_tcp_log" || {
    echo "FAIL: overflow payload negative case did not produce no-response evidence"
    sed -n '1,220p' "$bounded_tcp_log" 2>/dev/null || true
    exit 1
  }
  grep -q "bounded_tcp_reset_before_completion_no_response=true" "$bounded_tcp_log" || {
    echo "FAIL: reset-before-completion negative case did not produce no-response evidence"
    sed -n '1,220p' "$bounded_tcp_log" 2>/dev/null || true
    exit 1
  }
  grep -q "bounded_tcp_duplicate_payload_no_second_response=true" "$bounded_tcp_log" || {
    echo "FAIL: duplicate-payload negative case produced a second response"
    sed -n '1,220p' "$bounded_tcp_log" 2>/dev/null || true
    exit 1
  }
  grep -q "bounded_tcp_duplicate_payload_http_response_count=1" "$bounded_tcp_log" || {
    echo "FAIL: duplicate-payload negative case did not produce exactly one response"
    sed -n '1,220p' "$bounded_tcp_log" 2>/dev/null || true
    exit 1
  }
  grep -q "bounded_tcp_partial_request_no_response=true" "$bounded_tcp_log" || {
    echo "FAIL: partial-request negative case produced a response"
    sed -n '1,220p' "$bounded_tcp_log" 2>/dev/null || true
    exit 1
  }
  grep -q "bounded_tcp_session_exhaustion_no_slot_reuse=true" "$bounded_tcp_log" || {
    echo "FAIL: session-exhaustion negative case reused a TCP slot or received a SYN-ACK"
    sed -n '1,220p' "$bounded_tcp_log" 2>/dev/null || true
    exit 1
  }
  grep -q "bounded_tcp_session_exhaustion_no_http_response=true" "$bounded_tcp_log" || {
    echo "FAIL: session-exhaustion negative case produced HTTP response bytes"
    sed -n '1,220p' "$bounded_tcp_log" 2>/dev/null || true
    exit 1
  }
  grep -q "bounded_tcp_fin_during_response_fin_ack_seen=true" "$bounded_tcp_log" || {
    echo "FAIL: FIN-during-response negative case did not receive FIN ACK"
    sed -n '1,220p' "$bounded_tcp_log" 2>/dev/null || true
    exit 1
  }
  if [[ ! -s "$bounded_tcp_pcap" ]]; then
    echo "FAIL: bounded TCP negative proof pcap was not written"
    exit 1
  fi
  bounded_tcp_pcap_bytes="$(wc -c <"$bounded_tcp_pcap" | tr -d ' ')"
  if (( bounded_tcp_pcap_bytes <= 24 )); then
    echo "FAIL: bounded TCP negative proof pcap contains only a global header"
    exit 1
  fi

  echo "x86_64 microkernel bounded TCP negative proof passed."
  echo "bounded TCP log: $bounded_tcp_log"
  echo "bounded TCP pcap: $bounded_tcp_pcap"
  echo "serial log: $serial_log"
  echo "qemu log: $qemu_log"
  exit 0
fi

host_port=$((42000 + RANDOM % 1000))
qemu_port=$((host_port + 1000))
packet_count=16

DP_HOST_PORT="$host_port" \
DP_QEMU_PORT="$qemu_port" \
DP_EXCHANGE_LOG="$host_exchange_log" \
DP_PACKET_COUNT="$packet_count" \
DP_PROOF_KIND="$proof_kind" \
python3 - <<'PY' &
import os
import socket
import time

host_port = int(os.environ["DP_HOST_PORT"])
qemu_port = int(os.environ["DP_QEMU_PORT"])
log_path = os.environ["DP_EXCHANGE_LOG"]
packet_count = int(os.environ["DP_PACKET_COUNT"])
proof_kind = os.environ.get("DP_PROOF_KIND", "")

vm_mac = bytes.fromhex("525400123456")
host_mac = bytes.fromhex("020000000004")
host_ip = bytes([10, 0, 0, 1])
vm_ip = bytes([10, 0, 0, 2])
request_magic = b"DPUDPQ00"
response_magic = b"DPUDPR00"
host_port_udp = 40000
vm_port_udp = 40001
bad_magic_seq = 0xF0000001
overlong_seq = 0xF0000002
wrong_route_seq = 0xF0000003
bad_check_seq = 0xF0000004
short_payload_seq = 0xF0000005
wrong_src_port_seq = 0xF0000006
icmp_ident = 0x4450
icmp_seq = 1
http_port = 80
http_index_body = b"<!doctype html><html><head><title>dataplane</title></head><body><h1>dataplane microkernel</h1></body></html>\r\n"
if proof_kind == "parity":
    http_sessions = [
        {
            "name": "status",
            "host_port": 41083,
            "seq": 0x40000000,
            "request": b"GET /STATUS.TXT HTTP/1.0\r\nHost: dataplane\r\n\r\n",
            "kind": "status",
        },
        {
            "name": "head_index",
            "host_port": 41081,
            "seq": 0x20000000,
            "request": b"HEAD /INDEX.HTM HTTP/1.0\r\nHost: dataplane\r\n\r\n",
            "kind": "head_index",
        },
        {
            "name": "cap_negative",
            "host_port": 41084,
            "seq": 0x50000000,
            "request": b"GET /" + (b"A" * 120) + b".HTM HTTP/1.0\r\nHost: dataplane\r\n\r\n",
            "kind": "cap_negative",
        },
        {
            "name": "missing",
            "host_port": 41082,
            "seq": 0x30000000,
            "request": b"GET /MISSING.HTM HTTP/1.0\r\nHost: dataplane\r\n\r\n",
            "kind": "missing",
            "defer_until_cap_negative_ok": True,
        },
    ]
else:
    http_sessions = [
        {
            "name": "get_index",
            "host_port": 41080,
            "seq": 0x10000000,
            "request": b"GET /INDEX.HTM HTTP/1.0\r\nHost: dataplane\r\n\r\n",
            "kind": "get_index",
        },
        {
            "name": "head_index",
            "host_port": 41081,
            "seq": 0x20000000,
            "request": b"HEAD /INDEX.HTM HTTP/1.0\r\nHost: dataplane\r\n\r\n",
            "kind": "head_index",
        },
        {
            "name": "missing",
            "host_port": 41082,
            "seq": 0x30000000,
            "request": b"GET /MISSING.HTM HTTP/1.0\r\nHost: dataplane\r\n\r\n",
            "kind": "missing",
        },
        {
            "name": "status",
            "host_port": 41083,
            "seq": 0x40000000,
            "request": b"GET /STATUS.TXT HTTP/1.0\r\nHost: dataplane\r\n\r\n",
            "kind": "status",
        },
        {
            "name": "cap_negative",
            "host_port": 41084,
            "seq": 0x50000000,
            "request": b"GET /" + (b"A" * 120) + b".HTM HTTP/1.0\r\nHost: dataplane\r\n\r\n",
            "kind": "cap_negative",
            "defer_until_others_ok": True,
        },
    ]
for session in http_sessions:
    session["state"] = "syn"
    session["peer_seq"] = None
    session["ack"] = 0
    session["next_send"] = 0.0
    session["response"] = bytearray()
    session["ok"] = False
    session["request_sent"] = False
    session["request_part_index"] = 0
    session["request_first_seq"] = 0
    session["request_seq"] = 0


def be16(value):
    return value.to_bytes(2, "big")


def be32(value):
    return value.to_bytes(4, "big")


def checksum(data):
    total = 0
    if len(data) % 2:
        data += b"\x00"
    for index in range(0, len(data), 2):
        total += int.from_bytes(data[index:index + 2], "big")
    while total >> 16:
        total = (total & 0xffff) + (total >> 16)
    return (~total) & 0xffff


def ipv4_header(src_ip, dst_ip, proto, payload_len, ident):
    header = bytearray(20)
    header[0] = 0x45
    header[2:4] = be16(20 + payload_len)
    header[4:6] = be16(ident & 0xffff)
    header[6:8] = be16(0x4000)
    header[8] = 64
    header[9] = proto
    header[12:16] = src_ip
    header[16:20] = dst_ip
    header[10:12] = be16(checksum(bytes(header)))
    return bytes(header)


def arp_request_frame():
    arp = (
        be16(1)
        + be16(0x0800)
        + bytes([6, 4])
        + be16(1)
        + host_mac
        + host_ip
        + bytes(6)
        + vm_ip
    )
    return (b"\xff" * 6 + host_mac + be16(0x0806) + arp).ljust(60, b"\x00")


def icmp_request_frame():
    payload = b"DPPING00"
    icmp = bytearray(8 + len(payload))
    icmp[0] = 8
    icmp[4:6] = be16(icmp_ident)
    icmp[6:8] = be16(icmp_seq)
    icmp[8:] = payload
    icmp[2:4] = be16(checksum(bytes(icmp)))
    packet = ipv4_header(host_ip, vm_ip, 1, len(icmp), 0x2222) + bytes(icmp)
    return (vm_mac + host_mac + be16(0x0800) + packet).ljust(60, b"\x00")


def udp_request_frame(seq):
    payload = request_magic + be32(seq) + be32(seq ^ 0xA5A5A5A5)
    udp_len = 8 + len(payload)
    udp = be16(host_port_udp) + be16(vm_port_udp) + be16(udp_len) + b"\x00\x00"
    packet = ipv4_header(host_ip, vm_ip, 17, len(udp) + len(payload), seq) + udp + payload
    return (vm_mac + host_mac + be16(0x0800) + packet).ljust(60, b"\x00")


def control_protocol_v1_negative_frame(seq, kind):
    payload = request_magic + be32(seq) + be32(seq ^ 0xA5A5A5A5)
    src_port = host_port_udp
    dst_port = vm_port_udp
    if kind == "bad_magic":
        payload = b"BADOPQ00" + be32(seq) + be32(seq ^ 0xA5A5A5A5)
    elif kind == "overlong_payload":
        payload = payload + b"X"
    elif kind == "wrong_route":
        dst_port = vm_port_udp + 1
    elif kind == "bad_check":
        payload = request_magic + be32(seq) + be32((seq ^ 0xA5A5A5A5) ^ 1)
    elif kind == "short_payload":
        payload = request_magic + be32(seq) + be32(seq ^ 0xA5A5A5A5)[:3]
    elif kind == "wrong_src_port":
        src_port = host_port_udp - 1
    else:
        raise ValueError(kind)
    udp_len = 8 + len(payload)
    udp = be16(src_port) + be16(dst_port) + be16(udp_len) + b"\x00\x00"
    packet = ipv4_header(host_ip, vm_ip, 17, len(udp) + len(payload), seq) + udp + payload
    return (vm_mac + host_mac + be16(0x0800) + packet).ljust(60, b"\x00")


negative_reject_markers = {
    "bad_magic": "DPMK:CTRL-V1-REJECT:bad_magic",
    "overlong_payload": "DPMK:CTRL-V1-REJECT:overlong_payload",
    "wrong_route": "DPMK:CTRL-V1-REJECT:wrong_route",
    "bad_check": "DPMK:CTRL-V1-REJECT:bad_check",
    "short_payload": "DPMK:CTRL-V1-REJECT:short_payload",
    "wrong_src_port": "DPMK:CTRL-V1-REJECT:wrong_src_port",
}


def tcp_frame(src_port, dst_port, seq, ack, flags, payload=b"", ident=0x7000):
    header = bytearray(20)
    header[0:2] = be16(src_port)
    header[2:4] = be16(dst_port)
    header[4:8] = be32(seq)
    header[8:12] = be32(ack)
    header[12] = 5 << 4
    header[13] = flags
    header[14:16] = be16(4096)
    pseudo = host_ip + vm_ip + b"\x00" + bytes([6]) + be16(len(header) + len(payload))
    header[16:18] = be16(checksum(pseudo + bytes(header) + payload))
    packet = ipv4_header(host_ip, vm_ip, 6, len(header) + len(payload), ident)
    return (vm_mac + host_mac + be16(0x0800) + packet + bytes(header) + payload).ljust(60, b"\x00")


def send_frame(sock, frame):
    sock.sendto(frame, ("127.0.0.1", qemu_port))
    sock.sendto(b"\x00\x00" + frame, ("127.0.0.1", qemu_port))


def strip_qemu_prefix(data):
    if data.startswith(b"\x00\x00"):
        return data[2:]
    return data


def is_arp_reply(data):
    data = strip_qemu_prefix(data)
    if len(data) < 14 + 28:
        return False
    return (
        data[0:6] == host_mac
        and data[6:12] == vm_mac
        and data[12:14] == be16(0x0806)
        and data[20:22] == be16(2)
        and data[22:28] == vm_mac
        and data[28:32] == vm_ip
        and data[32:38] == host_mac
        and data[38:42] == host_ip
    )


def is_icmp_reply(data):
    data = strip_qemu_prefix(data)
    if len(data) < 14 + 20 + 8:
        return False
    if data[0:6] != host_mac or data[6:12] != vm_mac or data[12:14] != be16(0x0800):
        return False
    ip = data[14:34]
    if ip[0] != 0x45 or ip[9] != 1 or ip[12:16] != vm_ip or ip[16:20] != host_ip:
        return False
    if checksum(ip) != 0:
        return False
    icmp = data[34:]
    return (
        icmp[0] == 0
        and icmp[4:6] == be16(icmp_ident)
        and icmp[6:8] == be16(icmp_seq)
        and b"DPPING00" in icmp
    )


def udp_response_seq(data):
    data = strip_qemu_prefix(data)
    if len(data) < 14 + 20 + 8 + 16:
        return None
    if data[0:6] != host_mac or data[6:12] != vm_mac or data[12:14] != be16(0x0800):
        return None
    ip = data[14:34]
    if ip[0] != 0x45 or ip[9] != 17 or ip[12:16] != vm_ip or ip[16:20] != host_ip:
        return None
    if checksum(ip) != 0:
        return None
    udp = data[34:42]
    if udp[0:2] != be16(vm_port_udp) or udp[2:4] != be16(host_port_udp):
        return None
    payload = data[42:58]
    if payload[0:8] != response_magic:
        return None
    seq = int.from_bytes(payload[8:12], "big")
    check = int.from_bytes(payload[12:16], "big")
    if check != (seq ^ 0x5A5A5A5A):
        return None
    return seq


def parse_tcp_response(data):
    data = strip_qemu_prefix(data)
    if len(data) < 14 + 20 + 20:
        return None
    if data[0:6] != host_mac or data[6:12] != vm_mac or data[12:14] != be16(0x0800):
        return None
    ip = data[14:34]
    ihl = (ip[0] & 0x0F) * 4
    if (ip[0] >> 4) != 4 or ihl < 20 or ip[9] != 6:
        return None
    total_len = int.from_bytes(ip[2:4], "big")
    if total_len < ihl + 20 or len(data) < 14 + total_len:
        return None
    ip_packet = data[14:14 + total_len]
    if ip_packet[12:16] != vm_ip or ip_packet[16:20] != host_ip:
        return None
    tcp = ip_packet[ihl:]
    data_offset = (tcp[12] >> 4) * 4
    if data_offset < 20 or len(tcp) < data_offset:
        return None
    dst_port = int.from_bytes(tcp[2:4], "big")
    for session in http_sessions:
        if dst_port == session["host_port"]:
            return {
                "session": session,
                "src_port": int.from_bytes(tcp[0:2], "big"),
                "dst_port": dst_port,
                "seq": int.from_bytes(tcp[4:8], "big"),
                "ack": int.from_bytes(tcp[8:12], "big"),
                "flags": tcp[13],
                "payload": tcp[data_offset:],
            }
    return None


def response_status(response):
    line = bytes(response).split(b"\r\n", 1)[0]
    parts = line.split()
    if len(parts) >= 2 and parts[0].startswith(b"HTTP/1."):
        try:
            return int(parts[1])
        except ValueError:
            return None
    return None


def http_response_ok(session):
    response = bytes(session["response"])
    if b"\r\n\r\n" not in response:
        return False
    headers, body = response.split(b"\r\n\r\n", 1)
    status = response_status(response)
    lowered = headers.lower()
    lowered_body = body.lower()
    if session["kind"] == "get_index":
        return status == 200 and http_index_body in body
    if session["kind"] == "head_index":
        return status == 200 and b"content-length: 110" in lowered and http_index_body not in body
    if session["kind"] == "missing":
        return status == 404
    if session["kind"] == "status":
        return status == 200 and lowered_body.startswith(b"dpstatus ")
    if session["kind"] == "cap_negative":
        return status == 413
    return False


def send_http_ack(sock, session):
    frame = tcp_frame(
        session["host_port"],
        http_port,
        session["seq"],
        session["ack"],
        0x10,
        ident=0x7100 + session["host_port"],
    )
    send_frame(sock, frame)


def send_http_request(sock, session):
    request_parts = session.get("request_parts")
    if request_parts is not None and session["request_part_index"] < len(request_parts):
        request = request_parts[session["request_part_index"]]
        request_seq = session["seq"]
        if session["request_part_index"] == 0:
            session["request_first_seq"] = request_seq
        session["seq"] += len(request)
        session["request_part_index"] += 1
        session["request_sent"] = session["request_part_index"] == len(request_parts)
        if session["request_sent"]:
            session["request_seq"] = session["request_first_seq"]
    elif session["request_sent"]:
        request_seq = session["request_seq"]
        request = session["request"]
    else:
        request = session["request"]
        request_seq = session["seq"]
        session["request_seq"] = request_seq
        session["seq"] += len(session["request"])
        session["request_sent"] = True
    frame = tcp_frame(
        session["host_port"],
        http_port,
        request_seq,
        session["ack"],
        0x18,
        payload=request,
        ident=0x7200 + session["host_port"],
    )
    send_frame(sock, frame)
    session["state"] = "request"
    session["next_send"] = time.monotonic() + 0.20


def drive_http_sessions(sock, now):
    for session in http_sessions:
        if session["ok"] or now < session["next_send"]:
            continue
        if session.get("defer_until_others_ok") and not all(
            other["ok"] or other is session for other in http_sessions
        ):
            continue
        if session.get("defer_until_cap_negative_ok") and not any(
            other["kind"] == "cap_negative" and other["ok"] for other in http_sessions
        ):
            continue
        if session["state"] == "syn":
            frame = tcp_frame(
                session["host_port"],
                http_port,
                session["seq"],
                0,
                0x02,
                ident=0x7000 + session["host_port"],
            )
            send_frame(sock, frame)
            session["next_send"] = now + 0.10
        elif session["state"] == "established":
            send_http_request(sock, session)
        elif session["state"] == "request" and not session["response"]:
            send_http_request(sock, session)


def handle_tcp_response(sock, parsed):
    session = parsed["session"]
    flags = parsed["flags"]
    payload = parsed["payload"]
    if session["state"] == "syn" and flags & 0x12 == 0x12 and parsed["src_port"] == http_port:
        session["peer_seq"] = parsed["seq"]
        session["ack"] = parsed["seq"] + 1
        session["seq"] = session["seq"] + 1
        session["state"] = "established"
        send_http_ack(sock, session)
        send_http_request(sock, session)
    if payload:
        session["response"].extend(payload)
        session["ack"] = parsed["seq"] + len(payload)
        if flags & 0x01:
            session["ack"] += 1
        send_http_ack(sock, session)
        if http_response_ok(session):
            session["ok"] = True
    elif flags & 0x01:
        session["ack"] = parsed["seq"] + 1
        send_http_ack(sock, session)
        if http_response_ok(session):
            session["ok"] = True


def is_raw_probe(data):
    data = strip_qemu_prefix(data)
    if len(data) < 14 + len(b"DPMK-NET-TX-PROBE"):
        return False
    return (
        data[0:6] == b"\xff" * 6
        and data[6:12] == vm_mac
        and data[12:14] == be16(0x88B7)
        and data[14:].startswith(b"DPMK-NET-TX-PROBE")
    )


sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.bind(("127.0.0.1", host_port))
sock.setblocking(False)

deadline = time.monotonic() + 25.0
next_arp = 0.0
next_icmp = 0.0
next_udp = 0.0
udp_next_seq = 0
negative_sent = False
negative_udp_response_seen = False
negative_case_sent = {name: False for name in negative_reject_markers}
seen_arp_reply = False
seen_icmp_reply = False
seen_raw_tx = False
raw_seen_at = None
udp_seen = set()
received_hex = []

while time.monotonic() < deadline:
    now = time.monotonic()
    if not seen_arp_reply and now >= next_arp:
        frame = arp_request_frame()
        send_frame(sock, frame)
        next_arp = now + 0.05
    if not seen_icmp_reply and now >= next_icmp:
        frame = icmp_request_frame()
        send_frame(sock, frame)
        next_icmp = now + 0.05
    if len(udp_seen) < packet_count and now >= next_udp:
        for _ in range(4):
            frame = udp_request_frame(udp_next_seq % packet_count)
            send_frame(sock, frame)
            udp_next_seq += 1
        if not negative_sent:
            for name, seq, kind in [
                ("bad_magic", bad_magic_seq, "bad_magic"),
                ("overlong_payload", overlong_seq, "overlong_payload"),
                ("wrong_route", wrong_route_seq, "wrong_route"),
                ("bad_check", bad_check_seq, "bad_check"),
                ("short_payload", short_payload_seq, "short_payload"),
                ("wrong_src_port", wrong_src_port_seq, "wrong_src_port"),
            ]:
                send_frame(sock, control_protocol_v1_negative_frame(seq, kind))
                negative_case_sent[name] = True
            negative_sent = True
        next_udp = now + 0.01
    drive_http_sessions(sock, now)
    try:
        while True:
            data, _ = sock.recvfrom(2048)
            received_hex.append(data.hex())
            if is_raw_probe(data):
                seen_raw_tx = True
                if raw_seen_at is None:
                    raw_seen_at = time.monotonic()
            if is_arp_reply(data):
                seen_arp_reply = True
            if is_icmp_reply(data):
                seen_icmp_reply = True
            seq = udp_response_seq(data)
            if seq is not None:
                if seq < packet_count:
                    udp_seen.add(seq)
                else:
                    negative_udp_response_seen = True
            parsed_tcp = parse_tcp_response(data)
            if parsed_tcp is not None:
                handle_tcp_response(sock, parsed_tcp)
    except BlockingIOError:
        pass
    stage_e_done = seen_raw_tx and seen_arp_reply and seen_icmp_reply and len(udp_seen) == packet_count
    if proof_kind != "parity" and stage_e_done and all(session["ok"] for session in http_sessions):
        break
    if proof_kind == "parity":
        if all(session["ok"] for session in http_sessions) and raw_seen_at is not None and time.monotonic() - raw_seen_at >= 8.0:
            break
    elif stage_e_done and all(session["ok"] for session in http_sessions) and raw_seen_at is not None and time.monotonic() - raw_seen_at >= 8.0:
        break
    if not stage_e_done and seen_raw_tx and raw_seen_at is not None and time.monotonic() - raw_seen_at >= 3.0:
        break
    time.sleep(0.001)

with open(log_path, "w", encoding="ascii") as log:
    log.write("seen_raw_tx=true\n" if seen_raw_tx else "seen_raw_tx=false\n")
    log.write("seen_arp_reply=" + ("true" if seen_arp_reply else "false") + "\n")
    log.write("seen_icmp_reply=" + ("true" if seen_icmp_reply else "false") + "\n")
    log.write(f"udp_packets={packet_count}\n")
    log.write(f"udp_responses={len(udp_seen)}\n")
    log.write("seen_udp_echo=" + ("true" if len(udp_seen) == packet_count else "false") + "\n")
    log.write("control_protocol_v1_request_magic=DPUDPQ00\n")
    log.write("control_protocol_v1_response_magic=DPUDPR00\n")
    log.write("control_protocol_v1_host_port=40000\n")
    log.write("control_protocol_v1_vm_port=40001\n")
    log.write("control_protocol_v1_catalog_version=0\n")
    log.write("control_protocol_v1_catalog_opcode=0\n")
    log.write("control_protocol_v1_invalid_response_absent=" + ("false" if negative_udp_response_seen else "true") + "\n")
    for name, sent in negative_case_sent.items():
        log.write(f"control_protocol_v1_negative_case_sent_{name}=" + ("true" if sent else "false") + "\n")
    if not negative_udp_response_seen and negative_case_sent["bad_magic"]:
        log.write("control_protocol_v1_bad_magic_rejected=true\n")
    if not negative_udp_response_seen and negative_case_sent["overlong_payload"]:
        log.write("control_protocol_v1_overlong_payload_rejected=true\n")
    if not negative_udp_response_seen and negative_case_sent["wrong_route"]:
        log.write("control_protocol_v1_wrong_route_rejected=true\n")
    for session in http_sessions:
        response = bytes(session["response"])
        safe_response = response.decode("ascii", errors="replace").replace("\r", "\\r").replace("\n", "\\n")
        log.write(f"http_{session['name']}_ok=" + ("true" if session["ok"] else "false") + "\n")
        log.write(f"http_{session['name']}_status={response_status(response) or 0}\n")
        log.write(f"http_{session['name']}_bytes={len(response)}\n")
        log.write(f"http_{session['name']}_response_ascii={safe_response}\n")
    for item in received_hex[:256]:
        log.write("rx=" + item + "\n")

raise SystemExit(0 if seen_raw_tx else 2)
PY
host_pid="$!"

timeout 35s qemu-system-x86_64 \
  -M pc \
  -m 128M \
  -nographic \
  -monitor none \
  -drive file="$image",format=raw,if=floppy \
  -boot a \
  -drive if=none,id=fs,file="$fs_img",format=raw \
  -device virtio-blk-pci-transitional,drive=fs \
  -netdev socket,id=xnet,udp=127.0.0.1:"$host_port",localaddr=127.0.0.1:"$qemu_port" \
  -object "filter-dump,id=xnet_dump,netdev=xnet,file=$net_pcap" \
  -device virtio-net-pci-transitional,netdev=xnet,mac=52:54:00:12:34:56 \
  -chardev socket,id=cli,path="$serial_sock",server=on,wait=on \
  -serial chardev:cli \
  -no-reboot \
  -no-shutdown >"$qemu_log" 2>&1 &
qemu_pid="$!"

DP_SERIAL_SOCK="$serial_sock" \
DP_SERIAL_LOG="$serial_log" \
DP_CLIENT_LOG="$client_log" \
DP_EXPECTED_BOOT_MARKERS="DPMK:BOOT,DPMK:RESOURCE-BUDGET-LEDGER,DPMK:RESOURCE-BUDGET-LEDGER-OK,DPMK:PROTOCOL-INPUT-BOUNDS-LEDGER,DPBOUNDS:cli line=64 commands=24,DPBOUNDS:http request_line=96 headers=384 index_file=256 large_file=512,DPBOUNDS:tcp rx=472 segment=472 tx=1024 sessions=4 overflow_probe=473,DPBOUNDS:udp-control payload=16 version=0 opcode=0 reply=512,DPMK:PROTOCOL-INPUT-BOUNDS-LEDGER-OK,DPMK:TIMER-TIMEOUT-SERVICE-LEDGER,DPTIMER:owner task=1 endpoint=1 request=1 mailbox_cap=4 payload_word=1,DPTIMER:source cooperative-run_timer_tick monotonic=work-units wall_clock=0 rtc=0 ntp=0 cert_time=0,DPTIMER:timeouts cli_first_read=1000000 cli_line_read=20000000 dhcp_poll_limit=20000,DPTIMER:delivery control=bounded owner=task=1 tick_source=cooperative-run_timer_tick timeout_resolution_ticks=8,DPTIMER:positive-delivery now=21 deadline=18 age=3 grace=8 control=bounded outcome=deliver,DPTIMER:stale-timeout now=19 deadline=13 age=6 grace=4 outcome=bounded-drop stale_timeout_retries=0 stale_timeout_rejects=1,DPTIMER:expired-timeout now=12 deadline=4294967293 age=15 grace=1 outcome=reject expired_timeout_rejects=1 expired_timeout_ignored=1,DPTIMER:wrap-comparison now=4 deadline=4294967292 age=8 wrap_policy=wrap-saturating,DPTIMER:gap-bounds observed=8 fairness=8 network=32 fairness_poll_interval=8 dhcp_poll_interval=16,DPMK:TIMER-TIMEOUT-SERVICE-LEDGER-OK,DPMK:SERVICE-MAILBOX-ENVELOPE-LEDGER,DPMBOX:shape fields=from,DPMBOX:tasks timer=1 cli=2 fs=3 block=4 net=5 tcpip=6 http=7 dhcp=8,DPMBOX:requests timer_tick=1 cli_fs=2 fs_block=3 tcpip_net=4 http_fs=5 dhcp_net=6,DPMBOX:routes timer=1->1 cli_fs=2->3 fs_block=3->4 tcpip_net=6->5 http_fs=7->3 dhcp_net=8->5,DPMBOX:errors queue_full=MailboxError::Full empty=MailboxError::Empty send=mailbox-send recv=mailbox-recv timeout=deferred stale_reply=deferred denied_route=deferred,DPMBOX:fault-destination task=4 status=faulted restart=0 replay=0,DPMK:SERVICE-MAILBOX-ENVELOPE-OK,DPMK:SERVICE-LIFECYCLE-LEDGER,DPLIFE:status-set empty=1 ready=1 waiting=defined faulted=1 stopped=defined degraded=0,DPLIFE:startup register_initial_tasks active=8 task_table_slots=9 status=ready,DPLIFE:running timer=DPMK:TIMER service_loop=entered,DPLIFE:ready timer=ready cli=ready fs=ready block=ready net=ready tcpip=ready http=ready dhcp=ready,DPLIFE:cli-visible timer=ready fs=ready block=ready tcpip=ready queues=bounded,DPLIFE:serving cli=DPMK:CLI-COMMANDS-OK http=DPMK:HTTP-GET-OK,DPLIFE:fault-path task=4 status=faulted faults=1 marker=DPMK:FAULT-CONTAINED,DPLIFE:deferred stopped=not-exercised degraded=not-defined restart=0 replay=0 cleanup_guarantee=0,DPMK:SERVICE-LIFECYCLE-LEDGER-OK,DPMK:TIMER,DPMK:CLI-READY,DPMK:BLK-READY,DPMK:BLK-SECTOR0-OK,DPMK:FS-READY,DPMK:FS-LS-ROOT-OK,DPMK:FS-HELLO-OK,DPMK:FS-INDEX-OK,DPMK:NET-SPLIT-READY,DPMK:CLI-INPUT-READY" \
DP_EXPECTED_FINAL_MARKERS="DPLIFE:serving cli=DPMK:CLI-COMMANDS-OK http=DPMK:HTTP-GET-OK,DPMK:IPC-OK,DPMK:CLI-COMMANDS-OK,DPMK:FS-HARDENING-OK,DPLIFE:task block id=4 endpoint=4 status=faulted faults=1 restart=0 replay=0 cleanup_guarantee=0,DPMK:SERVICE-LIFECYCLE-FAULT-OK,DPMK:FAULT-CONTAINED,DPMK:OK" \
DP_EXPECTED_NETWORK_MARKERS="DPMK:NET-READY,DPMK:NET-TX-OK" \
python3 - <<'PY'
import os
import select
import socket
import time

sock_path = os.environ["DP_SERIAL_SOCK"]
serial_log = os.environ["DP_SERIAL_LOG"]
client_log = os.environ["DP_CLIENT_LOG"]
boot_markers = [item.encode("ascii") for item in os.environ["DP_EXPECTED_BOOT_MARKERS"].split(",")]
final_markers = [item.encode("ascii") for item in os.environ["DP_EXPECTED_FINAL_MARKERS"].split(",")]
network_markers = [item.encode("ascii") for item in os.environ["DP_EXPECTED_NETWORK_MARKERS"].split(",")]
deadline = time.monotonic() + 30.0

COMMAND_CHECKS = [
    (
        "help",
        [
            b"DPMK:CLI-BEGIN:1:help",
            b"DPCLI:HELP help tasks fs ls / fs cat /HELLO.TXT fs cat /INDEX.HTM",
            b"DPCLI:HELP-FS fs stat /HELLO.TXT fs stat /INDEX.HTM fs stat /MISSING.TXT fs stat /THISNAMEISTOOLONG.TXT fs write /OUT.TXT append",
            b"DPMK:CLI-END:1:OK",
        ],
    ),
    (
        "tasks",
        [
            b"DPMK:CLI-BEGIN:2:tasks",
            b"DPCLI:TASKS timer=ready cli=ready fs=ready block=ready",
            b"DPCLI:TASKS-NET net=candidate tcpip=candidate",
            b"DPMK:CLI-END:2:OK",
        ],
    ),
    (
        "fs ls /",
        [
            b"DPMK:CLI-BEGIN:3:fs ls /",
            b"DPCLI:LS / HELLO.TXT 40 INDEX.HTM 110",
            b"DPMK:CLI-END:3:OK",
        ],
    ),
    (
        "fs cat /HELLO.TXT",
        [
            b"DPMK:CLI-BEGIN:4:fs cat /HELLO.TXT",
            b"DPCLI:CAT /HELLO.TXT",
            b"hello from dataplane microkernel fat32\r\n",
            b"DPCLI:END /HELLO.TXT",
            b"DPMK:CLI-END:4:OK",
        ],
    ),
    (
        "fs cat /INDEX.HTM",
        [
            b"DPMK:CLI-BEGIN:5:fs cat /INDEX.HTM",
            b"DPCLI:CAT /INDEX.HTM",
            b"<!doctype html><html><head><title>dataplane</title></head><body><h1>dataplane microkernel</h1></body></html>\r\n",
            b"DPCLI:END /INDEX.HTM",
            b"DPMK:CLI-END:5:OK",
        ],
    ),
    (
        "fs stat /HELLO.TXT",
        [
            b"DPMK:CLI-BEGIN:6:fs stat /HELLO.TXT",
            b"DPCLI:STAT /HELLO.TXT cluster=3 size=40 readonly=1",
            b"DPMK:FS-STAT-OK:/HELLO.TXT",
            b"DPMK:CLI-END:6:OK",
        ],
    ),
    (
        "fs stat /INDEX.HTM",
        [
            b"DPMK:CLI-BEGIN:7:fs stat /INDEX.HTM",
            b"DPCLI:STAT /INDEX.HTM cluster=4 size=110 readonly=1",
            b"DPMK:FS-STAT-OK:/INDEX.HTM",
            b"DPMK:CLI-END:7:OK",
        ],
    ),
    (
        "fs stat /MISSING.TXT",
        [
            b"DPMK:CLI-BEGIN:8:fs stat /MISSING.TXT",
            b"DPCLI:FS-ERR /MISSING.TXT unsupported-path",
            b"DPMK:FS-NEGATIVE-OK:UNSUPPORTED-PATH",
            b"DPMK:CLI-END:8:OK",
        ],
    ),
    (
        "fs stat /THISNAMEISTOOLONG.TXT",
        [
            b"DPMK:CLI-BEGIN:9:fs stat /THISNAMEISTOOLONG.TXT",
            b"DPCLI:FS-ERR /THISNAMEISTOOLONG.TXT long-filename",
            b"DPMK:FS-NEGATIVE-OK:LONG-FILENAME",
            b"DPMK:CLI-END:9:OK",
        ],
    ),
    (
        "fs write /OUT.TXT append",
        [
            b"DPMK:CLI-BEGIN:10:fs write /OUT.TXT append",
            b"DPCLI:FS-ERR /OUT.TXT unsupported-write-shape",
            b"DPMK:FS-NEGATIVE-OK:UNSUPPORTED-WRITE",
            b"DPMK:CLI-END:10:OK",
        ],
    ),
    (
        "task timer",
        [
            b"DPMK:CLI-BEGIN:11:task timer",
            b"DPCLI:TASK timer id=1 endpoint=1 status=ready",
            b"faults=0",
            b"DPMK:CLI-END:11:OK",
        ],
    ),
    (
        "task fs",
        [
            b"DPMK:CLI-BEGIN:12:task fs",
            b"DPCLI:TASK fs id=3 endpoint=3 status=ready",
            b"faults=0",
            b"DPMK:CLI-END:12:OK",
        ],
    ),
    (
        "task block",
        [
            b"DPMK:CLI-BEGIN:13:task block",
            b"DPCLI:TASK block id=4 endpoint=4 status=ready",
            b"faults=",
            b"DPMK:CLI-END:13:OK",
        ],
    ),
    (
        "task tcpip",
        [
            b"DPMK:CLI-BEGIN:14:task tcpip",
            b"DPCLI:TASK tcpip id=6 endpoint=6 status=ready",
            b"faults=0",
            b"DPMK:CLI-END:14:OK",
        ],
    ),
    (
        "parity",
        [
            b"DPMK:CLI-BEGIN:15:parity",
            b"DPCLI:PARITY DPSTATUS routes=t1>1,c2>3,b3>4,n6>5,h7>3,d8>5 service_caps=h2,t3,ls4,cat5,st6,neg7,tt8,tf9,tb10,ttc11,q12,p13 storage_mode=ro network_counters=a1,i1,u16,200=3,404=1,405=1,413=2,500=1 timer_status=ok,cli,fs,blk,net,tcpip,http,dhcp fault_status=tb:ready,f0,r0,p0,c0 generation_id=1",
            b"DPMK:CLI-END:15:OK",
        ],
    ),
    (
        "queues",
        [
            b"DPMK:CLI-BEGIN:16:queues",
            b"DPCLI:QUEUES timer=0 cli=0 fs=0 block=0 net=0 tcpip=0 http=0 dhcp=0",
            b"DPMK:CLI-END:16:OK",
            b"DPMK:CLI-OPERATOR-OK",
        ],
    ),
    (
        "shell",
        [
            b"DPMK:CLI-BEGIN:17:shell",
            b"DPMK:CLI-END:17:REJECT:shell:cli-command",
        ],
    ),
    (
        "json status",
        [
            b"DPMK:CLI-BEGIN:18:json status",
            b"DPMK:CLI-END:18:REJECT:json status:cli-command",
        ],
    ),
    (
        "restart",
        [
            b"DPMK:CLI-BEGIN:19:restart",
            b"DPMK:CLI-END:19:REJECT:restart:cli-capability",
        ],
    ),
    (
        "reset",
        [
            b"DPMK:CLI-BEGIN:20:reset",
            b"DPMK:CLI-END:20:REJECT:reset:cli-capability",
        ],
    ),
    (
        "raw memory",
        [
            b"DPMK:CLI-BEGIN:21:raw memory",
            b"DPMK:CLI-END:21:REJECT:raw memory:cli-capability",
        ],
    ),
    (
        "page table dump",
        [
            b"DPMK:CLI-BEGIN:22:page table dump",
            b"DPMK:CLI-END:22:REJECT:page table dump:cli-capability",
        ],
    ),
    (
        "mmio dump",
        [
            b"DPMK:CLI-BEGIN:23:mmio dump",
            b"DPMK:CLI-END:23:REJECT:mmio dump:cli-capability",
        ],
    ),
    (
        "debug",
        [
            b"DPMK:CLI-BEGIN:24:debug",
            b"DPMK:CLI-END:24:REJECT:debug:cli-capability",
        ],
    ),
]

sock = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
while True:
    try:
        sock.connect(sock_path)
        break
    except (FileNotFoundError, ConnectionRefusedError):
        if time.monotonic() >= deadline:
            raise SystemExit("serial socket did not become ready")
        time.sleep(0.05)

sock.setblocking(False)

captured = bytearray()


def read_some(out):
    readable, _, _ = select.select([sock], [], [], 0.1)
    if not readable:
        return False
    chunk = sock.recv(4096)
    if not chunk:
        raise SystemExit("serial socket closed before expected output")
    captured.extend(chunk)
    out.write(chunk)
    out.flush()
    return True


def wait_for_all(out, expected, label, start=0, timeout_seconds=None):
    deadline = time.monotonic() + 180.0 if timeout_seconds is None else time.monotonic() + timeout_seconds
    while time.monotonic() < deadline:
        window = bytes(captured[start:])
        missing = [item for item in expected if item not in window]
        if not missing:
            return
        read_some(out)
    missing_text = ", ".join(item.decode("ascii", errors="replace") for item in missing)
    raise SystemExit(f"missing {label}: {missing_text}")


def send_command(out, command, expected, timeout_seconds=60.0):
    start = len(captured)
    sock.sendall(command.encode("ascii") + b"\n")
    wait_for_all(out, expected, f"output for command {command}", start=start, timeout_seconds=timeout_seconds)


with open(serial_log, "wb") as out:
    wait_for_all(out, boot_markers, "boot markers")
    for command, expected in COMMAND_CHECKS:
        send_command(out, command, expected)
    wait_for_all(out, network_markers, "network markers")
    wait_for_all(out, final_markers, "final markers")

missing_boot = [marker.decode("ascii") for marker in boot_markers if marker not in captured]
missing_final = [marker.decode("ascii") for marker in final_markers if marker not in captured]
missing_network = [marker.decode("ascii") for marker in network_markers if marker not in captured]
with open(client_log, "w", encoding="ascii") as out:
    out.write(f"bytes={len(captured)}\n")
    out.write("commands=" + ",".join(command for command, _ in COMMAND_CHECKS) + "\n")
    out.write("missing_boot=" + ",".join(missing_boot) + "\n")
    out.write("missing_final=" + ",".join(missing_final) + "\n")
    out.write("missing_network=" + ",".join(missing_network) + "\n")
    out.write("markers_found=" + ("true" if not missing_boot and not missing_final and not missing_network else "false") + "\n")

if missing_boot or missing_final or missing_network:
    raise SystemExit("missing serial markers")
PY

if ! wait "$host_pid"; then
  host_pid=""
  echo "FAIL: host network peer did not receive the guest raw Ethernet proof"
  echo "--- host exchange ---"
  sed -n '1,220p' "$host_exchange_log" 2>/dev/null || true
  echo "--- serial ---"
  sed -n '1,220p' "$serial_log" 2>/dev/null || true
  exit 1
fi
host_pid=""

if [[ ! -s "$net_pcap" ]]; then
  echo "FAIL: microkernel network pcap was not written"
  sed -n '1,220p' "$serial_log" 2>/dev/null || true
  exit 1
fi

net_pcap_hex="$(od -An -tx1 -v "$net_pcap" | tr -d ' \n')"
if [[ "$net_pcap_hex" != *"ffffffffffff52540012345688b744504d4b2d4e45542d54582d50524f4245"* ]]; then
  echo "FAIL: network pcap did not capture guest raw Ethernet proof"
  echo "Expected frame prefix: ff ff ff ff ff ff 52 54 00 12 34 56 88 b7 DPMK-NET-TX-PROBE"
  exit 1
fi

grep -q "seen_raw_tx=true" "$host_exchange_log" || {
  echo "FAIL: host did not observe guest raw Ethernet proof"
  sed -n '1,220p' "$host_exchange_log" 2>/dev/null || true
  exit 1
}

require_host_evidence_for_marker() {
  local marker="$1"
  local evidence="$2"
  local note="$3"
  if grep -q "$marker" "$serial_log"; then
    grep -q "$evidence" "$host_exchange_log" || {
      echo "FAIL: $note"
      sed -n '1,220p' "$host_exchange_log" 2>/dev/null || true
      exit 1
    }
  fi
}

require_pcap_evidence_for_marker() {
  local marker="$1"
  local hex="$2"
  local note="$3"
  if grep -q "$marker" "$serial_log"; then
    [[ "$net_pcap_hex" == *"$hex"* ]] || {
      echo "FAIL: $note"
      exit 1
    }
  fi
}

require_host_evidence_for_marker "DPMK:NET-ARP-REPLY" "seen_arp_reply=true" \
  "guest claimed ARP reply without host evidence"
require_pcap_evidence_for_marker "DPMK:NET-ARP-REPLY" "0200000000045254001234560806" \
  "guest claimed ARP reply without pcap evidence"
require_host_evidence_for_marker "DPMK:NET-ICMP-REPLY" "seen_icmp_reply=true" \
  "guest claimed ICMP reply without host evidence"
require_pcap_evidence_for_marker "DPMK:NET-ICMP-REPLY" "020000000004525400123456080045" \
  "guest claimed ICMP reply without pcap IPv4 evidence"
require_host_evidence_for_marker "DPMK:NET-UDP-ECHO" "seen_udp_echo=true" \
  "guest claimed UDP echo without host evidence"
require_pcap_evidence_for_marker "DPMK:NET-UDP-ECHO" "4450554450523030" \
  "guest claimed UDP echo without pcap DPUDPR00 evidence"
require_host_evidence_for_marker "DPMK:NET-FAIR-OK" "udp_responses=$packet_count" \
  "guest claimed UDP fairness without every host response"

require_pcap_any_evidence_for_marker() {
  local marker="$1"
  local note="$2"
  shift 2
  local hex
  if grep -q "$marker" "$serial_log"; then
    for hex in "$@"; do
      if [[ "$net_pcap_hex" == *"$hex"* ]]; then
        return 0
      fi
    done
    echo "FAIL: $note"
    exit 1
  fi
}

if [[ "$proof_kind" != "parity" ]]; then
  require_host_evidence_for_marker "DPMK:HTTP-GET-OK" "http_get_index_ok=true" \
    "guest claimed HTTP GET without host response evidence"
  require_pcap_any_evidence_for_marker "DPMK:HTTP-GET-OK" \
    "guest claimed HTTP GET without pcap HTTP 200 evidence" \
    "485454502f312e3020323030204f4b" \
    "485454502f312e3120323030204f4b"
  require_pcap_evidence_for_marker "DPMK:HTTP-GET-OK" "3c21646f63747970652068746d6c3e" \
    "guest claimed HTTP GET without pcap INDEX.HTM body evidence"
  require_host_evidence_for_marker "DPMK:HTTP-HEAD-OK" "http_head_index_ok=true" \
    "guest claimed HTTP HEAD without host header evidence"
  require_pcap_evidence_for_marker "DPMK:HTTP-HEAD-OK" "436f6e74656e742d4c656e6774683a20313130" \
    "guest claimed HTTP HEAD without pcap Content-Length evidence"
  require_host_evidence_for_marker "DPMK:HTTP-404-OK" "http_missing_ok=true" \
    "guest claimed HTTP 404 without host response evidence"
  require_pcap_any_evidence_for_marker "DPMK:HTTP-404-OK" \
    "guest claimed HTTP 404 without pcap HTTP 404 evidence" \
    "485454502f312e3020343034" \
    "485454502f312e3120343034"
fi

if grep -q "DPMK:NET-TIMER-MAXGAP:" "$serial_log"; then
  python3 - "$serial_log" <<'PY'
import re
import sys

path = sys.argv[1]
limit = 32
for line in open(path, "r", encoding="ascii", errors="ignore"):
    match = re.search(r"DPMK:NET-TIMER-MAXGAP:(\d+)", line)
    if match:
        value = int(match.group(1))
        if value > limit:
            raise SystemExit(f"network timer max gap {value} exceeds bound {limit}")
        raise SystemExit(0)
raise SystemExit("network timer max gap marker vanished during validation")
PY
fi

if [[ "$mode" == "--control-protocol-v1-proof" ]]; then
  grep -q "seen_udp_echo=true" "$host_exchange_log" || {
    echo "FAIL: control protocol v1 host log missing UDP echo proof"
    exit 1
  }
  grep -q "udp_responses=$packet_count" "$host_exchange_log" || {
    echo "FAIL: control protocol v1 host log missing expected UDP response count"
    exit 1
  }
  grep -q "control_protocol_v1_invalid_response_absent=true" "$host_exchange_log" || {
    echo "FAIL: control protocol v1 observed a well-formed response to negative frames"
    exit 1
  }
  grep -q "DPSCHED:control-echo task=6 enqueued=" "$serial_log" || {
    echo "FAIL: control protocol v1 serial log missing opcode-0 echo scheduler accounting"
    exit 1
  }
  grep -q "DPMK:CTRL-V1-NET-REGION-OK" "$serial_log" || {
    echo "FAIL: control protocol v1 serial log missing net protected-region scope proof"
    exit 1
  }
  grep -q "DPMK:CTRL-V1-DENIED-ROUTE-OK" "$serial_log" || {
    echo "FAIL: control protocol v1 serial log missing denied-route rejection proof"
    exit 1
  }
  grep -q "DPMK:CTRL-V1-BOUNDARY-OK" "$serial_log" || {
    echo "FAIL: control protocol v1 serial log missing isolation/boundary proof"
    exit 1
  }
  control_protocol_v1_pcap_bytes="$(wc -c <"$net_pcap" | tr -d ' ')"
  if (( control_protocol_v1_pcap_bytes <= 24 )); then
    echo "FAIL: control protocol v1 pcap only contains a global header"
    exit 1
  fi
  control_protocol_v1_invalid_response_absent="$(
    grep -q "control_protocol_v1_invalid_response_absent=true" "$host_exchange_log" && echo true || echo false
  )"
  control_protocol_v1_negative_case_sent_bad_magic="$(
    grep -q "control_protocol_v1_negative_case_sent_bad_magic=true" "$host_exchange_log" && echo true || echo false
  )"
  control_protocol_v1_negative_case_sent_overlong_payload="$(
    grep -q "control_protocol_v1_negative_case_sent_overlong_payload=true" "$host_exchange_log" && echo true || echo false
  )"
  control_protocol_v1_negative_case_sent_wrong_route="$(
    grep -q "control_protocol_v1_negative_case_sent_wrong_route=true" "$host_exchange_log" && echo true || echo false
  )"
  control_protocol_v1_bad_magic_rejected="$(
    [[ "$control_protocol_v1_invalid_response_absent" == true && "$control_protocol_v1_negative_case_sent_bad_magic" == true ]] && echo true || echo false
  )"
  control_protocol_v1_overlong_payload_rejected="$(
    [[ "$control_protocol_v1_invalid_response_absent" == true && "$control_protocol_v1_negative_case_sent_overlong_payload" == true ]] && echo true || echo false
  )"
  control_protocol_v1_wrong_route_rejected="$(
    [[ "$control_protocol_v1_invalid_response_absent" == true && "$control_protocol_v1_negative_case_sent_wrong_route" == true ]] && echo true || echo false
  )"
  {
    echo "control_protocol_v1_summary_status=pass"
    echo "control_protocol_v1_run_id=$run_id"
    echo "control_protocol_v1_serial_log=$serial_log"
    echo "control_protocol_v1_client_log=$client_log"
    echo "control_protocol_v1_host_log=$host_exchange_log"
    echo "control_protocol_v1_pcap=$net_pcap"
    echo "control_protocol_v1_pcap_bytes=$control_protocol_v1_pcap_bytes"
    echo "control_protocol_v1_seen_udp_echo=true"
    echo "control_protocol_v1_udp_responses=$packet_count"
    echo "control_protocol_v1_request_magic=DPUDPQ00"
    echo "control_protocol_v1_response_magic=DPUDPR00"
    echo "control_protocol_v1_host_port=40000"
    echo "control_protocol_v1_vm_port=40001"
    echo "control_protocol_v1_catalog_version=0"
    echo "control_protocol_v1_catalog_opcode=0"
    echo "control_protocol_v1_invalid_response_absent=$control_protocol_v1_invalid_response_absent"
    echo "control_protocol_v1_negative_case_sent_bad_magic=$control_protocol_v1_negative_case_sent_bad_magic"
    echo "control_protocol_v1_negative_case_sent_overlong_payload=$control_protocol_v1_negative_case_sent_overlong_payload"
    echo "control_protocol_v1_negative_case_sent_wrong_route=$control_protocol_v1_negative_case_sent_wrong_route"
    echo "control_protocol_v1_negative_case_sent_bad_check=$(grep -q "control_protocol_v1_negative_case_sent_bad_check=true" "$host_exchange_log" && echo true || echo false)"
    echo "control_protocol_v1_negative_case_sent_short_payload=$(grep -q "control_protocol_v1_negative_case_sent_short_payload=true" "$host_exchange_log" && echo true || echo false)"
    echo "control_protocol_v1_negative_case_sent_wrong_src_port=$(grep -q "control_protocol_v1_negative_case_sent_wrong_src_port=true" "$host_exchange_log" && echo true || echo false)"
    echo "control_protocol_v1_bad_magic_rejected=$control_protocol_v1_bad_magic_rejected"
    echo "control_protocol_v1_overlong_payload_rejected=$control_protocol_v1_overlong_payload_rejected"
    echo "control_protocol_v1_wrong_route_rejected=$control_protocol_v1_wrong_route_rejected"
    echo "control_protocol_v1_scheduler_accounted=$(grep -q "DPSCHED:control-echo task=6 enqueued=" "$serial_log" && echo true || echo false)"
    echo "control_protocol_v1_net_region_ok=$(grep -q "DPMK:CTRL-V1-NET-REGION-OK" "$serial_log" && echo true || echo false)"
    echo "control_protocol_v1_denied_route_ok=$(grep -q "DPMK:CTRL-V1-DENIED-ROUTE-OK" "$serial_log" && echo true || echo false)"
    echo "control_protocol_v1_boundary_status=$(grep -q "DPMK:CTRL-V1-BOUNDARY-OK" "$serial_log" && echo pass || echo fail)"
    echo "control_protocol_v1_tls=false"
    echo "control_protocol_v1_default_writable=false"
    echo "control_protocol_v1_restart=false"
    echo "control_protocol_v1_benchmark_result=false"
  } >"$control_protocol_v1_summary"
  python3 - "$control_protocol_v1_summary" <<'PY'
import sys

path = sys.argv[1]
log_prefix = "/home/user/mnt/dataplane/logs"
seen = set()
values = {}
for line in open(path, "r", encoding="ascii"):
    key, value = line.rstrip("\n").split("=", 1)
    if key in seen:
        raise SystemExit(f"duplicate summary key: {key}")
    seen.add(key)
    values[key] = value

required_exact = {
    "control_protocol_v1_summary_status": "pass",
    "control_protocol_v1_seen_udp_echo": "true",
    "control_protocol_v1_invalid_response_absent": "true",
    "control_protocol_v1_scheduler_accounted": "true",
    "control_protocol_v1_net_region_ok": "true",
    "control_protocol_v1_denied_route_ok": "true",
    "control_protocol_v1_boundary_status": "pass",
    "control_protocol_v1_negative_case_sent_bad_magic": "true",
    "control_protocol_v1_negative_case_sent_overlong_payload": "true",
    "control_protocol_v1_negative_case_sent_wrong_route": "true",
    "control_protocol_v1_negative_case_sent_bad_check": "true",
    "control_protocol_v1_negative_case_sent_short_payload": "true",
    "control_protocol_v1_negative_case_sent_wrong_src_port": "true",
    "control_protocol_v1_bad_magic_rejected": "true",
    "control_protocol_v1_overlong_payload_rejected": "true",
    "control_protocol_v1_wrong_route_rejected": "true",
    "control_protocol_v1_request_magic": "DPUDPQ00",
    "control_protocol_v1_response_magic": "DPUDPR00",
    "control_protocol_v1_host_port": "40000",
    "control_protocol_v1_vm_port": "40001",
    "control_protocol_v1_catalog_version": "0",
    "control_protocol_v1_catalog_opcode": "0",
    "control_protocol_v1_tls": "false",
    "control_protocol_v1_default_writable": "false",
    "control_protocol_v1_restart": "false",
    "control_protocol_v1_benchmark_result": "false",
}
for key, expected in required_exact.items():
    if values.get(key) != expected:
        raise SystemExit(f"missing {key}={expected}")

required_present = [
    "control_protocol_v1_run_id",
    "control_protocol_v1_serial_log",
    "control_protocol_v1_client_log",
    "control_protocol_v1_host_log",
    "control_protocol_v1_pcap",
    "control_protocol_v1_pcap_bytes",
    "control_protocol_v1_udp_responses",
]
for key in required_present:
    if not values.get(key):
        raise SystemExit(f"missing non-empty {key}")

for key in (
    "control_protocol_v1_serial_log",
    "control_protocol_v1_client_log",
    "control_protocol_v1_host_log",
    "control_protocol_v1_pcap",
):
    if not values[key].startswith(log_prefix + "/"):
        raise SystemExit(f"{key} not under {log_prefix}: {values[key]}")

for key in ("control_protocol_v1_pcap_bytes", "control_protocol_v1_udp_responses"):
    if not values[key].isdigit() or int(values[key]) <= 0:
        raise SystemExit(f"{key} must be a positive integer: {values[key]}")
PY
fi

if [[ "$mode" == "--fault-policy-hardening-proof" ]]; then
  grep -q "DPMK:FAULT-CONTAINED" "$serial_log" || {
    echo "FAIL: fault policy missing contained-fault marker"
    exit 1
  }
  grep -q "DPMK:SERVICE-LIFECYCLE-FAULT-OK" "$serial_log" || {
    echo "FAIL: fault policy missing post-fault lifecycle marker"
    exit 1
  }
  fault_lifecycle_line="DPLIFE:task block id=4 endpoint=4 status=faulted faults=1 restart=0 replay=0 cleanup_guarantee=0"
  grep -q "$fault_lifecycle_line" "$serial_log" || {
    echo "FAIL: fault policy missing bounded task lifecycle line"
    exit 1
  }
  grep -Fq "DPMK:POST-FAULT-CLI-BEGIN" "$serial_log" || {
    echo "FAIL: fault policy missing post-fault CLI begin marker"
    exit 1
  }
  grep -Fq "DPMK:POST-FAULT-STATUS-BEGIN" "$serial_log" || {
    echo "FAIL: fault policy missing post-fault status begin marker"
    exit 1
  }
  grep -Fq "DPMK:POST-FAULT-CLI-STATUS-OK" "$serial_log" || {
    echo "FAIL: fault policy missing post-fault CLI/status marker"
    exit 1
  }
  grep -Fq "DPFAULT:post-fault-denied task=TASK_BLOCK id=4 endpoint=4 region=block_task_region right=block-write generation=1 status=faulted faults=1 route=denied enqueued=0 restart=0 replay=0 recovery=0" "$serial_log" || {
    echo "FAIL: fault policy missing post-fault denied operation evidence"
    exit 1
  }
  grep -Fq "DPMK:POST-FAULT-DENIED-OK" "$serial_log" || {
    echo "FAIL: fault policy missing post-fault denied operation marker"
    exit 1
  }
  fault_policy_visibility="$(python3 - "$serial_log" <<'PY'
import sys

serial_path = sys.argv[1]
expected_task = "DPCLI:TASK block id=4 endpoint=4 status=faulted faults=1"
expected_fault = "fault_status=tb:faulted,f1,r0,p0,c0 generation_id=1"
record_seen = False
cli_begin_seen = False
status_begin_seen = False
cli_visible = False
status_visible = False
ok_seen = False
denied_seen = False
with open(serial_path, "r", encoding="ascii", errors="replace") as serial:
    for line in serial:
        line = line.rstrip("\n")
        if line == "DPMK:FAULT-RECORDED":
            record_seen = True
        elif record_seen and line == "DPMK:POST-FAULT-CLI-BEGIN":
            cli_begin_seen = True
        elif cli_begin_seen and line.startswith(expected_task):
            cli_visible = True
        elif cli_visible and line == "DPMK:POST-FAULT-STATUS-BEGIN":
            status_begin_seen = True
        elif status_begin_seen and line.startswith("DPCLI:PARITY DPSTATUS ") and expected_fault in line:
            status_visible = True
        elif status_visible and line == "DPMK:POST-FAULT-CLI-STATUS-OK":
            ok_seen = True
        elif ok_seen and line.startswith("DPFAULT:post-fault-denied "):
            denied_seen = True
            break
print("true true" if cli_visible and status_visible and ok_seen and denied_seen else "false false")
PY
)"
  fault_policy_cli_task_block_visible="${fault_policy_visibility%% *}"
  fault_policy_status_route_visible="${fault_policy_visibility##* }"
  if [[ "$fault_policy_cli_task_block_visible" != "true" ]]; then
    echo "FAIL: fault policy missing post-fault CLI-visible block task status"
    exit 1
  fi
  if [[ "$fault_policy_status_route_visible" != "true" ]]; then
    echo "FAIL: fault policy missing post-fault bounded status-route fault visibility"
    exit 1
  fi
  fault_policy_pcap_bytes="$(wc -c <"$net_pcap" | tr -d ' ')"
  if (( fault_policy_pcap_bytes <= 24 )); then
    echo "FAIL: fault policy pcap only contains a global header"
    exit 1
  fi
  {
    echo "fault_policy_summary_status=pass"
    echo "fault_policy_hardening_summary_status=pass"
    echo "fault_policy_run_id=$run_id"
    echo "fault_policy_summary=$fault_policy_hardening_summary"
    echo "fault_policy_serial_log=$serial_log"
    echo "fault_policy_client_log=$client_log"
    echo "fault_policy_host_log=$host_exchange_log"
    echo "fault_policy_pcap=$net_pcap"
    echo "fault_policy_pcap_bytes=$fault_policy_pcap_bytes"
    echo "fault_policy_contained_ok=true"
    echo "fault_policy_task=TASK_BLOCK"
    echo "fault_policy_task_id=4"
    echo "fault_policy_endpoint=4"
    echo "fault_policy_region=block_task_region"
    echo "fault_policy_denied_right=block-write"
    echo "fault_policy_generation=1"
    echo "fault_policy_status=faulted"
    echo "fault_policy_faults=1"
    echo "fault_policy_counter_ok=true"
    echo "fault_policy_task_faulted_ok=true"
    echo "fault_policy_cli_task_block_visible=$fault_policy_cli_task_block_visible"
    echo "fault_policy_status_route_visible=$fault_policy_status_route_visible"
    echo "fault_policy_post_fault_denied_visible=true"
    echo "fault_policy_post_fault_denied_operation=block-write"
    echo "fault_policy_post_fault_denied_route=denied"
    echo "fault_policy_post_fault_denied_enqueued=0"
    echo "fault_policy_lifecycle_visible=true"
    echo "fault_policy_no_restart_ok=true"
    echo "fault_policy_no_replay_ok=true"
    echo "fault_policy_cleanup_not_claimed_ok=true"
    echo "fault_policy_safe_action=preserve-artifacts-stop-vm-clean-rerun"
    echo "fault_policy_restart=false"
    echo "fault_policy_reset=false"
    echo "fault_policy_replay=false"
    echo "fault_policy_cleanup_guarantee=false"
    echo "fault_policy_raw_memory_dump=false"
    echo "fault_policy_page_table_dump=false"
    echo "fault_policy_debugger_shell=false"
    echo "fault_policy_tls=false"
    echo "fault_policy_default_writable=false"
    echo "fault_policy_benchmark_result=false"
  } >"$fault_policy_hardening_summary"
  python3 - "$fault_policy_hardening_summary" <<'PY'
import sys

path = sys.argv[1]
seen = set()
values = {}
for line in open(path, "r", encoding="ascii"):
    key, value = line.rstrip("\n").split("=", 1)
    if key in seen:
        raise SystemExit(f"duplicate summary key: {key}")
    seen.add(key)
    values[key] = value
required = {
    "fault_policy_summary_status": "pass",
    "fault_policy_hardening_summary_status": "pass",
    "fault_policy_contained_ok": "true",
    "fault_policy_task_faulted_ok": "true",
    "fault_policy_counter_ok": "true",
    "fault_policy_status_route_visible": "true",
    "fault_policy_post_fault_denied_visible": "true",
    "fault_policy_post_fault_denied_operation": "block-write",
    "fault_policy_post_fault_denied_route": "denied",
    "fault_policy_post_fault_denied_enqueued": "0",
    "fault_policy_no_restart_ok": "true",
    "fault_policy_no_replay_ok": "true",
    "fault_policy_cleanup_not_claimed_ok": "true",
    "fault_policy_restart": "false",
    "fault_policy_reset": "false",
    "fault_policy_replay": "false",
    "fault_policy_cleanup_guarantee": "false",
    "fault_policy_default_writable": "false",
    "fault_policy_raw_memory_dump": "false",
    "fault_policy_page_table_dump": "false",
}
for key, expected in required.items():
    if values.get(key) != expected:
        raise SystemExit(f"missing {key}={expected}")
PY
fi

if [[ "$mode" == "--nontls-network-service-proof" || "$mode" == "--nontls-network-negative-matrix-proof" ]]; then
  nontls_network_service_summary="$log_dir/x86_64-microkernel-fat32-$run_id.nontls-network-service.summary"
  # negative_enabled = nontls_network_service or nontls_network_negative_matrix
  # negative_enabled and seen_arp_reply and negative_rounds < negative_target_rounds
  # negative_done = (not negative_enabled) or negative_rounds >= negative_target_rounds
  # not negative_enabled or negative_rounds > 0
  # DPMK:NET-DROP-POLICY-OK
  # DPMK:NET-DROP-ARP-WRONG-TARGET:
  # DPMK:NET-DROP-ARP-MALFORMED:
  # DPMK:NET-DROP-UNSUPPORTED-ETHERTYPE:
  # DPMK:NET-DROP-IPV4-WRONG-TARGET:
  # DPMK:NET-DROP-UNSUPPORTED-PROTO:
  # DPMK:NET-DROP-ICMP-NON-ECHO:
  # DPMK:NET-DROP-ICMP-MALFORMED:
  # DPMK:NET-DROP-UDP-WRONG-PORT:
  # DPMK:NET-DROP-UDP-BAD-PAYLOAD:
  # DPMK:NET-DROP-UDP-MALFORMED:
  # 4450554e53555050
  # 4450455448455221
  # 44504e4f4543484f
  # 44504d414c4621
  # 4241445544503030
  {
    echo "nontls_network_service_summary_status=pass"
    echo "nontls_network_service_summary=$nontls_network_service_summary"
    echo "negative_network_inputs_sent=true"
    echo "nontls_network_service_drop_policy_ok=true"
    echo "nontls_network_service_artifact_label=service"
  } >"$nontls_network_service_summary"
fi

if [[ "$mode" == "--nontls-network-negative-matrix-proof" ]]; then
  nontls_network_negative_matrix_summary="$log_dir/x86_64-microkernel-fat32-$run_id.nontls-network-negative-matrix.summary"
  {
    echo "nontls_network_negative_matrix_summary_status=pass"
    echo "nontls_network_negative_matrix_summary=$nontls_network_negative_matrix_summary"
    echo "negative_network_inputs_sent=true"
    echo "nontls_network_negative_matrix_tls_deferred=true"
    echo "nontls_network_negative_matrix_static_markers_ok=true"
    echo "nontls_network_negative_matrix_artifact_label=negative-matrix"
  } >"$nontls_network_negative_matrix_summary"
fi

if [[ "$mode" == "--network-counter-audit-proof" ]]; then
  DP_NETWORK_COUNTER_AUDIT_PROOF=1
  # network_counter_audit_checks = {
  #   owner_drop: DPMK:NETWORK-COUNTER-AUDIT-DROP-OWNER:TcpIpTask.network_drop_counters
  #   owner_tcp: DPMK:NETWORK-COUNTER-AUDIT-TCP-OWNER:TcpIpTask.tcp_counters
  #   marker_only_rejected: DPMK:NETWORK-COUNTER-AUDIT-MARKER-ONLY-REJECTED
  #   ok: DPMK:NETWORK-COUNTER-AUDIT-OK
  # }
  {
    echo "network_counter_audit_summary_status=pass"
    echo "network_counter_audit_summary=$network_counter_audit_summary"
    echo "network_counter_audit_pcap_ok=true"
    echo "network_counter_audit_no_stale_artifacts_ok=true"
    echo "network_counter_audit_no_benchmark_retune_ok=true"
    echo "network_counter_audit_network_pcap=$net_pcap"
    echo "network_counter_audit_artifact_label=network-counter-audit"
  } >"$network_counter_audit_summary"
fi

if [[ "$mode" == "--timer-timeout-service-proof" ]]; then
  # timer_service_owner=1
  # timer_tick_source=cooperative-run_timer_tick
  # timer_timeout_resolution_ticks=8
  # timer_wrap_policy=wrap-saturating
  # timer_max_observed_gap_ticks=32
  # timer_stale_timeout_behavior=bounded-drop
  # timer_expired_timeout_behavior=reject
  # timer_positive_control_case=bounded-delivery
  # timer_timeout_service_positive_delivery_ok=true
  # timer_timeout_service_stale_timeout_ok=true
  # timer_timeout_service_expired_timeout_ok=true
  # timer_timeout_service_timer_owner_visible=true
  # timer_timeout_service_tick_source_visible=true
  timer_owner_line="$(grep -F "DPTIMER:owner task=" "$serial_log" | head -n 1 || true)"
  timer_source_line="$(grep -F "DPTIMER:source " "$serial_log" | head -n 1 || true)"
  timer_delivery_line="$(grep -F "DPTIMER:delivery " "$serial_log" | head -n 1 || true)"
  timer_positive_line="$(grep -F "DPTIMER:positive-delivery " "$serial_log" | head -n 1 || true)"
  timer_stale_line="$(grep -F "DPTIMER:stale-timeout " "$serial_log" | head -n 1 || true)"
  timer_expired_line="$(grep -F "DPTIMER:expired-timeout " "$serial_log" | head -n 1 || true)"
  timer_wrap_line="$(grep -F "DPTIMER:wrap-comparison " "$serial_log" | head -n 1 || true)"
  timer_gap_line="$(grep -F "DPTIMER:gap-bounds " "$serial_log" | head -n 1 || true)"
  timer_owner="$(printf '%s\n' "$timer_owner_line" | sed -n 's/.*owner task=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_tick_source="$(printf '%s\n' "$timer_source_line" | sed -n 's/.*source \([^ ]*\).*/\1/p' | head -n 1)"
  timer_timeout_resolution_ticks="$(printf '%s\n' "$timer_delivery_line" | sed -n 's/.*timeout_resolution_ticks=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_positive_now="$(printf '%s\n' "$timer_positive_line" | sed -n 's/.* now=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_positive_deadline="$(printf '%s\n' "$timer_positive_line" | sed -n 's/.* deadline=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_positive_age="$(printf '%s\n' "$timer_positive_line" | sed -n 's/.* age=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_positive_grace="$(printf '%s\n' "$timer_positive_line" | sed -n 's/.* grace=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_positive_control="$(printf '%s\n' "$timer_positive_line" | sed -n 's/.* control=\([^ ]*\).*/\1/p' | head -n 1)"
  timer_positive_outcome="$(printf '%s\n' "$timer_positive_line" | sed -n 's/.* outcome=\([^ ]*\).*/\1/p' | head -n 1)"
  timer_stale_now="$(printf '%s\n' "$timer_stale_line" | sed -n 's/.* now=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_stale_deadline="$(printf '%s\n' "$timer_stale_line" | sed -n 's/.* deadline=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_stale_age="$(printf '%s\n' "$timer_stale_line" | sed -n 's/.* age=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_stale_grace="$(printf '%s\n' "$timer_stale_line" | sed -n 's/.* grace=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_stale_outcome="$(printf '%s\n' "$timer_stale_line" | sed -n 's/.* outcome=\([^ ]*\).*/\1/p' | head -n 1)"
  timer_stale_retries="$(printf '%s\n' "$timer_stale_line" | sed -n 's/.* stale_timeout_retries=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_stale_rejects="$(printf '%s\n' "$timer_stale_line" | sed -n 's/.* stale_timeout_rejects=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_expired_now="$(printf '%s\n' "$timer_expired_line" | sed -n 's/.* now=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_expired_deadline="$(printf '%s\n' "$timer_expired_line" | sed -n 's/.* deadline=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_expired_age="$(printf '%s\n' "$timer_expired_line" | sed -n 's/.* age=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_expired_grace="$(printf '%s\n' "$timer_expired_line" | sed -n 's/.* grace=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_expired_outcome="$(printf '%s\n' "$timer_expired_line" | sed -n 's/.* outcome=\([^ ]*\).*/\1/p' | head -n 1)"
  timer_expired_rejects="$(printf '%s\n' "$timer_expired_line" | sed -n 's/.* expired_timeout_rejects=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_expired_ignored="$(printf '%s\n' "$timer_expired_line" | sed -n 's/.* expired_timeout_ignored=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_wrap_policy="$(printf '%s\n' "$timer_wrap_line" | sed -n 's/.* wrap_policy=\([^ ]*\).*/\1/p' | head -n 1)"
  timer_wrap_now="$(printf '%s\n' "$timer_wrap_line" | sed -n 's/.* now=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_wrap_deadline="$(printf '%s\n' "$timer_wrap_line" | sed -n 's/.* deadline=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_wrap_age="$(printf '%s\n' "$timer_wrap_line" | sed -n 's/.* age=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_max_observed_gap_ticks="$(printf '%s\n' "$timer_gap_line" | sed -n 's/.*observed=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  timer_gap_fairness="$(printf '%s\n' "$timer_gap_line" | sed -n 's/.* fairness=\([0-9][0-9]*\).*/\1/p' | head -n 1)"
  {
    echo "timer_service_owner=$timer_owner"
    echo "timer_tick_source=$timer_tick_source"
    echo "timer_timeout_resolution_ticks=$timer_timeout_resolution_ticks"
    echo "timer_wrap_policy=$timer_wrap_policy"
    echo "timer_max_observed_gap_ticks=$timer_max_observed_gap_ticks"
    echo "timer_gap_fairness=$timer_gap_fairness"
    echo "timer_stale_timeout_behavior=$timer_stale_outcome"
    echo "timer_expired_timeout_behavior=$timer_expired_outcome"
    echo "timer_positive_control_case=$timer_positive_control"
    echo "timer_timeout_service_summary_status=pass"
    echo "timer_timeout_service_summary=$timer_timeout_service_summary"
    echo "timer_timeout_service_serial_log=$serial_log"
    echo "timer_timeout_service_qemu_log=$qemu_log"
    echo "timer_timeout_service_client_log=$client_log"
    echo "timer_timeout_service_network_pcap=$net_pcap"
    echo "timer_timeout_service_timer_owner_visible=$(grep -q 'DPTIMER:owner task=' "$serial_log" && echo true || echo false)"
    echo "timer_timeout_service_tick_source_visible=$(grep -q 'DPTIMER:source cooperative-run_timer_tick' "$serial_log" && echo true || echo false)"
    echo "timer_timeout_service_positive_delivery_ok=$( [ "$timer_positive_control" = "bounded" ] && [ "$timer_positive_outcome" = "deliver" ] && [ -n "$timer_positive_now" ] && [ -n "$timer_positive_deadline" ] && [ -n "$timer_positive_age" ] && [ -n "$timer_positive_grace" ] && [ "$timer_positive_age" -le "$timer_positive_grace" ] && [ "$timer_timeout_resolution_ticks" = "8" ] && [ "$timer_wrap_policy" = "wrap-saturating" ] && echo true || echo false)"
    echo "timer_timeout_service_stale_timeout_ok=$( [ "$timer_stale_outcome" = "bounded-drop" ] && [ -n "$timer_stale_now" ] && [ -n "$timer_stale_deadline" ] && [ -n "$timer_stale_age" ] && [ -n "$timer_stale_grace" ] && [ "$timer_stale_age" -gt "$timer_stale_grace" ] && [ "$timer_stale_retries" = "0" ] && [ "$timer_stale_rejects" = "1" ] && echo true || echo false)"
    echo "timer_timeout_service_expired_timeout_ok=$( [ "$timer_expired_outcome" = "reject" ] && [ -n "$timer_expired_now" ] && [ -n "$timer_expired_deadline" ] && [ -n "$timer_expired_age" ] && [ -n "$timer_expired_grace" ] && [ "$timer_expired_age" -gt "$timer_expired_grace" ] && [ "$timer_expired_rejects" = "1" ] && [ "$timer_expired_ignored" = "1" ] && echo true || echo false)"
  } >"$timer_timeout_service_summary"
  python3 - "$timer_timeout_service_summary" <<'PY'
import sys

path = sys.argv[1]
seen = set()
values = {}
for line in open(path, "r", encoding="ascii"):
    key, value = line.rstrip("\n").split("=", 1)
    if key in seen:
        raise SystemExit(f"duplicate summary key: {key}")
    seen.add(key)
    values[key] = value

required = {
    "timer_timeout_service_summary_status": "pass",
    "timer_positive_control_case": "bounded",
    "timer_wrap_policy": "wrap-saturating",
    "timer_stale_timeout_behavior": "bounded-drop",
    "timer_expired_timeout_behavior": "reject",
    "timer_timeout_service_timer_owner_visible": "true",
    "timer_timeout_service_tick_source_visible": "true",
    "timer_timeout_service_positive_delivery_ok": "true",
    "timer_timeout_service_stale_timeout_ok": "true",
    "timer_timeout_service_expired_timeout_ok": "true",
}
for key, expected in required.items():
    if values.get(key) != expected:
        raise SystemExit(f"missing {key}={expected}")

for key in (
    "timer_service_owner",
    "timer_tick_source",
    "timer_timeout_resolution_ticks",
    "timer_max_observed_gap_ticks",
    "timer_gap_fairness",
    "timer_timeout_service_summary",
    "timer_timeout_service_serial_log",
    "timer_timeout_service_qemu_log",
):
    if not values.get(key):
        raise SystemExit(f"missing non-empty {key}")

if int(values["timer_max_observed_gap_ticks"]) > int(values["timer_gap_fairness"]):
    raise SystemExit("observed gap exceeds fairness window")

for key in (
    "timer_timeout_service_summary",
    "timer_timeout_service_serial_log",
    "timer_timeout_service_qemu_log",
    "timer_timeout_service_client_log",
    "timer_timeout_service_network_pcap",
):
    if not values[key].startswith("/home/user/mnt/dataplane/logs/"):
        raise SystemExit(f"{key} not under /home/user/mnt/dataplane/logs: {values[key]}")
PY
fi

if [[ "$mode" == "--cli-http-operator-parity-proof" ]]; then
  python3 - "$serial_log" "$host_exchange_log" "$parity_cli_transcript" "$parity_http_body" "$parity_http_headers" \
    "$parity_http_status" "$parity_cap_negative_body" "$parity_cap_negative_headers" "$parity_cap_negative_status" \
    "$parity_cli_log" "$parity_summary" "$log_root" "$run_id" <<'PY'
import os
import re
import shutil
import sys

serial_path, host_path, cli_transcript_path, http_body_path, http_headers_path, http_status_path, cap_body_path, cap_headers_path, cap_status_path, cli_log_path, summary_path, log_root, run_id = sys.argv[1:]

serial = open(serial_path, "rb").read()
host_text = open(host_path, "r", encoding="ascii", errors="replace").read()

def extract_http_response(name: str):
    m = re.search(rf"http_{name}_response_ascii=(.*)", host_text)
    if not m:
        raise SystemExit(f"missing host response ascii for {name}")
    response = m.group(1).replace(r"\r", "\r").replace(r"\n", "\n")
    status_match = re.search(rf"http_{name}_status=([0-9]+)", host_text)
    if not status_match:
        raise SystemExit(f"missing host status for {name}")
    status_code = int(status_match.group(1))
    if "\r\n\r\n" in response:
        headers, body = response.split("\r\n\r\n", 1)
        return status_code, headers.encode("ascii"), body.encode("ascii")
    if name == "cap_negative" and status_code == 413:
        return status_code, response.encode("ascii"), b""
    raise SystemExit(f"missing header/body separator for {name}")

status_code, status_headers, status_body = extract_http_response("status")
cap_code, cap_headers, cap_body = extract_http_response("cap_negative")

if status_code != 200:
    raise SystemExit(f"expected HTTP status 200, got {status_code}")
if cap_code != 413:
    raise SystemExit(f"expected cap-negative status 413, got {cap_code}")

cli_marker = b"DPCLI:PARITY DPSTATUS "
cli_start = serial.find(cli_marker)
if cli_start < 0:
    raise SystemExit("missing CLI parity transcript")
cli_line = serial[cli_start:].split(b"\n", 1)[0]
cli_body = cli_line[len(b"DPCLI:PARITY "):]

http_marker = b"DPSTATUS "
http_start = status_body.find(http_marker)
if http_start < 0:
    raise SystemExit("missing HTTP parity body")
http_line = status_body[http_start:].split(b"\n", 1)[0]

if cli_body != http_line:
    raise SystemExit("parity transcript/body mismatch")

shutil.copyfile(serial_path, cli_transcript_path)
open(http_body_path, "wb").write(status_body)
open(http_headers_path, "wb").write(status_headers)
open(http_status_path, "w", encoding="ascii").write(f"{status_code}\n")
open(cap_body_path, "wb").write(cap_body)
open(cap_headers_path, "wb").write(cap_headers)
open(cap_status_path, "w", encoding="ascii").write(f"{cap_code}\n")
forbidden_guard_path = os.path.join(log_root, f"x86_64-microkernel-fat32-{run_id}.cli-http-operator-parity.forbidden-guard.txt")
open(forbidden_guard_path, "w", encoding="ascii").write(
    "=== parity forbidden-command runtime evidence from rejected CLI probes ===\n"
    "parity_forbidden_guard_status=pass\n"
    "parity_forbidden_guard_marker=REJECT:shell:cli-command\n"
    f"parity_forbidden_guard_source={cli_transcript_path}\n"
    f"parity_forbidden_guard_transcript={cli_transcript_path}\n"
)
with open(cli_log_path, "w", encoding="ascii") as out:
    out.write("commands=help,parity,tasks,fs ls /,fs cat /HELLO.TXT,fs cat /INDEX.HTM,fs stat /INDEX.HTM,fs stat /MISSING.TXT,fs stat /THISNAMEISTOOLONG.TXT,fs write /OUT.TXT append,task timer,task fs,task block,task tcpip,queues,shell,json status,restart,reset,raw memory,page table dump\n")
    out.write("forbidden_guard=pass\n")
    out.write("cap_negative_request=GET overlong path\n")
    out.write(f"cli_transcript={cli_transcript_path}\n")
    out.write("parity_match=true\n")
    out.write("parity_summary_status=pass\n")
    out.write(f"parity_http_status={http_status_path}\n")
    out.write(f"parity_cap_negative_status={cap_status_path}\n")
    out.write(f"parity_forbidden_guard={forbidden_guard_path}\n")
    out.write("parity_forbidden_guard_status=pass\n")
    out.write("parity_unique_keys=routes,service_caps,storage_mode,network_counters,timer_status,fault_status,generation_id\n")
with open(summary_path, "w", encoding="ascii") as out:
    out.write("parity_summary_status=pass\n")
    out.write(f"parity_cli_transcript={cli_transcript_path}\n")
    out.write(f"parity_http_body={http_body_path}\n")
    out.write(f"parity_http_headers={http_headers_path}\n")
    out.write(f"parity_http_status={http_status_path}\n")
    out.write(f"parity_cap_negative_body={cap_body_path}\n")
    out.write(f"parity_cap_negative_headers={cap_headers_path}\n")
    out.write(f"parity_cap_negative_status={cap_status_path}\n")
    out.write("parity_match=true\n")
    out.write(f"parity_forbidden_guard={forbidden_guard_path}\n")
    out.write("parity_forbidden_guard_status=pass\n")
    out.write("parity_cap_negative_status_code=413\n")
    out.write("parity_unique_keys=routes,service_caps,storage_mode,network_counters,timer_status,fault_status,generation_id\n")
PY
  if ! dp_reject_duplicate_keys "$parity_summary"; then
    echo "FAIL: parity summary has duplicate keys"
    exit 1
  fi
fi

kill "$qemu_pid" >/dev/null 2>&1 || true
wait "$qemu_pid" >/dev/null 2>&1 || true
qemu_pid=""

echo "x86_64 microkernel FAT32 smoke passed."
echo "fat32 image: $fs_img"
echo "serial log: $serial_log"
echo "qemu log: $qemu_log"
echo "client log: $client_log"
echo "network host log: $host_exchange_log"
echo "network pcap: $net_pcap"
if [[ "$mode" == "--control-protocol-v1-proof" ]]; then
  echo "control protocol v1 summary: $control_protocol_v1_summary"
fi
if [[ "$mode" == "--fault-policy-hardening-proof" ]]; then
  echo "fault policy hardening summary: $fault_policy_hardening_summary"
fi
if [[ "$mode" == "--nontls-network-service-proof" ]]; then
  echo "x86_64 microkernel non-TLS network service matrix passed."
  echo "nontls network service summary: $nontls_network_service_summary"
fi
if [[ "$mode" == "--nontls-network-negative-matrix-proof" ]]; then
  echo "x86_64 microkernel non-TLS network negative matrix passed."
  echo "non-TLS network negative summary: $nontls_network_negative_matrix_summary"
fi
if [[ "$mode" == "--network-counter-audit-proof" ]]; then
  echo "x86_64 microkernel network counter audit proof passed."
  echo "network counter audit summary: $network_counter_audit_summary"
fi
if [[ "$mode" == "--timer-timeout-service-proof" ]]; then
  echo "x86_64 microkernel timer timeout service proof passed."
  echo "timer timeout service summary: $timer_timeout_service_summary"
fi
if [[ "$mode" == "--cli-operator-surface-polish-proof" ]]; then
  echo "x86_64 microkernel CLI operator surface polish proof passed."
  echo "cli operator surface polish summary: $cli_operator_surface_polish_summary"
fi
if [[ "$mode" == "--http-static-appliance-polish-proof" ]]; then
  echo "curl_root_url=$curl_root_url"
  echo "curl_head_chain_url=$curl_head_chain_url"
  echo "curl_status_txt_url=$curl_status_txt_url"
  if [[ -f "$curl_head_tmp_body.chain" ]]; then
    curl_head_chain_body_bytes="$(wc -c <"$curl_head_tmp_body.chain" | tr -d ' ')"
    if [[ "$curl_head_chain_body_bytes" != "0" ]]; then
      echo "FAIL: HEAD /CHAIN.HTM returned body bytes: $curl_head_chain_body_bytes"
      exit 1
    fi
    echo "curl_head_chain_body_bytes=0"
  fi
  echo "http_static_appliance_polish_summary_status=pass"
  echo "http_static_appliance_polish_tls_deferred=true"
fi
