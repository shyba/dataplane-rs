#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

runner="tools/x86_64_microkernel_fat32_integrity_readonly.sh"
builder="tools/microkernel_make_fat32_image.py"
main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
fat32="crates/dataplane-x86_64-microkernel-smoke/src/fat32.rs"
makefile="Makefile"
fat32_runner="tools/x86_64_microkernel_fat32_run.sh"

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
  grep -Fq -- "$literal" "$file" || fail "$note; missing literal in $file: $literal"
}

reject_regex() {
  local file="$1"
  local regex="$2"
  local note="$3"
  local tmp_dir out err status
  tmp_dir="${TMPDIR:-/home/user/mnt/dataplane/tmp}"
  mkdir -p "$tmp_dir"
  out="$(mktemp "$tmp_dir/fat32-integrity-guard.XXXXXX")"
  err="$(mktemp "$tmp_dir/fat32-integrity-guard-err.XXXXXX")"
  set +e
  rg -n -- "$regex" "$file" >"$out" 2>"$err"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    cat "$out" >&2
    rm -f "$out" "$err"
    fail "$note"
  fi
  if [[ "$status" -ne 1 ]]; then
    cat "$err" >&2 || true
    rm -f "$out" "$err"
    fail "could not scan $file for forbidden regex: $regex"
  fi
  rm -f "$out" "$err"
}

echo "=== x86_64 Microkernel FAT32 Integrity Readonly Contract ==="

for file in "$runner" "$builder" "$main" "$fat32" "$makefile" "$fat32_runner"; do
  require_file "$file"
done

for literal in \
  'mnt_root="/home/user/mnt/dataplane"' \
  'appliance_image="$mnt_root/microkernel-fat32.img"' \
  'expected_fat32_sha256="66bea60b6cbce4be68aabf7f36e4485dde589eef23453f7613af89b8d4c155cf"' \
  'expected_fat32_bytes="67108864"' \
  'capture_image_identity()' \
  'validate_fixed_image_identity()' \
  'validate_summary_image_identity()' \
  'run_identity_negative_fixtures()' \
  'validate_fixed_image_identity "$appliance_image"' \
  'validate_summary_image_identity "$identity_capture" "$appliance_image"' \
  'run_identity_negative_fixtures' \
  'fat32_integrity_readonly_log_root=$log_dir' \
  'fat32_integrity_readonly_appliance_image=$appliance_image' \
  'fat32_integrity_readonly_appliance_image_bytes=$appliance_bytes' \
  'fat32_integrity_readonly_appliance_image_sha256=$appliance_sha' \
  'fat32_integrity_readonly_expected_image_bytes=$expected_fat32_bytes' \
  'fat32_integrity_readonly_expected_image_sha256=$expected_fat32_sha256' \
  'fat32_integrity_readonly_missing_identity_fixture_rejected=true' \
  'fat32_integrity_readonly_mismatched_identity_fixture_rejected=true' \
  'fat32_integrity_readonly_replaced_after_capture_fixture_rejected=true' \
  'fat32_integrity_readonly_appliance_identity_checked_before_guest_evidence=true' \
  'empty identity field' \
  'duplicate identity key' \
  'current image sha256 mismatch' \
  'current image byte length mismatch' \
  'missing appliance identity fixture unexpectedly passed' \
  'mismatched appliance identity fixture unexpectedly passed' \
  'replaced-after-capture appliance identity fixture unexpectedly passed' \
  'run_bad_image_case()' \
  'mutate_bad_image()' \
  'bad_boot_signature block-sector0-signature' \
  'bad_bytes_per_sector fat32-bytes-per-sector' \
  'bad_label fat32-label' \
  'bad_fat_entry fat32-fat-entry' \
  'oversized_hello fat32-hello-entry' \
  'missing_root_entry fat32-entry-name' \
  'malformed_root_attr fat32-entry-attr' \
  'bad_file_content fat32-file-content' \
  'short_image virtio-blk-status' \
  'DPMK:FAIL:$expected_reason' \
  'fat32_integrity_readonly_summary_status=pass' \
  '"fat32_integrity_readonly_appliance_image": "/home/user/mnt/dataplane/microkernel-fat32.img"' \
  '"fat32_integrity_readonly_appliance_image_bytes": "67108864"' \
  '"fat32_integrity_readonly_appliance_image_sha256": "66bea60b6cbce4be68aabf7f36e4485dde589eef23453f7613af89b8d4c155cf"' \
  '"fat32_integrity_readonly_expected_image_bytes": "67108864"' \
  '"fat32_integrity_readonly_expected_image_sha256": "66bea60b6cbce4be68aabf7f36e4485dde589eef23453f7613af89b8d4c155cf"' \
  '"fat32_integrity_readonly_missing_identity_fixture_rejected": "true"' \
  '"fat32_integrity_readonly_mismatched_identity_fixture_rejected": "true"' \
  '"fat32_integrity_readonly_replaced_after_capture_fixture_rejected": "true"' \
  '"fat32_integrity_readonly_appliance_identity_checked_before_guest_evidence": "true"' \
  '"fat32_integrity_readonly_bad_boot_signature_rejected": "true"' \
  '"fat32_integrity_readonly_bad_bytes_per_sector_rejected": "true"' \
  '"fat32_integrity_readonly_bad_label_rejected": "true"' \
  '"fat32_integrity_readonly_bad_fat_entry_rejected": "true"' \
  '"fat32_integrity_readonly_oversized_hello_rejected": "true"' \
  '"fat32_integrity_readonly_missing_root_entry_rejected": "true"' \
  '"fat32_integrity_readonly_malformed_root_attr_rejected": "true"' \
  '"fat32_integrity_readonly_bad_file_content_rejected": "true"' \
  '"fat32_integrity_readonly_short_image_rejected": "true"' \
  'fat32_integrity_readonly_default_writable=false' \
  'fat32_integrity_readonly_repair_mode=false' \
  'fat32_integrity_readonly_fsck=false' \
  'fat32_integrity_readonly_write_feature_used=false' \
  'fat32_integrity_readonly_crash_consistency_claim=false' \
  'fat32_integrity_readonly_tls=false' \
  'fat32_integrity_readonly_benchmark_result=false'; do
  require_literal "$runner" "$literal" "runner must preserve FAT32 integrity readonly literal"
