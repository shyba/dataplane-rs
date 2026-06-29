#!/usr/bin/env bash
# RP2040 no-alloc smoke contract guard.
#
# The guard rebuilds first, then checks both source-level contract and linked
# ELF symbols. It must not pass on a stale ELF from a prior build.
set -euo pipefail

cd "$(dirname "$0")/.."

target="thumbv6m-none-eabi"
elf="target/$target/release/dataplane-rp2040-noalloc-smoke"
crate_path="crates/dataplane-rp2040-noalloc-smoke/Cargo.toml"
source_dir="crates/dataplane-rp2040-noalloc-smoke/src"
main_source="$source_dir/main.rs"

echo "=== RP2040 No-Alloc Smoke Contract Guard ==="

if [ ! -f "$crate_path" ]; then
    echo "FAIL: no-alloc smoke crate not found at $crate_path"
    exit 1
fi

./tools/rp2040_noalloc_smoke_build.sh

if [ ! -f "$elf" ]; then
    echo "FAIL: no-alloc smoke ELF not built at $elf"
    exit 1
fi

if find crates/dataplane-rp2040-noalloc-smoke \
    \( -name '*.rs' -o -name 'Cargo.toml' -o -name 'build.rs' -o -name 'memory.x' \) \
    -newer "$elf" | rg -q .; then
    echo "FAIL: no-alloc smoke ELF is older than smoke crate sources"
    exit 1
fi

FAILED=0

echo "Contract 1: source must import and exercise the runtime no-alloc surface..."
if rg -q 'dataplane_runtime::noalloc' "$main_source"; then
    echo "  runtime no-alloc import: ok"
else
    echo "FAIL: $main_source must import dataplane_runtime::noalloc"
    FAILED=1
fi

if rg -q 'FixedTaskSlots' "$main_source" && rg -q 'drive_counts_step' "$main_source"; then
    echo "  fixed task slots and count-step exercise: ok"
else
    echo "FAIL: smoke source must exercise FixedTaskSlots and drive_counts_step"
    FAILED=1
fi

echo "Contract 2: source must not define or import heap-backed allocation..."
heap_token_pattern='global_allocator|#\[global_allocator\]|extern\s+crate\s+alloc|use\s+::?alloc|(^|[^A-Za-z0-9_])alloc::|Vec<|String|Box<'
if rg -n "$heap_token_pattern" "$source_dir" >/dev/null 2>&1; then
    echo "FAIL: heap-backed no-alloc smoke source token found"
    rg -n "$heap_token_pattern" "$source_dir" || true
    FAILED=1
else
    echo "  source allocation tokens: absent"
fi

echo "Contract 3: linked ELF must not reference allocator symbols..."
for sym in \
    malloc calloc realloc free \
    __rust_alloc __rust_alloc_zeroed __rust_dealloc __rust_realloc \
    __rdl_alloc __rdl_dealloc __rg_alloc GlobalAlloc
do
    if arm-none-eabi-nm "$elf" 2>/dev/null | rg -F "$sym" >/dev/null 2>&1; then
        echo "FAIL: forbidden allocator symbol found: $sym"
        FAILED=1
    fi
done

echo "Contract 4: linked ELF must contain dataplane code..."
if arm-none-eabi-nm "$elf" 2>/dev/null | rg -q 'dataplane'; then
    echo "  dataplane symbols: present"
else
    echo "FAIL: no dataplane symbols found in no-alloc smoke ELF"
    FAILED=1
fi

if [ "$FAILED" -ne 0 ]; then
    echo ""
    echo "RP2040 no-alloc smoke contract guard FAILED."
    exit 1
fi

echo ""
echo "RP2040 no-alloc smoke contract guard passed."
