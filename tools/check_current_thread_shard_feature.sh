#!/usr/bin/env bash
# Current-Thread Shard Feature Guard
# Checks that current-thread-shard-driver feature compiles without ESP32 toolchain
set -euo pipefail

cd "$(dirname "$0")/.."

echo "=== Current-Thread Shard Feature Guard ==="

FEATURE="current-thread-shard-driver"
MANIFEST="applications/erlang/ranch_uring/native/Cargo.toml"

echo "Checking: cargo check with $FEATURE feature..."
if cargo check --manifest-path "$MANIFEST" --lib --features "$FEATURE" 2>&1; then
    echo "  $FEATURE compiles: ok"
else
    echo "FAIL: $FEATURE does not compile"
    exit 1
fi

echo ""
echo "Current-thread feature guard passed."
