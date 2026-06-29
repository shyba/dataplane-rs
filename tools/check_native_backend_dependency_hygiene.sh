#!/usr/bin/env bash
# Native backend dependency hygiene guard.
#
# Deferred backend/server work must not re-enter the NIF crate through dormant
# feature flags, allowlists, or optional dependencies without same-patch source
# users and an explicit destination boundary.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

MANIFEST="applications/erlang/ranch_uring/native/Cargo.toml"
LOCKFILE="Cargo.lock"

echo "=== Native Backend Dependency Hygiene Guard ==="

if [ ! -f "$MANIFEST" ]; then
    echo "FAIL: native Cargo.toml not found at $MANIFEST"
    exit 1
fi

FAILED=0

echo -n "Checking native bench allowlist is absent... "
if [ -e "applications/erlang/ranch_uring/native/.bench_allowlist" ]; then
    echo "FAIL: native .bench_allowlist must not be recreated"
    FAILED=1
else
    echo "ok"
fi

FORBIDDEN_NATIVE_FEATURES="bench-monoio bench-hyper bench-async-engine tokio-adapter"
for feature in $FORBIDDEN_NATIVE_FEATURES; do
    echo -n "Checking native feature '$feature' is absent... "
    if rg -q "^[[:space:]]*$feature[[:space:]]=" "$MANIFEST"; then
        echo "FAIL: dormant native feature '$feature' is present"
        FAILED=1
    else
        echo "ok"
    fi
done

FORBIDDEN_NATIVE_DEPS="monoio hyper async-engine"
for dep in $FORBIDDEN_NATIVE_DEPS; do
    echo -n "Checking native dependency '$dep' is absent... "
    if rg -q "^[[:space:]]*$dep[[:space:]]=" "$MANIFEST" \
        || rg -q "^[[:space:]]*\\[(target\\.[^]]+\\.)?(build-)?dependencies\\.$dep\\]" "$MANIFEST" \
        || rg -q "package[[:space:]]*=[[:space:]]*\"$dep\"" "$MANIFEST"; then
        echo "FAIL: dormant native dependency '$dep' is present"
        FAILED=1
    else
        echo "ok"
    fi
done

echo -n "Checking crates/dataplane-server is absent... "
if [ -d "crates/dataplane-server" ]; then
    echo "FAIL: crates/dataplane-server exists without an active source-backed backend PRD"
    FAILED=1
else
    echo "ok"
fi

if [ -f "$LOCKFILE" ]; then
    for package in monoio hyper async-engine; do
        echo -n "Checking Cargo.lock package '$package' is absent... "
        if rg -q "^name = \"$package\"$" "$LOCKFILE"; then
            echo "FAIL: deferred backend package '$package' is present in Cargo.lock"
            FAILED=1
        else
            echo "ok"
        fi
    done
fi

if [ "$FAILED" -ne 0 ]; then
    echo ""
    echo "NATIVE BACKEND DEPENDENCY HYGIENE FAILED"
    exit 1
fi

echo ""
echo "Native backend dependency hygiene checks passed."
