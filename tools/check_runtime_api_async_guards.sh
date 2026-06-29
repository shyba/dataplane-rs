#!/usr/bin/env bash
# Runtime API Async Guards
# Static guards for async request/reply path correctness
set -euo pipefail

# Determine repo root robustly: BASH_SOURCE[0] works when run as script,
# falls back to $0 when sourced. Parent of tools/ is the repo root.
if [ -n "${BASH_SOURCE[0]:-}" ]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
else
    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
fi
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

echo "=== Runtime API Async Guards ==="

FAILED=0
RUNTIME_API="applications/erlang/ranch_uring/native/src/runtime_api.rs"
RUNTIME="applications/erlang/ranch_uring/native/src/runtime.rs"
RUNTIME_SHARD="applications/erlang/ranch_uring/native/src/runtime_shard.rs"
RUNTIME_COMMAND="applications/erlang/ranch_uring/native/src/runtime_command.rs"

require_pattern() {
    local label="$1"
    local pattern="$2"
    local path="$3"
    # Temporarily disable set -e to capture rg exit code without early exit
    # rg -q exits 0 on match, 1 on no match, 2 on error
    local rg_exit
    set +e
    rg -q "$pattern" "$path" > /dev/null 2>&1
    rg_exit=$?
    set -e
    if [ "$rg_exit" -eq 0 ]; then
        echo "  $label: ok"
    elif [ "$rg_exit" -eq 1 ]; then
        echo "FAIL: $label"
        FAILED=1
    else
        echo "FAIL: $label (rg error $rg_exit)"
        FAILED=1
    fi
}

# Guard: close_async repeated close path
# The guard verifies that repeated close_async on a session either:
# - Returns error before exposing request_id, OR
# - Enqueues Command::CloseAsync for proper reply
echo "Checking: close_async repeated close behavior..."
require_pattern "close_async exists" "pub fn close_async\\b" "$RUNTIME_API"
require_pattern "close_async enqueues terminal CloseAsync command" "Command::CloseAsync" "$RUNTIME_API"
require_pattern "CloseAsync carries request_id" "CloseAsync \\{" "$RUNTIME_COMMAND"
require_pattern "CloseAsync carries target" "target: ResultTarget" "$RUNTIME_COMMAND"
require_pattern "CloseAsync handler always queues reply" "queue_reply_ok\\(target, request_id\\)" "$RUNTIME_SHARD"

# Guard: routed request ids use nonzero shard namespace
echo "Checking: next_request_id_for_shard uses nonzero namespace..."
require_pattern "next_request_id_for_shard exists" "fn next_request_id_for_shard\\b" "$RUNTIME"
require_pattern "request ids use shard-plus-one high byte" "\\(\\(\\(shard \\+ 1\\) as u64\\) << 56\\)" "$RUNTIME"
require_pattern "zero-namespace routed ids rejected by test" "shard_from_routed_id\\(7, 4\\)\\.is_err\\(\\)" "$RUNTIME"

# Guard: owner_down routes through runtime owner cleanup
echo "Checking: owner_down routes through runtime cleanup..."
require_pattern "owner_down API exists" "pub\\(crate\\) fn owner_down\\b" "$RUNTIME_API"
require_pattern "owner_down enqueues OwnerDown" "Command::OwnerDown" "$RUNTIME_API"
require_pattern "OwnerDown command exists" "OwnerDown \\{" "$RUNTIME_COMMAND"
require_pattern "OwnerDown handler closes connection" "Command::OwnerDown \\{ session_id \\}" "$RUNTIME_SHARD"
require_pattern "OwnerDown uses begin_close_connection" "begin_close_connection\\(session_id, NifError::Closed\\)" "$RUNTIME_SHARD"

# Guard: runtime_api import hygiene stays reviewable after extraction.
echo "Checking: runtime_api.rs import hygiene..."
set +e
rg -q '^\s*use\s+super::\*' "$RUNTIME_API" > /dev/null 2>&1
RG_EXIT=$?
set -e
if [ "$RG_EXIT" -eq 0 ]; then
    echo "FAIL: runtime_api.rs must not use super::*"
    FAILED=1
elif [ "$RG_EXIT" -eq 1 ]; then
    echo "  runtime_api no wildcard: ok"
else
    echo "FAIL: runtime_api wildcard check rg error $RG_EXIT"
    FAILED=1
fi

DP_CS_0228_COUNT=$(rg -c '^// DP-CS-0228:' "$RUNTIME_API" 2>/dev/null || echo "0")
if [ "$DP_CS_0228_COUNT" = "1" ]; then
    echo "  runtime_api single DP-CS-0228 comment: ok"
else
    echo "FAIL: runtime_api has $DP_CS_0228_COUNT DP-CS-0228 comments"
    FAILED=1
fi

set +e
rg -q '^#!\[allow\(unused_imports\)\]' "$RUNTIME_API" > /dev/null 2>&1
RG_EXIT=$?
set -e
if [ "$RG_EXIT" -eq 0 ]; then
    echo "FAIL: runtime_api.rs must not use module-level allow(unused_imports)"
    FAILED=1
elif [ "$RG_EXIT" -eq 1 ]; then
    echo "  runtime_api no module-level allow(unused_imports): ok"
else
    echo "FAIL: runtime_api allow check rg error $RG_EXIT"
    FAILED=1
fi

# Guard: stat_async / set_active_async etc. carry request_id and target
echo "Checking: async operations carry request_id and target..."
for VARIANT in RecvAsync SendAsync BatchAsync SetActiveAsync SetNoDelayAsync ShutdownAsync StatAsync NoopAsync; do
    require_pattern "$VARIANT command is emitted" "Command::$VARIANT" "$RUNTIME_API"
done
require_pattern "async commands target Erlang reply pid" "ResultTarget::Erlang\\(reply_pid\\)" "$RUNTIME_API"
require_pattern "accept_async sends terminal session result" "send_session_result\\(&reply_pid, request_id, result\\)" "$RUNTIME_API"
require_pattern "close_async listener path sends terminal unit result" "send_unit_result\\(&reply_pid, request_id, result\\)" "$RUNTIME_API"

echo ""
if [ "$FAILED" -eq 0 ]; then
    echo "Runtime API async guards passed."
    exit 0
else
    echo "Runtime API async guards FAILED."
    exit 1
fi
