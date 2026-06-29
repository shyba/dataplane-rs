#!/usr/bin/env bash

set -euo pipefail

target="thumbv6m-none-eabi"

echo "=== RP2040 Toolchain Gate ==="

if rustup target list --installed | rg -Fxq "$target"; then
  echo "target $target: installed"
else
  echo "FAIL: target $target is not installed"
  exit 1
fi

for tool in arm-none-eabi-objcopy arm-none-eabi-size arm-none-eabi-readelf; do
  if command -v "$tool" >/dev/null 2>&1; then
    echo "$tool: present"
  else
    echo "FAIL: $tool is not available"
    exit 1
  fi
done

echo "rustc cfg for $target:"
rustc --print cfg --target "$target"

optional_tools=(
  probe-rs
  cargo-embed
  elf2uf2
  elf2uf2-rs
  picotool
  openocd
)

missing_optional=()
for tool in "${optional_tools[@]}"; do
  if command -v "$tool" >/dev/null 2>&1; then
    echo "$tool: present"
  else
    missing_optional+=("$tool")
  fi
done

if ((${#missing_optional[@]} > 0)); then
  echo "flash/debug tools missing (non-fatal): ${missing_optional[*]}"
else
  echo "flash/debug tools missing (non-fatal): none"
fi

echo "RP2040 toolchain gate passed."
