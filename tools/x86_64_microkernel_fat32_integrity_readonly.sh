#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

target="x86_64-unknown-none"
kernel="target/$target/release/dataplane-x86_64-microkernel-smoke.img"
mnt_root="/home/user/mnt/dataplane"
tmp_root="$mnt_root/tmp"
log_dir="$mnt_root/logs"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
case_dir="$log_dir/x86_64-microkernel-fat32-integrity-readonly-$run_id.cases"
appliance_image="$mnt_root/microkernel-fat32.img"
summary="$log_dir/x86_64-microkernel-fat32-integrity-readonly-$run_id.summary"
expected_fat32_sha256="66bea60b6cbce4be68aabf7f36e4485dde589eef23453f7613af89b8d4c155cf"
expected_fat32_bytes="67108864"

mkdir -p "$tmp_root" "$log_dir" "$case_dir"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

sha256_file() {
  sha256sum "$1" | awk '{ print $1 }'
}

byte_count_file() {
  wc -c <"$1" | tr -d ' '
}

capture_image_identity() {
  local image="$1"
  local out="$2"
  [[ -f "$image" ]] || fail "cannot capture identity for missing image: $image"
  {
    echo "fat32_integrity_readonly_appliance_image=$image"
    echo "fat32_integrity_readonly_appliance_image_bytes=$(byte_count_file "$image")"
    echo "fat32_integrity_readonly_appliance_image_sha256=$(sha256_file "$image")"
  } >"$out"
}

validate_fixed_image_identity() {
  local image="$1"
  local actual_sha actual_bytes
  [[ -f "$image" ]] || fail "missing FAT32 image before evidence collection: $image"
  actual_sha="$(sha256_file "$image")"
  actual_bytes="$(byte_count_file "$image")"
  [[ -n "$actual_sha" ]] || fail "empty FAT32 sha256 capture before evidence collection"
  [[ -n "$actual_bytes" ]] || fail "empty FAT32 byte-length capture before evidence collection"
  [[ "$actual_sha" == "$expected_fat32_sha256" ]] || fail "FAT32 image sha256 mismatch before evidence collection: got $actual_sha expected $expected_fat32_sha256"
  [[ "$actual_bytes" == "$expected_fat32_bytes" ]] || fail "FAT32 image byte length mismatch before evidence collection: got $actual_bytes expected $expected_fat32_bytes"
}

validate_summary_image_identity() {
  local identity_summary="$1"
  local image="$2"
  python3 - "$identity_summary" "$image" "$expected_fat32_sha256" "$expected_fat32_bytes" <<'PY'
import hashlib
import os
import sys

summary_path, image_path, expected_sha, expected_bytes = sys.argv[1:]
required = {
    "fat32_integrity_readonly_appliance_image": image_path,
    "fat32_integrity_readonly_appliance_image_bytes": expected_bytes,
    "fat32_integrity_readonly_appliance_image_sha256": expected_sha,
}
if not os.path.exists(summary_path):
    raise SystemExit(f"missing identity summary: {summary_path}")
if not os.path.exists(image_path):
    raise SystemExit(f"missing image for identity validation: {image_path}")
values = {}
with open(summary_path, "r", encoding="ascii") as handle:
    for raw in handle:
        line = raw.rstrip("\n")
        if "=" not in line:
            raise SystemExit(f"malformed identity line: {line!r}")
        key, value = line.split("=", 1)
        if not key or not value:
            raise SystemExit(f"empty identity field: {line!r}")
        if key in values:
            raise SystemExit(f"duplicate identity key: {key}")
        values[key] = value
for key, expected in required.items():
    actual = values.get(key)
    if actual != expected:
        raise SystemExit(f"identity mismatch for {key}: got {actual!r} expected {expected!r}")
with open(image_path, "rb") as handle:
    data = handle.read()
actual_sha = hashlib.sha256(data).hexdigest()
actual_bytes = str(len(data))
if actual_sha != expected_sha:
    raise SystemExit(f"current image sha256 mismatch: got {actual_sha} expected {expected_sha}")
if actual_bytes != expected_bytes:
    raise SystemExit(f"current image byte length mismatch: got {actual_bytes} expected {expected_bytes}")
PY
}

run_identity_negative_fixtures() {
  local missing_summary="$case_dir/appliance-identity-missing.summary"
  local mismatch_summary="$case_dir/appliance-identity-mismatched.summary"
  local replaced_image="$case_dir/appliance-identity-replaced.img"
  local replaced_summary="$case_dir/appliance-identity-replaced.summary"

  : >"$missing_summary"
  if validate_summary_image_identity "$missing_summary" "$appliance_image" >/dev/null 2>&1; then
    fail "missing appliance identity fixture unexpectedly passed"
  fi

  {
    echo "fat32_integrity_readonly_appliance_image=$appliance_image"
    echo "fat32_integrity_readonly_appliance_image_bytes=$expected_fat32_bytes"
    echo "fat32_integrity_readonly_appliance_image_sha256=0000000000000000000000000000000000000000000000000000000000000000"
  } >"$mismatch_summary"
  if validate_summary_image_identity "$mismatch_summary" "$appliance_image" >/dev/null 2>&1; then
    fail "mismatched appliance identity fixture unexpectedly passed"
  fi

  cp "$appliance_image" "$replaced_image"
  capture_image_identity "$replaced_image" "$replaced_summary"
  printf 'x' | dd of="$replaced_image" bs=1 seek=128 count=1 conv=notrunc status=none
  if validate_summary_image_identity "$replaced_summary" "$replaced_image" >/dev/null 2>&1; then
    fail "replaced-after-capture appliance identity fixture unexpectedly passed"
  fi
}

