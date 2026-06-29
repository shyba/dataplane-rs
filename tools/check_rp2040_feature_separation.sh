#!/usr/bin/env bash
# RP2040 Feature Separation Guard
# Proves rp2040-noalloc does NOT enable rp2040-compile and vice versa
# DP-NB-????: rp2040-noalloc-frontier-rp2040-no-alloc-feature-boundary-stays-dependency-clean
set -euo pipefail

cd "$(dirname "$0")/.."

echo "=== RP2040 Feature Separation Guard ==="

FAILED=0

# Check dataplane-core-reactor Cargo.toml feature declarations
CORE_MANIFEST="crates/dataplane-core-reactor/Cargo.toml"
if [ -f "$CORE_MANIFEST" ]; then
    echo "Checking dataplane-core-reactor feature declarations..."

    # rp2040-noalloc should NOT enable rp2040-compile
    if rg -q 'rp2040-noalloc.*=.*\[.*rp2040-compile' "$CORE_MANIFEST" 2>/dev/null; then
        echo "FAIL: rp2040-noalloc enables rp2040-compile in $CORE_MANIFEST"
        FAILED=1
    fi

    # rp2040-compile should NOT enable rp2040-noalloc
    if rg -q 'rp2040-compile.*=.*\[.*rp2040-noalloc' "$CORE_MANIFEST" 2>/dev/null; then
        echo "FAIL: rp2040-compile enables rp2040-noalloc in $CORE_MANIFEST"
        FAILED=1
    fi

    # Verify both features are declared independently
    if ! rg -q '^\s*rp2040-noalloc\s*=' "$CORE_MANIFEST" 2>/dev/null; then
        echo "FAIL: rp2040-noalloc feature not declared in $CORE_MANIFEST"
        FAILED=1
    fi
    if ! rg -q '^\s*rp2040-compile\s*=' "$CORE_MANIFEST" 2>/dev/null; then
        echo "FAIL: rp2040-compile feature not declared in $CORE_MANIFEST"
        FAILED=1
    fi

    echo "  dataplane-core-reactor: features are separate"
fi

# Check dataplane-runtime Cargo.toml feature declarations
RUNTIME_MANIFEST="crates/dataplane-runtime/Cargo.toml"
if [ -f "$RUNTIME_MANIFEST" ]; then
    echo "Checking dataplane-runtime feature declarations..."

    # rp2040-noalloc should NOT enable rp2040-integration (which uses rp2040-compile)
    if rg -q 'rp2040-noalloc.*rp2040-integration' "$RUNTIME_MANIFEST" 2>/dev/null; then
        echo "FAIL: rp2040-noalloc enables rp2040-integration in $RUNTIME_MANIFEST"
        FAILED=1
    fi

    # rp2040-integration should NOT enable rp2040-noalloc
    if rg -q 'rp2040-integration.*rp2040-noalloc' "$RUNTIME_MANIFEST" 2>/dev/null; then
        echo "FAIL: rp2040-integration enables rp2040-noalloc in $RUNTIME_MANIFEST"
        FAILED=1
    fi

    echo "  dataplane-runtime: features are separate"
fi

# Verify compile-fail behavior: rp2040-noalloc cannot see rp2040-compile symbols
echo "Verifying no-alloc path cannot see alloc path..."
target="thumbv6m-none-eabi"

# Try to compile dataplane-core-reactor with rp2040-noalloc only (no default features)
# If it accidentally enables rp2040-compile, we might see symbols from the alloc path
core_check=$(cargo metadata -p dataplane-core-reactor --no-default-features --features rp2040-noalloc --format-version 1 2>/dev/null | \
    rg '"name":"dataplane-core-reactor-alloc"' || echo "NOT_FOUND")

if [ "$core_check" != "NOT_FOUND" ]; then
    # Check if dataplane-core-reactor-alloc is actually used in the rp2040-noalloc graph
    alloc_used=$(cargo tree -p dataplane-core-reactor --no-default-features --features rp2040-noalloc --target "$target" -e normal 2>/dev/null | \
        rg 'dataplane-core-reactor-alloc' || echo "NOT_USED")
    if [ "$alloc_used" != "NOT_USED" ]; then
        echo "FAIL: rp2040-noalloc feature pulls in dataplane-core-reactor-alloc"
        FAILED=1
    fi
fi

if [ $FAILED -ne 0 ]; then
    echo ""
    echo "RP2040 feature separation guard FAILED."
    exit 1
fi

echo ""
echo "RP2040 feature separation guard passed."
exit 0