done

for literal in \
  'mnt_root="/home/user/mnt/dataplane"' \
  'fs_img="$mnt_root/microkernel-fat32.img"' \
  'tools/microkernel_make_fat32_image.py "$fs_img"' \
  '-drive if=none,id=fs,file="$fs_img",format=raw'; do
  require_literal "$fat32_runner" "$literal" "FAT32 HTTP/CLI runner must preserve appliance image binding"
done

if grep -Fq 'validate_fixed_image_identity "$valid_image"' "$runner" || \
   grep -Fq 'validate_summary_image_identity "$identity_capture" "$valid_image"' "$runner"; then
  fail "integrity runner must not validate only a case-dir valid image before guest evidence"
fi

identity_gate_line="$(awk '/validate_summary_image_identity "\$identity_capture" "\$appliance_image"/ { print NR; exit }' "$runner")"
bad_matrix_line="$(awk '/run_bad_image_case bad_boot_signature block-sector0-signature/ { print NR; exit }' "$runner")"
if [[ -z "$identity_gate_line" || -z "$bad_matrix_line" ]]; then
  fail "could not locate identity gate or bad-image matrix line in runner"
fi
if (( identity_gate_line >= bad_matrix_line )); then
  fail "identity gate must execute before bad-image guest evidence is accepted"
fi

for literal in \
  'pub(crate) fn parse_boot_sector(' \
  'return Err("fat32-boot-signature");' \
  'return Err("fat32-bytes-per-sector");' \
  'return Err("fat32-label");' \
  'fn verify_fat_entries(&self, memory: &FsTaskMemory' \
  'return Err("fat32-fat-entry");' \
  'fn parse_root_directory(' \
  'return Err("fat32-hello-entry");' \
  'return Err("fat32-file-content");' \
  'fn parse_short_entry(entry: &[u8], expected_name: [u8; 11])'; do
  require_literal "$fat32" "$literal" "guest must preserve FAT32 integrity error surface"
done

for literal in \
  'TOTAL_SECTORS = 131_072' \
  'write_boot_sector(image)' \
  'write_fats(image)' \
  'write_root_directory(image)' \
  'write_file(image, HELLO_CLUSTER, HELLO_CONTENT)' \
  'write_file(image, INDEX_CLUSTER, INDEX_CONTENT)' \
  'write_file(image, LARGE_CLUSTER, LARGE_CONTENT)'; do
  require_literal "$builder" "$literal" "FAT32 builder must preserve deterministic image shape"
done

for literal in \
  'x86_64-microkernel-fat32-integrity-readonly-contract:' \
  './tools/check_x86_64_microkernel_fat32_integrity_readonly_contract.sh' \
  'x86_64-microkernel-fat32-integrity-readonly:' \
  './tools/x86_64_microkernel_fat32_integrity_readonly.sh'; do
  require_literal "$makefile" "$literal" "Makefile must expose FAT32 integrity readonly target"
done

for file in "$runner" "$makefile"; do
  reject_regex "$file" 'fat32[-_ ]?integrity[-_ ]?readonly.*(fsck=true|repair_mode=true|default_writable=true|write_feature_used=true|crash_consistency_claim=true)' \
    "FAT32 integrity readonly packet must not claim repair, default writes, write feature use, or crash consistency in $file"
  reject_regex "$file" 'fat32[-_ ]?integrity[-_ ]?readonly.*(https://|curl[[:space:]]+-k|openssl|rustls|embedded-tls|webpki|aws-lc-rs|certificate|entropy)' \
    "FAT32 integrity readonly packet must not reopen TLS or crypto work in $file"
  reject_regex "$file" 'fat32[-_ ]?integrity[-_ ]?readonly.*(STRICT_FIVE_CALIBRATION|run_strict_five|strict-five|benchmark tuning|retune)' \
    "FAT32 integrity readonly packet must not tune or substitute benchmarks in $file"
done

echo "x86_64 microkernel FAT32 integrity readonly contract OK"
