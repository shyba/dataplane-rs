#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

guest="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
fat32="crates/dataplane-x86_64-microkernel-smoke/src/fat32.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
makefile="Makefile"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_literal() {
  local file="$1"
  local needle="$2"
  local note="$3"
  rg -Fq -- "$needle" "$file" || fail "$note"
}

reject_literal() {
  local file="$1"
  local needle="$2"
  local note="$3"
  local status matches
  set +e
  matches="$(rg -n -F -- "$needle" "$file")"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    printf '%s\n' "$matches" >&2
    fail "$note"
  fi
  [[ "$status" -eq 1 ]] || fail "could not scan $file for $needle"
}

for file in "$guest" "$fat32" "$runner" "$makefile"; do
  [[ -f "$file" ]] || fail "missing required file: $file"
done

echo "=== x86_64 Microkernel Filesystem Service Hardening Contract Guard ==="

for needle in \
  "enum FsOperation" \
  "ListRoot" \
  "Stat" \
  "Open" \
  "ReadAt" \
  "enum FsError" \
  "UnsupportedPath" \
  "LongFilename" \
  "UnsupportedWrite" \
  "BadHandle" \
  "Offset" \
  "fn handle_service_request("; do
  require_literal "$fat32" "$needle" "fat32 module must expose bounded filesystem service invariant: $needle"
done

for needle in \
  "const FS_ROOT_ENTRY_SLOTS: usize = 4;" \
  "const FS_OPEN_FILE_SLOTS: usize = 4;" \
  "fn run_filesystem_read_matrix_probe(&mut self) -> Result<(), &'static str>" \
  "fn run_fs_policy_and_directory_slice_probe(&mut self) -> Result<(), &'static str>" \
  "DPMK:FS-SERVICE-HTTP-OK" \
  "DPMK:FS-READ-MATRIX-OUTPUT-CAP-OK" \
  "DPMK:FS-SERVICE-HARDENING-OK"; do
  require_literal "$guest" "$needle" "guest must expose bounded filesystem service invariant: $needle"
done

for needle in \
  'filesystem_service_hardening_summary="$log_dir/x86_64-microkernel-fat32-$run_id.filesystem-service-hardening.summary"' \
  'DP_MICROKERNEL_FILESYSTEM_SERVICE_HARDENING_PROOF' \
  '--filesystem-service-hardening-proof' \
  'filesystem_service_hardening_summary_status=pass' \
  'filesystem_service_hardening_mode=readonly-bounded-fs-service' \
  'filesystem_service_hardening_default_readonly_ok=true' \
  'filesystem_service_hardening_cli_http_shared_service_ok=true' \
  'filesystem_service_hardening_missing_file_ok=true' \
  'filesystem_service_hardening_bad_path_ok=true' \
  'filesystem_service_hardening_oversized_read_rejected_ok=true' \
  'filesystem_service_hardening_output_cap_ok=true' \
  'filesystem_service_hardening_default_write_rejected_ok=true' \
  'filesystem_service_hardening_journal_feature_separate_ok=true' \
  'filesystem_service_hardening_no_broad_fs_api_ok=true' \
  'filesystem service hardening summary: $filesystem_service_hardening_summary'; do
  require_literal "$runner" "$needle" "runner must expose focused filesystem hardening evidence: $needle"
done

for needle in \
  'filesystem_read_matrix_small_cli_ok=true' \
  'filesystem_read_matrix_chain_cli_ok=true' \
  'filesystem_read_matrix_bad_handle_ok=true' \
  'filesystem_read_matrix_bad_offset_ok=true' \
  'filesystem_read_matrix_output_cap_ok=true' \
  'fs_policy_and_directory_slice_list_root_ok=true' \
  'fs_policy_and_directory_slice_read_at_slot_ok=true' \
  'storage_readonly_status_ok=true'; do
  require_literal "$runner" "$needle" "focused filesystem hardening mode must check evidence: $needle"
done

require_literal "$makefile" "x86_64-microkernel-filesystem-service-hardening-contract:" \
  "Makefile must expose filesystem service hardening contract target"
require_literal "$makefile" "x86_64-microkernel-filesystem-service-hardening:" \
  "Makefile must expose filesystem service hardening proof target"
require_literal "$makefile" "DP_MICROKERNEL_FILESYSTEM_SERVICE_HARDENING_PROOF=1" \
  "Makefile must run the focused filesystem hardening proof"

for forbidden in \
  "GenericFileSystem" \
  "std::fs" \
  "default writable" \
  "host filesystem" \
  "--tls-proof" \
  "--https" \
  "curl -k" \
  "OpenSSL"; do
  reject_literal "$guest" "$forbidden" "guest must not add forbidden filesystem surface: $forbidden"
done

for forbidden in \
  "--https" \
  "curl -k" \
  "OpenSSL" \
  "GenericFileSystem" \
  "std::fs"; do
  reject_literal "$runner" "$forbidden" "runner must not add forbidden filesystem proof surface: $forbidden"
done

echo "x86_64 microkernel filesystem service hardening contract guard passed."
