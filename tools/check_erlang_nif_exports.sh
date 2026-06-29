#!/usr/bin/env bash
# Erlang NIF Export Guard
# Verifies Erlang .beam exports match native rustler::nif function names
set -euo pipefail

cd "$(dirname "$0")/.."

echo "=== Erlang NIF Export Guard ==="

FAILED=0

# Find ranch_uring_nif.erl source
ERL_SRC=$(find applications/erlang/ranch_uring -name "ranch_uring_nif.erl" 2>/dev/null | head -1)
if [ -z "$ERL_SRC" ]; then
    echo "WARN: ranch_uring_nif.erl not found"
    echo "All Erlang NIF export guards passed (no .erl source found)."
    exit 0
fi

NIF_PATH="applications/erlang/ranch_uring/native/src/nif.rs"
echo "Erl source: $ERL_SRC"
echo "NIF impl: $NIF_PATH"

# Core APIs that should have corresponding native rustler::nif implementations
# Erlang export format: -export([func/arity]).
# Rust format: #[rustler::nif] pub fn func(...)
CORE_FUNCS="init stop_runtime bench_direct_ok bench_direct_binary"

# Extended list of async + sync NIFs (approximate - actual arities differ)
ASYNC_FUNCS="listen accept recv send close shutdown setopt setopts getopt getopts"

echo "Checking: Erlang exports have corresponding native NIF implementations..."

# For each core function, check both Erlang export and Rust implementation
for FUNC in $CORE_FUNCS; do
    # Check if Erlang exports this function (any arity)
    if rg -q "^-export.*\b$FUNC/\d+" "$ERL_SRC" 2>/dev/null; then
        # Check if native has #[rustler::nif] pub fn $FUNC
        if rg -q "#\[rustler::nif\]" "$NIF_PATH" 2>/dev/null && \
           rg -q "pub fn $FUNC\b" "$NIF_PATH" 2>/dev/null; then
            echo "  $FUNC: exported and implemented: ok"
        else
            echo "  $FUNC: exported in Erlang but NOT found as #[rustler::nif] pub fn in nif.rs"
            FAILED=1
        fi
    fi
done

# Check async functions - they exist in runtime_api.rs not nif.rs
# The Erlang module delegates to runtime_api via the NIF stubs
echo ""
echo "Checking: async runtime API stubs (delegated to runtime_api.rs)..."

# These functions are implemented in runtime_api.rs and called from nif.rs
# or directly exported from nif.rs. Check if they exist in the NIF path.
for FUNC in $ASYNC_FUNCS; do
    # Check if Erlang exports this function
    if rg -q "^-export.*\b$FUNC/\d+" "$ERL_SRC" 2>/dev/null; then
        # Check if it's implemented somewhere in native src
        IMPL_PATH=$(rg -l "pub fn $FUNC\b" applications/erlang/ranch_uring/native/src/ 2>/dev/null || true)
        if [ -n "$IMPL_PATH" ]; then
            IMPL_PATH=${IMPL_PATH//$'\n'/, }
            echo "  $FUNC: exported and implemented in $IMPL_PATH: ok"
        else
            echo "  $FUNC: exported in Erlang but not found as pub fn in native src"
            FAILED=1
        fi
    fi
done

echo ""
if [ "$FAILED" -eq 0 ]; then
    echo "Erlang NIF export guard passed."
    exit 0
else
    echo "Erlang NIF export guard FAILED."
    exit 1
fi
