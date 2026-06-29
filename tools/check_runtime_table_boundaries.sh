#!/usr/bin/env bash
# Runtime table boundary guard — prevents regression of table encapsulation
# and runtime_adapter extraction boundary.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
NATIVE_SRC="$REPO_ROOT/applications/erlang/ranch_uring/native/src"
NIF_CRATE="$REPO_ROOT/crates/dataplane-nif/src"
FAILED=0

echo "=== Runtime Table Boundary Guard ==="

# DP-NIF-0062: Guard against Deref reintroduction on runtime tables
echo -n "Checking for Deref on ConnectionTable/ListenerTable... "
if grep -rq "impl.*Deref.*for.*\(ConnectionTable\|ListenerTable\)" "$NATIVE_SRC/runtime_connection_table.rs" "$NATIVE_SRC/runtime_listener_table.rs" 2>/dev/null; then
    echo "FAIL: Deref found on runtime table"
    FAILED=1
else
    echo "ok"
fi

# DP-NIF-0063: Guard against contains_key in production code
echo -n "Checking for contains_key production access... "
CONTAINS_KEY_FILES=$(grep -rl "contains_key" "$NATIVE_SRC" \
    --include="*.rs" \
    -d skip \
    2>/dev/null \
    | grep -v "_test\.rs$" | grep -v "runtime_connection_table\.rs$" | grep -v "runtime_listener_table\.rs$" \
    || true)
if [ -n "$CONTAINS_KEY_FILES" ]; then
    echo "FAIL: contains_key found in:"
    echo "$CONTAINS_KEY_FILES"
    FAILED=1
else
    echo "ok"
fi

# DP-NIF-0064: Guard against backing-map .inner access outside table modules
echo -n "Checking for .inner access outside table modules... "
INNER_ACCESS=$(grep -rn "\.inner" "$NATIVE_SRC" \
    --include="*.rs" \
    -d skip \
    2>/dev/null \
    | grep -v "runtime_connection_table\.rs$" | grep -v "runtime_listener_table\.rs$" \
    | grep -v "_test\.rs$" \
    || true)
if [ -n "$INNER_ACCESS" ]; then
    echo "FAIL: .inner access found outside table modules:"
    echo "$INNER_ACCESS"
    FAILED=1
else
    echo "ok"
fi

# DP-NIF-0065: Guard runtime_adapter extraction boundary
echo -n "Checking runtime_adapter is NOT in native crate... "
if [ -e "$NATIVE_SRC/runtime_adapter.rs" ]; then
    echo "FAIL: native/src/runtime_adapter.rs still exists"
    FAILED=1
else
    echo "ok"
fi

echo -n "Checking runtime_adapter IS in dataplane-nif... "
if [ -e "$NIF_CRATE/runtime_adapter.rs" ]; then
    echo "ok"
else
    echo "FAIL: crates/dataplane-nif/src/runtime_adapter.rs does not exist"
    FAILED=1
fi

if [ $FAILED -eq 1 ]; then
    echo ""
    echo "RUNTIME BOUNDARY GUARD FAILED"
    exit 1
fi

echo ""
echo "All runtime boundary guards passed."
exit 0
