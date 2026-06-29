#!/usr/bin/env bash
# Native Feature Matrix Guard
# Runs cargo check (and optionally clippy) for default, current-thread, and SQPOLL features
set -euo pipefail

cd "$(dirname "$0")/.."

MODE="${1:-check}"  # "check" or "clippy"

echo "=== Native Feature Matrix Guard (mode: $MODE) ==="

MANIFEST="applications/erlang/ranch_uring/native/Cargo.toml"
FAILED=0

# 1. Default feature set
echo "[1/3] Checking default feature set..."
if [ "$MODE" = "clippy" ]; then
    if cargo clippy --manifest-path "$MANIFEST" --lib -- -D warnings 2>&1; then
        echo "  default clippy: ok"
    else
        echo "  default clippy: FAIL"
        FAILED=1
    fi
else
    if cargo check --manifest-path "$MANIFEST" --lib 2>&1; then
        echo "  default check: ok"
    else
        echo "  default check: FAIL"
        FAILED=1
    fi
fi

# 2. current-thread-shard-driver
echo "[2/3] Checking current-thread-shard-driver feature..."
if [ "$MODE" = "clippy" ]; then
    if cargo clippy --manifest-path "$MANIFEST" --lib --features current-thread-shard-driver -- -D warnings 2>&1; then
        echo "  current-thread clippy: ok"
    else
        echo "  current-thread clippy: FAIL"
        FAILED=1
    fi
else
    if cargo check --manifest-path "$MANIFEST" --lib --features current-thread-shard-driver 2>&1; then
        echo "  current-thread check: ok"
    else
        echo "  current-thread check: FAIL"
        FAILED=1
    fi
fi

# 3. exec-strategy-sqpoll
echo "[3/3] Checking exec-strategy-sqpoll feature..."
if [ "$MODE" = "clippy" ]; then
    if cargo clippy --manifest-path "$MANIFEST" --lib --features exec-strategy-sqpoll -- -D warnings 2>&1; then
        echo "  SQPOLL clippy: ok"
    else
        echo "  SQPOLL clippy: FAIL"
        FAILED=1
    fi
else
    if cargo check --manifest-path "$MANIFEST" --lib --features exec-strategy-sqpoll 2>&1; then
        echo "  SQPOLL check: ok"
    else
        echo "  SQPOLL check: FAIL"
        FAILED=1
    fi
fi

echo ""
if [ "$FAILED" -eq 0 ]; then
    echo "Feature matrix guard passed."
    exit 0
else
    echo "Feature matrix guard FAILED."
    exit 1
fi
