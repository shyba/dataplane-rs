#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/.."

target="thumbv6m-none-eabi"
elf="target/$target/release/dataplane-rp2040-noalloc-smoke"

echo "=== RP2040 No-Alloc Smoke Size ==="

./tools/rp2040_noalloc_smoke_build.sh

if [[ ! -f "$elf" ]]; then
  echo "FAIL: release ELF not found: $elf"
  echo "Run: make rp2040-noalloc-smoke-build"
  exit 1
fi

if find crates/dataplane-rp2040-noalloc-smoke \
  \( -name '*.rs' -o -name 'Cargo.toml' -o -name 'build.rs' -o -name 'memory.x' \) \
  -newer "$elf" | rg -q .; then
  echo "FAIL: release ELF is older than smoke crate sources"
  echo "Run: make rp2040-noalloc-smoke-build"
  exit 1
fi

echo ""
echo "=== ELF Section Info ==="
arm-none-eabi-size "$elf"
echo ""

echo "=== Key Sections ==="
# Show key sections with sizes
arm-none-eabi-readelf -S "$elf" 2>/dev/null | rg '\[\s*[0-9]+\]\s+(\.text|\.data|\.bss|\.rodata|\.vector_table)' || true

echo ""
echo "RP2040 no-alloc smoke size passed."