[[ -f "$kernel" ]] || fail "missing microkernel image: $kernel"
command -v qemu-system-x86_64 >/dev/null 2>&1 || fail "qemu-system-x86_64 not found"

validate_fixed_image_identity "$appliance_image"
identity_capture="$case_dir/appliance-image.identity"
capture_image_identity "$appliance_image" "$identity_capture"
validate_summary_image_identity "$identity_capture" "$appliance_image"
run_identity_negative_fixtures
appliance_sha="$(sha256_file "$appliance_image")"
appliance_bytes="$(byte_count_file "$appliance_image")"

mutate_bad_image() {
  local image="$1"
  local case_name="$2"
  python3 - "$image" "$case_name" <<'PY'
import sys

path, case_name = sys.argv[1:]
data = bytearray(open(path, "rb").read())
sector = 512
root = 2080 * sector
hello = 2081 * sector

if case_name == "bad_boot_signature":
    data[510:512] = b"\x00\x00"
elif case_name == "bad_bytes_per_sector":
    data[11:13] = (1024).to_bytes(2, "little")
elif case_name == "bad_label":
    data[82:90] = b"NOTFAT32"
elif case_name == "bad_fat_entry":
    data[32 * sector + 3 * 4 : 32 * sector + 3 * 4 + 4] = (0).to_bytes(4, "little")
elif case_name == "oversized_hello":
    data[root + 28 : root + 32] = (999).to_bytes(4, "little")
elif case_name == "missing_root_entry":
    data[root : root + 11] = b"MISSING TXT"
elif case_name == "malformed_root_attr":
    data[root + 11] = 0x10
elif case_name == "bad_file_content":
    data[hello : hello + 5] = b"xxxxx"
elif case_name == "short_image":
    data = data[:1024]
else:
    raise SystemExit(f"unknown FAT32 integrity case: {case_name}")

open(path, "wb").write(data)
PY
}

run_bad_image_case() {
  local case_name="$1"
  local expected_reason="$2"
  local bad_image="$case_dir/$case_name.img"
  local serial_log="$case_dir/$case_name.serial.log"
  local qemu_log="$case_dir/$case_name.qemu.log"
  local qemu_pid=""
  local deadline

  cp "$appliance_image" "$bad_image"
  mutate_bad_image "$bad_image" "$case_name"

  qemu-system-x86_64 \
    -M pc \
    -m 128M \
    -nographic \
    -monitor none \
    -drive file="$kernel",format=raw,if=floppy \
    -boot a \
    -drive if=none,id=fs,file="$bad_image",format=raw \
    -device virtio-blk-pci-transitional,drive=fs \
    -serial "file:$serial_log" \
    -no-reboot \
    -no-shutdown >"$qemu_log" 2>&1 &
  qemu_pid="$!"

  deadline=$((SECONDS + 8))
  while (( SECONDS < deadline )); do
    if grep -Fq "DPMK:FAIL:$expected_reason" "$serial_log" 2>/dev/null; then
      kill "$qemu_pid" >/dev/null 2>&1 || true
      wait "$qemu_pid" >/dev/null 2>&1 || true
      {
        echo "fat32_integrity_readonly_${case_name}_rejected=true"
        echo "fat32_integrity_readonly_${case_name}_reason=$expected_reason"
        echo "fat32_integrity_readonly_${case_name}_image=$bad_image"
        echo "fat32_integrity_readonly_${case_name}_serial_log=$serial_log"
        echo "fat32_integrity_readonly_${case_name}_qemu_log=$qemu_log"
      } >>"$summary"
      return 0
    fi
    if ! kill -0 "$qemu_pid" >/dev/null 2>&1; then
      break
    fi
    sleep 0.1
  done

  kill "$qemu_pid" >/dev/null 2>&1 || true
  wait "$qemu_pid" >/dev/null 2>&1 || true
  echo "FAIL: FAT32 integrity case $case_name did not emit DPMK:FAIL:$expected_reason" >&2
  sed -n '1,120p' "$serial_log" >&2 || true
  exit 1
}

