#!/usr/bin/env bash
set -euo pipefail

target="thumbv6m-none-eabi"

echo "=== QEMU Cortex-M0 Status ==="

if rustup target list --installed | rg -Fxq "$target"; then
  echo "target $target: installed"
else
  echo "FAIL: target $target is not installed"
  exit 1
fi

if command -v qemu-system-arm >/dev/null 2>&1; then
  echo "qemu-system-arm: present"
else
  echo "FAIL: qemu-system-arm is not available"
  exit 1
fi

if qemu-system-arm -machine help | rg -q '^microbit\s'; then
  echo "QEMU machine microbit: present"
else
  echo "FAIL: qemu-system-arm does not list the microbit machine"
  exit 1
fi

echo "QEMU Cortex-M0 status passed."
