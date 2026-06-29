#!/usr/bin/env bash
# check_io_uring_available.sh - Detect io_uring availability and EPERM
# L5 story: DP-OVN-0122, DP-OVN-0132
set -euo pipefail

REPO="$(cd "$(dirname "$0")/.." && pwd)"

echo "=== io_uring Availability Check ==="

# Try the local_exec_uring_bench binary (uses IoUring::new which will fail with EPERM
# if io_uring is not available or if permissions are insufficient)
OUTPUT=$("$REPO/target/release/local_exec_uring_bench" 2>&1) || true

if echo "$OUTPUT" | grep -qi "Operation not permitted"; then
    echo "RESULT: EPERM detected - io_uring unavailable or insufficient permissions"
    echo "DETAILS: $OUTPUT"
    exit 0  # EPERM is a valid/expected outcome
elif echo "$OUTPUT" | grep -qi "create io_uring"; then
    # Check if the error contains EPERM
    if echo "$OUTPUT" | grep -qiE "EPERM|permission|Operation not permitted"; then
        echo "RESULT: EPERM detected - io_uring unavailable or insufficient permissions"
        echo "DETAILS: $OUTPUT"
        exit 0
    fi
fi

# Check if it ran successfully
if echo "$OUTPUT" | grep -q "local_exec_uring_bench"; then
    echo "RESULT: SUCCESS - io_uring is available"
    echo "DETAILS: $OUTPUT"
    exit 0
fi

# Fallback: try a quick cargo run to probe io_uring
echo "Fallback: trying cargo run probe..."
PROBE_OUTPUT=$(cd "$REPO" && cargo run --manifest-path applications/erlang/ranch_uring/native/Cargo.toml --bin local_exec_uring_bench --release 2>&1) || true

if echo "$PROBE_OUTPUT" | grep -qiE "EPERM|permission|Operation not permitted"; then
    echo "RESULT: EPERM detected - io_uring unavailable or insufficient permissions"
    exit 0
elif echo "$PROBE_OUTPUT" | grep -q "local_exec_uring_bench"; then
    echo "RESULT: SUCCESS - io_uring is available"
    exit 0
fi

echo "RESULT: UNKNOWN - could not determine io_uring availability"
echo "OUTPUT: $PROBE_OUTPUT"
exit 1
