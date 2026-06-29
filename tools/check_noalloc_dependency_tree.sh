#!/usr/bin/env bash
# No-alloc dependency-tree guard.
#
# This guard fails closed: the no-alloc target path must compile with a tiny
# normal/build dependency graph, and the smoke crate must exercise the runtime
# no-alloc surface instead of bypassing it.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"

cd "$REPO_DIR"

target="thumbv6m-none-eabi"
FAILED=0

echo "=== No-Alloc Dependency Tree Guard ==="

tree_for() {
    local package="$1"
    local features="$2"
    cargo tree -p "$package" \
        --no-default-features \
        --features "$features" \
        --target "$target" \
        -e normal,build \
        --prefix none
}

check_exact_tree() {
    local package="$1"
    local features="$2"
    shift 2

    local output
    local lines
    output="$(tree_for "$package" "$features")"
    lines="$(printf '%s\n' "$output" | sed '/^[[:space:]]*$/d')"

    local expected_count="$#"
    local actual_count
    actual_count="$(printf '%s\n' "$lines" | wc -l | tr -d ' ')"
    if [ "$actual_count" -ne "$expected_count" ]; then
        echo "FAIL: unexpected dependency count for $package/$features on $target" >&2
        echo "Expected $expected_count lines, got $actual_count:" >&2
        printf '%s\n' "$lines" >&2
        FAILED=1
        return
    fi

    for expected in "$@"; do
        if ! printf '%s\n' "$lines" | rg -q "^${expected} v"; then
            echo "FAIL: dependency tree for $package/$features missing: $expected v" >&2
            printf '%s\n' "$lines" >&2
            FAILED=1
        fi
    done

    if printf '%s\n' "$lines" | rg -q '^dataplane-core-reactor-alloc v'; then
        echo "FAIL: $package/$features pulls in dataplane-core-reactor-alloc on no-alloc target" >&2
        printf '%s\n' "$lines" >&2
        FAILED=1
    fi
}

echo "Checking core reactor no-alloc dependency tree..."
check_exact_tree dataplane-core-reactor noalloc dataplane-core-reactor
echo ""

echo "Checking runtime no-alloc dependency tree..."
check_exact_tree dataplane-runtime noalloc dataplane-runtime dataplane-core-reactor
echo ""

echo "Checking no-alloc smoke crate dependency..."
smoke_manifest="crates/dataplane-rp2040-noalloc-smoke/Cargo.toml"
smoke_source="crates/dataplane-rp2040-noalloc-smoke/src/main.rs"

if [ ! -f "$smoke_manifest" ]; then
    echo "FAIL: smoke manifest not found at $smoke_manifest"
    FAILED=1
else
    if rg -q 'dataplane-runtime.*noalloc' "$smoke_manifest"; then
        echo "  PASS: smoke crate depends on dataplane-runtime with noalloc"
    else
        echo "FAIL: smoke crate must depend on dataplane-runtime with noalloc"
        FAILED=1
    fi
    if rg -q 'dataplane-core-reactor.*rp2040-noalloc' "$smoke_manifest"; then
        echo "FAIL: smoke crate bypasses runtime by directly depending on dataplane-core-reactor rp2040-noalloc"
        FAILED=1
    fi
fi

if [ ! -f "$smoke_source" ]; then
    echo "FAIL: smoke source not found at $smoke_source"
    FAILED=1
elif rg -q 'dataplane_runtime::noalloc' "$smoke_source"; then
    echo "  PASS: smoke source imports dataplane_runtime::noalloc"
else
    echo "FAIL: smoke source must import dataplane_runtime::noalloc"
    FAILED=1
fi

echo ""
echo "Verifying no-alloc compile gates while dependency graph is active..."
cargo check -p dataplane-core-reactor \
    --no-default-features \
    --features noalloc \
    --target "$target" \
    --lib
cargo check -p dataplane-runtime \
    --no-default-features \
    --features noalloc \
    --target "$target" \
    --lib

echo ""
echo "Checking host-graph hygiene: noalloc-only build on Linux must not pull io-uring/libc/rustix..."
host_tree="$(cargo tree -p dataplane-core-reactor --no-default-features --features noalloc -e normal,build --prefix none 2>&1)"
for forbidden in io-uring libc rustix; do
    if printf '%s\n' "$host_tree" | rg -q "^${forbidden} v"; then
        echo "FAIL: host-graph for dataplane-core-reactor/noalloc contains $forbidden" >&2
        printf '%s\n' "$host_tree" >&2
        FAILED=1
    fi
done
echo "  host-graph hygiene: ok"

echo ""
echo "=== Result ==="
if [ "$FAILED" -eq 0 ]; then
    echo "PASS: no-alloc dependency constraints satisfied."
    exit 0
fi

echo "FAIL: no-alloc dependency constraints violated."
exit 1
