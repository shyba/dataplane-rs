#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
fat32="crates/dataplane-x86_64-microkernel-smoke/src/fat32.rs"
runner="tools/x86_64_microkernel_fat32_service_errors.sh"
makefile="Makefile"
spec="changes/__archived_changes_2026-06-01/x86_64-microkernel-fat32-service-errors/specs/fat32-service-errors/spec.md"
tasks="changes/__archived_changes_2026-06-01/x86_64-microkernel-fat32-service-errors/tasks.md"

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

reject_regex() {
  local file="$1"
  local regex="$2"
  if rg -n -- "$regex" "$file"; then
    fail "forbidden stop-line or overclaim in $file: $regex"
  fi
}

for file in "$main" "$fat32" "$runner" "$makefile" "$spec" "$tasks"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel FAT32 Service Errors Contract Guard ==="

for literal in \
  'enum FsServiceRequest' \
  'Open {' \
  'Stat {' \
  'ReadAt {' \
  'ListRoot,' \
  'enum FsError' \
  'Handle,' \
  'Range,' \
  'Capacity,' \
  'FsError::Handle' \
  'FsError::Range' \
  'FsError::Capacity'; do
  require_literal "$fat32" "$literal"
done

for literal in \
  'FsServiceRequest::Open {' \
  'FsServiceRequest::Stat { handle:' \
  'FsServiceRequest::ReadAt {' \
  'FsError::Handle' \
  'FsError::Range' \
  'FsError::Capacity' \
  'fn run_fs_service_error_matrix(&mut self) -> Result<(), &' \
  'self.expect_fs_error(' \
  'DPMK:FS-SERVICE-ERROR-MATRIX-OK' \
  'DPFSERR:matrix small=ok multi_cluster=ok missing=' \
  'DPFSERR:counters open=2 stat=1 read_at=5 list_root=1 errors=6' \
  'self.fs_service_request(' \
  '.read()?'; do
  require_literal "$main" "$literal"
done

if awk '
  /fn load_http_files/ { in_body=1 }
  in_body && /fn load_http_file_chain/ { in_body=0 }
  in_body && /load_http_file_chain/ { found=1 }
  END { exit found ? 0 : 1 }
' "$main"; then
  fail "load_http_files must not bypass FsTask with load_http_file_chain"
fi

if awk '
  /fn load_http_files/ { in_body=1 }
  in_body && /fn load_http_file_chain/ { in_body=0 }
  in_body && /fs_service_request/ { found=1 }
  END { exit found ? 0 : 1 }
' "$main"; then
  :
else
  fail "load_http_files must route through fs_service_request"
fi

for literal in \
  'DP_MICROKERNEL_CURL_PROOF=1' \
  './tools/x86_64_microkernel_fat32_run.sh --curl-proof >"$curl_run_log" 2>&1' \
  'DP_MICROKERNEL_CLI_HTTP_OPERATOR_PARITY_PROOF=1' \
  './tools/x86_64_microkernel_fat32_run.sh --cli-http-operator-parity-proof >"$cli_run_log" 2>&1' \
  'fat32_service_errors_host_client_status=pass' \
  'fat32_service_errors_http_missing_status=404' \
  'fat32_service_errors_http_unsupported_write_status=405' \
  'fat32_service_errors_http_output_cap_status=413' \
  'fat32_service_errors_cli_unsupported_write=unsupported-write-shape' \
  'fat32_service_errors_matrix_markers_match=true' \
  'fat32_service_errors_summary_status=pass' \
  'fat32_service_errors_small_file=ok' \
  'fat32_service_errors_multi_cluster_file=ok' \
  'fat32_service_errors_bad_handle=fs-handle' \
  'fat32_service_errors_unopened_read=fs-handle' \
  'fat32_service_errors_bad_offset=fs-range' \
  'fat32_service_errors_output_cap=fs-capacity' \
  'fat32_service_errors_http_cli_same_fs_task=true' \
  'fat32_service_errors_default_writable=false' \
  'fat32_service_errors_cache_policy=false'; do
  require_literal "$runner" "$literal"
done

require_literal "$makefile" '.PHONY: x86_64-microkernel-fat32-service-errors-contract x86_64-microkernel-fat32-service-errors'
require_literal "$makefile" 'x86_64-microkernel-fat32-service-errors-contract:'
require_literal "$makefile" './tools/check_x86_64_microkernel_fat32_service_errors_contract.sh'
require_literal "$makefile" 'x86_64-microkernel-fat32-service-errors: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-service-errors-contract'
require_literal "$makefile" './tools/x86_64_microkernel_fat32_service_errors.sh'

for literal in \
  'small-file, multi-cluster-file, missing-file, bad-handle, unopened-read, bad-offset, output-cap, unsupported-path-shape, and unsupported-write' \
  'HTTP and CLI share the same service seam' \
  'Fail closed on helper bypass and stale evidence'; do
  require_literal "$spec" "$literal"
done

reject_regex "$main" 'default[ _-]?writable|fsck|journal|crash[ -]?consistency|https|certificate|generic[ _-]?socket|generic[ _-]?IPC|heap[ -]?backed'
reject_regex "$runner" 'default_writable=true|fsck=true|journaling=true|crash_consistency=true|https://|curl[[:space:]]+-k|OpenSSL|rustls|benchmark_retuning=true|cache_policy=true'

echo "x86_64 microkernel FAT32 service-errors contract OK"