{
  echo "fat32_integrity_readonly_summary_status=pass"
  echo "fat32_integrity_readonly_run_id=$run_id"
  echo "fat32_integrity_readonly_log_root=$log_dir"
  echo "fat32_integrity_readonly_case_dir=$case_dir"
  echo "fat32_integrity_readonly_appliance_image=$appliance_image"
  echo "fat32_integrity_readonly_appliance_image_bytes=$appliance_bytes"
  echo "fat32_integrity_readonly_appliance_image_sha256=$appliance_sha"
  echo "fat32_integrity_readonly_expected_image_bytes=$expected_fat32_bytes"
  echo "fat32_integrity_readonly_expected_image_sha256=$expected_fat32_sha256"
  echo "fat32_integrity_readonly_missing_identity_fixture_rejected=true"
  echo "fat32_integrity_readonly_mismatched_identity_fixture_rejected=true"
  echo "fat32_integrity_readonly_replaced_after_capture_fixture_rejected=true"
  echo "fat32_integrity_readonly_appliance_identity_checked_before_guest_evidence=true"
  echo "fat32_integrity_readonly_default_writable=false"
  echo "fat32_integrity_readonly_repair_mode=false"
  echo "fat32_integrity_readonly_fsck=false"
  echo "fat32_integrity_readonly_write_feature_used=false"
  echo "fat32_integrity_readonly_crash_consistency_claim=false"
  echo "fat32_integrity_readonly_tls=false"
  echo "fat32_integrity_readonly_https=false"
  echo "fat32_integrity_readonly_hardware_readiness=false"
  echo "fat32_integrity_readonly_benchmark_result=false"
  echo "fat32_integrity_readonly_stale_or_missing_summary_evidence_rejected=true"
} >"$summary"

run_bad_image_case bad_boot_signature block-sector0-signature
run_bad_image_case bad_bytes_per_sector fat32-bytes-per-sector
run_bad_image_case bad_label fat32-label
run_bad_image_case bad_fat_entry fat32-fat-entry
run_bad_image_case oversized_hello fat32-hello-entry
run_bad_image_case missing_root_entry fat32-entry-name
run_bad_image_case malformed_root_attr fat32-entry-attr
run_bad_image_case bad_file_content fat32-file-content
run_bad_image_case short_image virtio-blk-status

python3 - "$summary" <<'PY'
import sys

path = sys.argv[1]
values = {}
for line in open(path, "r", encoding="ascii"):
    key, value = line.rstrip("\n").split("=", 1)
    if key in values:
        raise SystemExit(f"duplicate summary key: {key}")
    values[key] = value

required = {
    "fat32_integrity_readonly_summary_status": "pass",
    "fat32_integrity_readonly_appliance_image": "/home/user/mnt/dataplane/microkernel-fat32.img",
    "fat32_integrity_readonly_appliance_image_bytes": "67108864",
    "fat32_integrity_readonly_appliance_image_sha256": "66bea60b6cbce4be68aabf7f36e4485dde589eef23453f7613af89b8d4c155cf",
    "fat32_integrity_readonly_expected_image_bytes": "67108864",
    "fat32_integrity_readonly_expected_image_sha256": "66bea60b6cbce4be68aabf7f36e4485dde589eef23453f7613af89b8d4c155cf",
    "fat32_integrity_readonly_missing_identity_fixture_rejected": "true",
    "fat32_integrity_readonly_mismatched_identity_fixture_rejected": "true",
    "fat32_integrity_readonly_replaced_after_capture_fixture_rejected": "true",
    "fat32_integrity_readonly_appliance_identity_checked_before_guest_evidence": "true",
    "fat32_integrity_readonly_bad_boot_signature_rejected": "true",
    "fat32_integrity_readonly_bad_bytes_per_sector_rejected": "true",
    "fat32_integrity_readonly_bad_label_rejected": "true",
    "fat32_integrity_readonly_bad_fat_entry_rejected": "true",
    "fat32_integrity_readonly_oversized_hello_rejected": "true",
    "fat32_integrity_readonly_missing_root_entry_rejected": "true",
    "fat32_integrity_readonly_malformed_root_attr_rejected": "true",
    "fat32_integrity_readonly_bad_file_content_rejected": "true",
    "fat32_integrity_readonly_short_image_rejected": "true",
    "fat32_integrity_readonly_default_writable": "false",
    "fat32_integrity_readonly_repair_mode": "false",
    "fat32_integrity_readonly_fsck": "false",
    "fat32_integrity_readonly_write_feature_used": "false",
    "fat32_integrity_readonly_crash_consistency_claim": "false",
    "fat32_integrity_readonly_tls": "false",
    "fat32_integrity_readonly_https": "false",
    "fat32_integrity_readonly_hardware_readiness": "false",
    "fat32_integrity_readonly_benchmark_result": "false",
    "fat32_integrity_readonly_stale_or_missing_summary_evidence_rejected": "true",
}
for key, expected in required.items():
    if values.get(key) != expected:
        raise SystemExit(f"missing {key}={expected}")
PY

echo "x86_64 microkernel FAT32 integrity readonly proof passed."
echo "summary: $summary"
