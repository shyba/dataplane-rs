#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

target="thumbv6m-none-eabi"
elf="target/$target/release/dataplane-qemu-cortexm0-smoke"

echo "=== QEMU Cortex-M0 Smoke ==="

if [[ ! -f "$elf" ]]; then
  echo "FAIL: release ELF not found: $elf"
  echo "Run: make qemu-cortexm0-smoke-build"
  exit 1
fi

timeout 20s qemu-system-arm \
  -M microbit \
  -cpu cortex-m0 \
  -nographic \
  -monitor none \
  -serial none \
  -semihosting-config enable=on,target=native \
  -kernel "$elf"

echo "QEMU Cortex-M0 smoke passed."
