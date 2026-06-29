#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

design="aidocs/053_microkernel_storage_durability_design_2026-05-31.md"
main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
makefile="Makefile"
frontier="aidocs/050_microkernel_robust_design_frontier_2026-05-30.md"
plan="aidocs/051_microkernel_pre_tls_appliance_plan_2026-05-30.md"
tls_deferred="aidocs/052_microkernel_tls_deferred_appliance_expansion_plan_2026-05-31.md"

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
  local note="$3"
  rg -q --fixed-strings -- "$literal" "$file" || fail "$note"
}

reject_regex() {
  local file="$1"
  local pattern="$2"
  local note="$3"
  local matches status
  set +e
  matches="$(rg -n -- "$pattern" "$file")"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    printf '%s\n' "$matches" >&2
    fail "$note"
  fi
  [[ "$status" -eq 1 ]] || fail "could not scan $file for forbidden regex: $pattern"
}

for file in "$design" "$main" "$runner" "$makefile" "$frontier" "$plan" "$tls_deferred"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel Storage Durability Design Contract ==="

for literal in \
  'design-only' \
  'Decision: keep the appliance read-only by default.' \
  'prefer a preallocated append-only journal file over copy-on-write FAT32 updates' \
  'This is a design-only packet, not an implementation packet.' \
  'Default storage mode is `readonly`.' \
  'The write proof is feature-gated by `fat32-write-proof`.' \
  'The write proof is ordered as data sector, FAT metadata, then root directory' \
  'What the current surface does not prove:' \
  'power-loss safety;' \
  'FAT copy repair;' \
  'directory-entry rollback;' \
  'journal replay;' \
  'image recovery after a torn write;' \
  'Data sector boundary:' \
  'FAT primary boundary:' \
  'FAT mirror boundary:' \
  'Directory entry boundary:' \
  'Scratch staging boundary:' \
  'Cache boundary:' \
  'Host image boundary:' \
  'Replay boundary:' \
  'x86_64-microkernel-preallocated-journal-file-proof' \
  'no default writable appliance mode;' \
  'no arbitrary file writes;' \
  'no crash-consistency claim for FAT32;' \
  'no TLS, HTTPS, certificate, entropy, or crypto-provider work.'; do
  require_literal "$design" "$literal" "storage durability design missing required literal: $literal"
done

for literal in \
  ' readonly=1' \
  'DPCLI:STAT ' \
  '#[cfg(feature = "fat32-write-proof")]' \
  'self.write_out_data_cluster(layout)?;' \
  'self.write_out_fat_entry(fat_entry_sector)?;' \
  'self.write_out_root_entry(layout)?;' \
  ; do
  require_literal "$main" "$literal" "guest storage source must preserve current bounded storage surface: $literal"
done

for literal in \
  'x86_64-microkernel-storage-durability-design-contract:' \
  './tools/check_x86_64_microkernel_storage_durability_design_contract.sh' \
  'x86_64-microkernel-storage-durability-design: guard-scripts-executable x86_64-microkernel-storage-integrity-frontier-contract x86_64-microkernel-storage-service-counters-contract x86_64-microkernel-filesystem-read-matrix-contract x86_64-microkernel-fs-policy-and-directory-slice-contract x86_64-microkernel-fault-and-recovery-preconditions-contract x86_64-microkernel-storage-durability-design-contract'; do
  require_literal "$makefile" "$literal" "Makefile must wire storage durability design target: $literal"
done

require_literal "$frontier" 'x86_64-microkernel-storage-durability-design' \
  "frontier must route storage durability design"
require_literal "$plan" 'x86_64-microkernel-storage-durability-design' \
  "pre-TLS plan must route storage durability design"
require_literal "$tls_deferred" 'x86_64-microkernel-storage-durability-design' \
  "TLS-deferred plan must route storage durability design"

for file in "$main" "$runner" "$makefile"; do
  reject_regex "$file" 'default writable|default-write mode|writable appliance mode' \
    "storage durability design must not make storage writable by default"
  reject_regex "$file" 'CrashConsistency|crash consistency guarantee|durable write guarantee|DurabilityClaim|whole-filesystem crash consistency' \
    "storage durability design must not overclaim durability in executable paths"
  reject_regex "$file" 'arbitrary host filesystem|host filesystem access|generic filesystem API|long filename support' \
    "storage durability design must not broaden filesystem scope"
done

reject_regex "$makefile" 'storage-durability-design.*(curl -k|https://|openssl|rustls|embedded-tls|webpki|aws-lc-rs|DPCLI:(RESTART|RESET|SHELL)|DPMK:(RESTART|RESET-COMMAND)|debugger|raw memory|MMIO dump|page-table dump|script engine|serde_json|hardware-ready|hardware readiness|silicon-ready|--storage-durability-design|DP_MICROKERNEL_STORAGE_DURABILITY_DESIGN_PROOF|default_writable=true|crash_consistency=true|broad FAT32 writes|benchmark claim)' \
  "storage durability design target must not route TLS/debugger/raw/JSON/hardware-readiness or runtime-durability overclaims"

echo "x86_64 microkernel storage durability design contract passed."
