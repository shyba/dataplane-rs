#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/.."

target="thumbv6m-none-eabi"
elf="target/$target/release/dataplane-rp2040-smoke"

echo "=== RP2040 Smoke Size ==="

./tools/rp2040_smoke_build.sh

if [[ ! -f "$elf" ]]; then
  echo "FAIL: release ELF not found: $elf"
  echo "Run: make rp2040-smoke-build"
  exit 1
fi

if find crates/dataplane-rp2040-smoke \
  \( -name '*.rs' -o -name 'Cargo.toml' -o -name 'build.rs' -o -name 'memory.x' \) \
  -newer "$elf" | rg -q .; then
  echo "FAIL: release ELF is older than smoke crate sources"
  echo "Run: make rp2040-smoke-build"
  exit 1
fi

arm-none-eabi-size "$elf"
echo ""
echo "=== Key Sections ==="

if ! command -v arm-none-eabi-readelf >/dev/null 2>&1; then
  echo "FAIL: arm-none-eabi-readelf is not available"
  exit 1
fi

section_output="$(arm-none-eabi-readelf -S "$elf" 2>/dev/null)"

hex_to_dec() {
  local hex="$1"
  hex="${hex#0}"
  [ -z "$hex" ] && hex="0"
  printf '%d' "0x$hex"
}

check_required_section() {
  local section="$1"
  local size
  size="$(printf '%s\n' "$section_output" | rg "^\s*\[\s*[0-9]+\]\s+$section\b" | awk '{print $7}')"
  if [ -z "$size" ]; then
    echo "FAIL: required section $section is missing"
    return 1
  fi
  local size_dec
  size_dec="$(hex_to_dec "$size")"
  echo "  $section: present (size $size_dec / 0x$size bytes)"
}

FAILED=0
check_required_section ".vector_table" || FAILED=1
check_required_section ".text" || FAILED=1
check_required_section ".data" || FAILED=1
check_required_section ".bss" || FAILED=1

text_size="$(printf '%s\n' "$section_output" | rg "^\s*\[\s*[0-9]+\]\s+\.text\b" | awk '{print $7}')"
if [ -n "$text_size" ] && [ "$(hex_to_dec "$text_size")" -eq 0 ]; then
  echo "FAIL: .text section is empty"
  FAILED=1
fi

if [ "$FAILED" -ne 0 ]; then
  echo "RP2040 smoke ELF section validation failed."
  exit 1
fi

echo "RP2040 smoke size passed."
