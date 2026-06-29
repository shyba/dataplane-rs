#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

kernel="crates/dataplane-x86_64-microkernel-smoke/src/kernel.rs"
scenarios="crates/dataplane-x86_64-microkernel-smoke/src/scenarios.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
makefile="Makefile"
matrix="tools/x86_64_microkernel_validation_matrix_run.sh"
matrix_guard="tools/check_x86_64_microkernel_validation_matrix_contract.sh"
plan="aidocs/051_microkernel_pre_tls_appliance_plan_2026-05-30.md"
frontier="aidocs/050_microkernel_robust_design_frontier_2026-05-30.md"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_file() {
  local file="$1"
  [[ -f "$file" ]] || fail "required file missing: $file"
}

require_literal() {
  local file="$1"
  local needle="$2"
  local note="$3"
  rg -q --fixed-strings -- "$needle" "$file" || fail "$note"
}

reject_regex() {
  local file="$1"
  local pattern="$2"
  local note="$3"
  local matches status
  set +e
  matches="$(rg -n "$pattern" "$file")"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    printf '%s\n' "$matches"
    fail "$note"
  fi
  if [[ "$status" -ne 1 ]]; then
    fail "could not scan $file for forbidden pattern: $pattern"
  fi
}

for file in "$kernel" "$scenarios" "$runner" "$makefile" "$matrix" "$matrix_guard" "$plan" "$frontier"; do
  require_file "$file"
done

require_literal "$kernel" '#[cfg(feature = "fat32-write-proof")]' \
  "write proof must stay feature-gated in the kernel call site"
require_literal "$kernel" 'kernel.run_fat32_write_probe()' \
  "write proof must stay routed through the kernel owner"
require_literal "$scenarios" '#[cfg(feature = "fat32-write-proof")]' \
  "write proof implementation must stay feature-gated in scenarios"
require_literal "$scenarios" 'pub(crate) fn run_fat32_write_probe(&mut self) -> Result<(), &' \
  "write proof implementation must stay owned by scenarios"
require_literal "$scenarios" 'DPMK:FAT32-WRITE-OK' \
  "write proof markers must remain in scenarios"

require_literal "$runner" 'DPCLI:STAT /HELLO.TXT cluster=3 size=40 readonly=1' \
  "storage service counters packet must preserve read-only stat evidence for HELLO"
require_literal "$runner" 'DPCLI:STAT /INDEX.HTM cluster=4 size=110 readonly=1' \
  "storage service counters packet must preserve read-only stat evidence for INDEX"
require_literal "$runner" 'DPMK:FS-STAT-OK:/HELLO.TXT' \
  "storage service counters packet must preserve HELLO stat acknowledgement"
require_literal "$runner" 'DPMK:FS-STAT-OK:/INDEX.HTM' \
  "storage service counters packet must preserve INDEX stat acknowledgement"
require_literal "$runner" '--storage-service-counters-proof' \
  "storage service counters packet must expose a dedicated proof mode"
require_literal "$runner" 'storage_service_counters_cache_scratch_counters=absent' \
  "storage service counters packet must say cache/scratch counters are absent when source-real evidence is missing"
require_literal "$runner" 'storage_service_counters_filesystem_error_classes=unsupported-path,long-filename,unsupported-write-shape' \
  "storage service counters packet must keep filesystem error classes source-real"

require_literal "$makefile" 'x86_64-microkernel-storage-service-counters-contract:' \
  "Makefile must expose storage service counters contract"
require_literal "$makefile" './tools/check_x86_64_microkernel_storage_service_counters_contract.sh' \
  "Makefile contract must run this guard"
require_literal "$makefile" 'x86_64-microkernel-storage-service-counters: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-storage-integrity-frontier-contract x86_64-microkernel-storage-service-counters-contract' \
  "Makefile proof target must use focused guards and avoid fat32-write dependency"
require_literal "$makefile" 'DP_MICROKERNEL_STORAGE_SERVICE_COUNTERS_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --storage-service-counters-proof' \
  "Makefile proof target must run focused mode"

require_literal "$matrix" 'storage-service-counters' \
  "validation matrix must include storage service counters scenario"
require_literal "$matrix" "storage-service-counters) echo 'storage service counters summary|source scan' ;;" \
  "validation matrix must require honest source-scan artifacts"

require_literal "$matrix_guard" 'storage-service-counters' \
  "validation matrix guard must cover the storage service counters scenario"

require_literal "$plan" 'storage-service-counters' \
  "pre-TLS plan must route storage service counters"
for literal in \
  'no journaling implementation' \
  'no durable-write claim' \
  'no long filename or directory expansion'; do
  require_literal "$plan" "$literal" \
    "pre-TLS plan must keep storage overclaim stop line explicit: $literal"
done
require_literal "$frontier" 'storage-service counters' \
  "frontier must route storage-service counters as next frontier"
for literal in \
  'journaling, crash consistency, durable writes, default writable' \
  'appliance mode, long filename support, arbitrary host filesystem, TLS/HTTPS,' \
  'and hardware readiness'; do
  require_literal "$frontier" "$literal" \
    "frontier must name storage overclaim explicitly: $literal"
done

for file in "$kernel" "$scenarios" "$runner" "$makefile" "$matrix"; do
  reject_regex "$file" 'curl -k|openssl|rustls|embedded-tls|webpki|aws-lc-rs|https://' \
    "storage service counters packet must not add TLS/HTTPS tooling"
  reject_regex "$file" 'CrashConsistency|crash consistency guarantee|durable write guarantee|DurabilityClaim|whole-FAT32 crash consistency' \
    "storage service counters packet must not claim durability or crash consistency"
  reject_regex "$file" 'default writable|default-write mode|writable appliance mode' \
    "storage service counters packet must not make default storage writable"
  reject_regex "$file" 'arbitrary host filesystem|host filesystem access' \
    "storage service counters packet must not broaden to arbitrary host filesystems"
  reject_regex "$file" 'hardware-ready|hardware readiness|silicon-ready' \
    "storage service counters packet must not claim hardware readiness"
done

echo "x86_64 microkernel storage service counters contract guard passed."
