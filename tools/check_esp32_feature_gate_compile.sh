#!/usr/bin/env bash

set -euo pipefail

target="${1:-xtensa-esp32-none-elf}"
log_file="$(mktemp)"
trap 'rm -f "$log_file"' EXIT

target_check_cmd=(
  cargo check
  -p dataplane-runtime
  --features esp32-integration
  --target "$target"
  --lib
)

host_check_cmd=(
  cargo check
  -p dataplane-runtime
  --features esp32-integration
  --lib
)

run_host_fallback() {
  echo "[esp32-check] running host fallback compile-only gate: ${host_check_cmd[*]}"
  "${host_check_cmd[@]}"
}

if ! rustc --print target-list | rg -Fxq "$target"; then
  echo "[esp32-check] target '$target' is not available in rustc target list on this toolchain."
  run_host_fallback
  exit 0
fi

echo "[esp32-check] running target compile-only gate: ${target_check_cmd[*]}"
if "${target_check_cmd[@]}" >"$log_file" 2>&1; then
  cat "$log_file"
  echo "[esp32-check] target compile-only gate passed for '$target'."
  exit 0
fi

cat "$log_file" >&2
if rg -q \
  'can'\''t find crate for `core`|can'\''t find crate for `std`|is not a recognized processor|target may not be installed|pkg-config has not been configured to support cross-compilation' \
  "$log_file"; then
  echo "[esp32-check] target compile-only gate is unsupported by this local toolchain for '$target'."
  run_host_fallback
  exit 0
fi

echo "[esp32-check] target compile-only gate failed for '$target' with a non-toolchain-support error." >&2
exit 1
