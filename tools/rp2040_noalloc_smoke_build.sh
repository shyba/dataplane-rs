#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/.."

crate_path="crates/dataplane-rp2040-noalloc-smoke/Cargo.toml"
target="thumbv6m-none-eabi"

echo "=== RP2040 No-Alloc Smoke Build ==="

if [[ ! -f "$crate_path" ]]; then
  echo "FAIL: expected crate manifest not found: $crate_path"
  exit 1
fi

cargo build -p dataplane-rp2040-noalloc-smoke --target "$target" --release --features bare-metal-bin
echo "RP2040 no-alloc smoke build passed."
