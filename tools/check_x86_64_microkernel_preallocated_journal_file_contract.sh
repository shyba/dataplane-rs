#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
scenarios="crates/dataplane-x86_64-microkernel-smoke/src/scenarios.rs"
fat32="crates/dataplane-x86_64-microkernel-smoke/src/fat32.rs"
cargo_toml="crates/dataplane-x86_64-microkernel-smoke/Cargo.toml"
image_tool="tools/microkernel_make_fat32_image.py"
runner="tools/x86_64_microkernel_fat32_run.sh"
makefile="Makefile"
packet_spec="changes/__archived_changes_2026-06-01/x86_64-microkernel-storage-prereq-contract-repair/specs/storage-prereq-contract-repair/spec.md"
packet_proposal="changes/__archived_changes_2026-06-01/x86_64-microkernel-storage-prereq-contract-repair/proposal.md"
packet_design="changes/__archived_changes_2026-06-01/x86_64-microkernel-storage-prereq-contract-repair/design.md"

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

for file in "$main" "$scenarios" "$fat32" "$cargo_toml" "$image_tool" "$runner" "$makefile" "$packet_spec" "$packet_proposal" "$packet_design"; do
  require_file "$file"
done

for literal in \
  'preallocated-journal-file-proof = ["fat32-write-proof"]' \
  'preallocated-journal-file-proof'; do
  require_literal "$cargo_toml" "$literal" \
    "crate feature must expose preallocated journal proof: $literal"
done

for literal in \
  'JOURNAL_CLUSTER = 9' \
  'JOURNAL_NAME = b"JOURNAL BIN"' \
  'root[128:160] = short_entry(JOURNAL_NAME, JOURNAL_CLUSTER, BYTES_PER_SECTOR)' \
  'write_file(image, JOURNAL_CLUSTER, bytes(BYTES_PER_SECTOR))' \
  'print(f"journal_cluster={JOURNAL_CLUSTER}")'; do
  require_literal "$image_tool" "$literal" \
    "image builder must preallocate fixed journal file: $literal"
done

for literal in \
  '#[cfg(feature = "fat32-write-proof")]' \
  'const FAT32_OUT_CLUSTER: u32 = 5;' \
  'const FAT32_OUT_NAME: [u8; 11] = *b"OUT     TXT";' \
  'const FAT32_OUT_CONTENT: &[u8] = b"dataplane guest fat32 write proof\r\n";'; do
  require_literal "$fat32" "$literal" \
    "journal proof contract must preserve moved FAT32 write-proof constants: $literal"
done

for literal in \
  'write_fat_eoc(memory, FAT32_OUT_CLUSTER)' \
  'write_file_content(memory, FAT32_OUT_CONTENT)' \
  'verify_fat_eoc(memory, FAT32_OUT_CLUSTER)' \
  'verify_file_content(memory, FAT32_OUT_CONTENT)'; do
  require_literal "$scenarios" "$literal" \
    "journal proof contract must stay narrowed to the current feature-gated boundary: $literal"
done

for literal in \
  '--write-proof' \
  '--preallocated-journal-file-proof' \
  'DP_MICROKERNEL_PREALLOCATED_JOURNAL_FILE_PROOF' \
  'write_mode=fat32-host-readback' \
  'write_mode=preallocated-journal-file-proof' \
  'journal_file=preallocated:/JOURNAL.BIN' \
  'write_path=/OUT.TXT' \
  'write_scope=single-root-out-txt-create-write-only-no-directories-delete-rename' \
  'write_marker_prefix=DPMK:FAT32-WRITE-' \
  'write_host_inspection=after-qemu-exit'; do
  require_literal "$runner" "$literal" \
    "runner must stay on the current feature-gated write-proof boundary: $literal"
done

for literal in \
  'x86_64-microkernel-preallocated-journal-file-proof-build:' \
  'X86_64_MICROKERNEL_FEATURES=preallocated-journal-file-proof ./tools/x86_64_microkernel_smoke_build.sh' \
  'x86_64-microkernel-preallocated-journal-file-proof-contract:' \
  './tools/check_x86_64_microkernel_preallocated_journal_file_contract.sh' \
  'DP_MICROKERNEL_PREALLOCATED_JOURNAL_FILE_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --preallocated-journal-file-proof'; do
  require_literal "$makefile" "$literal" \
    "Makefile must wire journal proof literal: $literal"
done

for literal in \
  'preallocated journal file exists in the test image;' \
  'feature-gated proof/design only' \
  'no default write enablement' \
  'no crash-consistency claim' \
  'no broad write path'; do
  require_literal "$packet_design" "$literal" \
    "storage prereq design must state the current journal boundary: $literal"
  require_literal "$packet_spec" "$literal" \
    "storage prereq spec must state the current journal boundary: $literal"
done

for file in "$main" "$runner" "$makefile" "$image_tool"; do
  reject_regex "$file" 'curl -k|openssl|rustls|embedded-tls|webpki|aws-lc-rs|https://' \
    "journal proof packet must not add TLS/HTTPS tooling"
  reject_regex "$file" 'default writable|default-write mode|writable appliance mode' \
    "journal proof packet must not make default storage writable"
  reject_regex "$file" 'arbitrary file writes|directory expansion|long filename support|generic filesystem API' \
    "journal proof packet must not broaden filesystem scope"
  reject_regex "$file" 'hardware-ready|hardware readiness|silicon-ready' \
    "journal proof packet must not claim hardware readiness"
done

reject_regex "$main" 'FAT32_OUT_ROOT_ENTRY_OFFSET: usize = 128' \
  "OUT.TXT write-proof slot must move after the preallocated journal entry"

echo "x86_64 microkernel preallocated journal file contract guard passed."
