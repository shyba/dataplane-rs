#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
makefile="Makefile"
matrix="tools/x86_64_microkernel_validation_matrix_run.sh"
matrix_guard="tools/check_x86_64_microkernel_validation_matrix_contract.sh"

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

rust_fn_body() {
  local file="$1"
  local function="$2"
  local body status
  set +e
  body="$(awk -v target="fn ${function}" '
    BEGIN { depth = 0; found = 0; started = 0 }
    !started && index($0, target) {
      started = 1
      found = 1
    }
    started {
      print
      line = $0
      opens = gsub(/\{/, "{", line)
      line = $0
      closes = gsub(/\}/, "}", line)
      depth += opens - closes
      if (depth == 0 && started) {
        exit 0
      }
    }
    END {
      if (!found || depth != 0) {
        exit 42
      }
    }
  ' "$file")"
  status=$?
  set -e
  [[ "$status" -eq 0 ]] || fail "could not extract function body for ${function}"
  printf '%s\n' "$body"
}

require_fn_literal() {
  local file="$1"
  local function="$2"
  local needle="$3"
  local note="$4"
  local body
  body="$(rust_fn_body "$file" "$function")"
  [[ "$body" == *"$needle"* ]] || fail "$note"
}

require_order() {
  local text="$1"
  local first="$2"
  local second="$3"
  local note="$4"
  local first_line second_line
  first_line="$(printf '%s\n' "$text" | rg -n --fixed-strings -- "$first" | head -n 1 | cut -d: -f1 || true)"
  second_line="$(printf '%s\n' "$text" | rg -n --fixed-strings -- "$second" | head -n 1 | cut -d: -f1 || true)"
  [[ -n "$first_line" && -n "$second_line" ]] || fail "$note"
  (( first_line < second_line )) || fail "$note"
}

for file in "$main" "$runner" "$makefile" "$matrix" "$matrix_guard"; do
  require_file "$file"
done

require_literal "$main" '#[cfg(feature = "fat32-write-proof")]' \
  "write proof code must remain cfg-gated"

write_body="$(rust_fn_body "$main" "run_fat32_write_probe")"
require_order "$write_body" 'self.write_out_fat_entry(fat_entry_sector)?;' 'self.write_out_fat_entry(fat_entry_sector + layout.fat_sectors)?;' \
  "write proof must update both FAT copies in order"
require_order "$write_body" 'self.write_out_fat_entry(fat_entry_sector + layout.fat_sectors)?;' 'self.write_out_data_cluster(layout)?;' \
  "write proof must write data after FAT metadata"
require_order "$write_body" 'self.write_out_data_cluster(layout)?;' 'self.write_out_root_entry(layout)?;' \
  "write proof must write root directory entry after data"

require_literal "$runner" 'DPCLI:STAT /HELLO.TXT cluster=3 size=40 readonly=1' \
  "runner must require read-only storage status for /HELLO.TXT"
require_literal "$runner" 'DPCLI:STAT /INDEX.HTM cluster=4 size=110 readonly=1' \
  "runner must require read-only storage status for /INDEX.HTM"
require_literal "$runner" 'DPMK:FS-STAT-OK:/HELLO.TXT' \
  "runner must require HELLO read-only status acknowledgement"
require_literal "$runner" 'DPMK:FS-STAT-OK:/INDEX.HTM' \
  "runner must require INDEX read-only status acknowledgement"

require_literal "$makefile" 'x86_64-microkernel-storage-integrity-frontier-contract:' \
  "Makefile must expose the storage integrity contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_storage_integrity_contract.sh' \
  "Makefile storage integrity contract must run this guard"
require_literal "$makefile" 'x86_64-microkernel-storage-integrity-frontier: guard-scripts-executable x86_64-microkernel-fat32-smoke x86_64-microkernel-fat32-write x86_64-microkernel-storage-integrity-frontier-contract' \
  "Makefile storage integrity target must preserve default read-only and feature-gated write proofs"
require_literal "$makefile" "echo 'x86_64 microkernel storage integrity proof passed.'" \
  "Makefile storage integrity target must emit the scenario success marker"

require_literal "$matrix" 'storage-integrity-frontier' \
  "validation matrix must know the storage integrity scenario"
require_literal "$matrix" "storage-integrity-frontier) echo 'make x86_64-microkernel-storage-integrity-frontier' ;;" \
  "validation matrix must dispatch storage integrity through Make"
require_literal "$matrix" "storage-integrity-frontier) echo 'x86_64 microkernel storage integrity proof passed.' ;;" \
  "validation matrix must require the storage integrity marker"
require_literal "$matrix" "storage-integrity-frontier) echo 'client log|serial log|qemu log|network pcap' ;;" \
  "validation matrix must require storage integrity artifacts"
require_literal "$matrix_guard" 'storage-integrity-frontier' \
  "validation matrix guard must cover the storage integrity scenario"

reject_regex "$runner" '(^|[^A-Z0-9_])--tls-proof([^A-Z0-9_]|$)' \
  "storage integrity runner changes must not add TLS proof modes"
reject_regex "$runner" 'DP_MICROKERNEL_TLS_PROOF' \
  "storage integrity runner changes must not add TLS env overrides"
reject_regex "$runner" 'curl[[:space:]]+-k' \
  "storage integrity proof must not use curl -k"

echo "x86_64 microkernel storage integrity contract passed."
