#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

runner="tools/x86_64_microkernel_fat32_served_byte_reproducibility.sh"
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

for file in "$runner" "$makefile" "$plan" "$diary"; do
  require_file "$file"
done

for literal in \
  'mnt_root="/home/user/mnt/dataplane"' \
  './tools/x86_64_microkernel_fat32_reproducibility.sh >"$repro_log" 2>&1' \
  './tools/x86_64_microkernel_fat32_run.sh --curl-proof >"$curl_log" 2>&1' \
  './tools/x86_64_microkernel_fat32_run.sh >"$normal_log" 2>&1' \
  'extract_label_path_allow_empty' \
  'fat32_artifact_reproducibility_index_htm_sha256' \
  'fat32_artifact_reproducibility_hello_txt_sha256' \
  'curl GET body does not match expected INDEX.HTM bytes' \
  'serial CLI output did not contain exact HELLO.TXT bytes' \
  'serial CLI output did not contain exact INDEX.HTM bytes' \
  'curl pcap did not contain exact INDEX.HTM response body bytes' \
  'cli_hello_body.write_bytes(expected_hello)' \
  'cli_index_body.write_bytes(expected_index)' \
  'curl HEAD unexpectedly served a body' \
  'stale served-byte digest summary was accepted' \
  'fat32_served_byte_reproducibility_summary_status=pass' \
  'fat32_served_byte_reproducibility_cli_hello_matches_image=true' \
  'fat32_served_byte_reproducibility_cli_index_matches_image=true' \
  'fat32_served_byte_reproducibility_http_get_index_matches_image=true' \
  'fat32_served_byte_reproducibility_http_head_body_empty=true' \
  'fat32_served_byte_reproducibility_http_head_length_matches_image=true' \
  'fat32_served_byte_reproducibility_pcap_contains_index_body=true' \
  'fat32_served_byte_reproducibility_stale_digest_rejected=true' \
  'fat32_served_byte_reproducibility_default_writable=false' \
  'fat32_served_byte_reproducibility_repair_mode=false' \
  'fat32_served_byte_reproducibility_fsck=false' \
  'fat32_served_byte_reproducibility_tls=false' \
  'fat32_served_byte_reproducibility_https=false' \
  'fat32_served_byte_reproducibility_benchmark_result=false'; do
  require_literal "$runner" "$literal"
done

require_literal "$makefile" '.PHONY: x86_64-microkernel-fat32-served-byte-reproducibility-contract x86_64-microkernel-fat32-served-byte-reproducibility'
require_literal "$makefile" 'x86_64-microkernel-fat32-served-byte-reproducibility-contract:'
require_literal "$makefile" './tools/check_x86_64_microkernel_fat32_served_byte_reproducibility_contract.sh'
require_literal "$makefile" 'x86_64-microkernel-fat32-served-byte-reproducibility: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-artifact-reproducibility-contract x86_64-microkernel-fat32-served-byte-reproducibility-contract'
require_literal "$makefile" './tools/x86_64_microkernel_fat32_served_byte_reproducibility.sh'
require_literal "$plan" '`x86_64-microkernel-fat32-served-byte-reproducibility`'
require_literal "$diary" '`x86_64-microkernel-fat32-served-byte-reproducibility`'

if rg -n -- 'repair_mode=true|default_writable=true|fsck=true|https://|curl[[:space:]]+-k|OpenAPI|serde_json|benchmark_result=true' "$runner"; then
  fail "served-byte packet must not cross stop lines"
fi

echo "x86_64 microkernel FAT32 served-byte reproducibility contract OK"
