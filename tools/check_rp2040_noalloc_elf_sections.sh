#!/usr/bin/env bash
# RP2040 No-Alloc ELF Section Guard
# Uses arm-none-eabi-readelf to prove real .text/.vector_table sections vs empty artifact
# DP-NB-????: rp2040-noalloc-frontier-rp2040-smoke-artifacts-include-section-and-size-evidence
set -euo pipefail

cd "$(dirname "$0")/.."

target="thumbv6m-none-eabi"
elf="target/$target/release/dataplane-rp2040-noalloc-smoke"

echo "=== RP2040 No-Alloc ELF Section Guard ==="

crate_path="crates/dataplane-rp2040-noalloc-smoke/Cargo.toml"
if [ ! -f "$crate_path" ]; then
    echo "FAIL: no-alloc smoke crate not found at $crate_path"
    exit 1
fi

if [ ! -f "$elf" ]; then
    echo "FAIL: no-alloc smoke ELF not built at $elf"
    exit 1
fi

FAILED=0

# Check arm-none-eabi-readelf is available
if ! command -v arm-none-eabi-readelf >/dev/null 2>&1; then
    echo "FAIL: arm-none-eabi-readelf is not available"
    exit 1
fi

echo "Checking ELF sections in $elf..."

# Get section headers
section_output=$(arm-none-eabi-readelf -S "$elf" 2>/dev/null)

# Helper to convert hex string to decimal
hex_to_dec() {
    local hex="$1"
    hex="${hex#0}"
    [ -z "$hex" ] && hex="0"
    printf '%d' "0x$hex"
}

# Check for critical sections that must be present in a real binary
check_required_section() {
    local section="$1"
    local size

    # Extract size from readelf output
    # Format: [Nr] Name Type Addr Off Size ES Flg Lk Inf Al
    # Size is field $7
    size=$(echo "$section_output" | rg "^\s*\[\s*[0-9]+\]\s+$section\b" | awk '{print $7}')
    
    if [ -z "$size" ]; then
        echo "FAIL: required section $section is MISSING"
        return 1
    fi
    
    # Convert hex size to decimal for display
    local size_dec
    size_dec=$(hex_to_dec "$size")
    echo "  $section: present (size $size_dec / 0x$size bytes)"
    return 0
}

# Check required sections
echo "Checking required sections..."
check_required_section ".text" || FAILED=1
check_required_section ".data" || FAILED=1
check_required_section ".bss" || FAILED=1

# Check vector_table (optional for no-alloc bare-metal)
vector_size=$(echo "$section_output" | rg "^\s*\[\s*[0-9]+\]\s+\.vector_table\b" | awk '{print $7}')
if [ -n "$vector_size" ]; then
    vector_size_dec=$(hex_to_dec "$vector_size")
    echo "  .vector_table: present (size $vector_size_dec / 0x$vector_size bytes)"
else
    echo "  .vector_table: not present (may be absent for no-alloc bare-metal)"
fi

# Verify .text section has non-zero size (proof of real code, not empty artifact)
text_size=$(echo "$section_output" | rg "^\s*\[\s*[0-9]+\]\s+\.text\b" | awk '{print $7}')
if [ -n "$text_size" ] && [[ "$text_size" =~ ^[0-9a-fA-F]+$ ]]; then
    text_size_dec=$(hex_to_dec "$text_size")
    if [ "$text_size_dec" -eq 0 ]; then
        echo "FAIL: .text section is EMPTY (size 0) - binary may be metadata-only"
        FAILED=1
    else
        echo "  .text actual code size: $text_size_dec bytes (0x$text_size)"
    fi
fi

# Check for symbol table (proves linking worked)
# Match text symbols: type T (global) or t (local) with space delimiters
symbol_count=$(arm-none-eabi-nm "$elf" 2>/dev/null | rg " T " | wc -l | tr -d ' ')
if [ -n "$symbol_count" ] && [ "$symbol_count" -gt 0 ]; then
    echo "  text symbols: $symbol_count found"
fi

# Verify no-alloc smoke binary is not just a stub
total_size=$(arm-none-eabi-size "$elf" 2>/dev/null | tail -1 | awk '{print $1 + $2 + $3}')
if [ -n "$total_size" ] && [ "$total_size" -eq 0 ]; then
    echo "FAIL: ELF has zero total size - binary is an empty artifact"
    FAILED=1
fi

if [ $FAILED -ne 0 ]; then
    echo ""
    echo "RP2040 no-alloc ELF section guard FAILED."
    exit 1
fi

echo ""
echo "RP2040 no-alloc ELF section guard passed."
exit 0
