#!/usr/bin/env bash
# Static guard: fails when untracked generated/source files are referenced in PRD completion notes
# DP-NB-0005
set -euo pipefail

cd "$(dirname "$0")/.."

echo "=== Untracked Generated Module Guard ==="

FAILED=0

# Known generated module patterns that should be tracked in git
GENERATED_PATTERNS="runtime_registration.rs runtime_startup.rs runtime_id_map.rs runtime_result_queue.rs runtime_pending_reply.rs"

# Resolve PRD note file references that are commonly written as basenames.
resolve_prd_path() {
    local ref="$1"
    case "$ref" in
        runtime_registration.rs|runtime_startup.rs|runtime_id_map.rs|runtime_result_queue.rs|runtime_pending_reply.rs|runtime_shard_close.rs|runtime_shard_recv.rs|runtime_command.rs|runtime_config.rs)
            echo "applications/erlang/ranch_uring/native/src/$ref"
            ;;
        embedded_host_loop_test_support.rs|embedded_host_loop.rs|runtime_profiles.rs)
            echo "crates/dataplane-runtime/src/$ref"
            ;;
        mailbox_future.rs|balanced_profile.rs)
            echo "crates/dataplane-core-reactor/src/$ref"
            ;;
        check_*.sh)
            echo "tools/$ref"
            ;;
        */*)
            echo "$ref"
            ;;
        *)
            echo ""
            ;;
    esac
}

# Check if any source or tool files referenced in prd.json notes are untracked.
if [ -f "prd.json" ]; then
    REFERENCED_FILES=$(jq -r '.. | objects | select(has("notes")) | .notes // empty | match("[A-Za-z0-9_./-]+\\.(rs|sh)"; "g") | .string' prd.json 2>/dev/null | sort -u || true)

    for ref in $REFERENCED_FILES; do
        file_path=$(resolve_prd_path "$ref")
        if [ -z "$file_path" ] || [ ! -f "$file_path" ]; then
            continue
        fi
        if ! git ls-files --error-unmatch "$file_path" >/dev/null 2>&1; then
            echo "FAIL: PRD-referenced file '$ref' exists but is not tracked in git at $file_path"
            FAILED=1
        fi
    done
fi

# Also check that generated modules in native/src are tracked
NATIVE_SRC="applications/erlang/ranch_uring/native/src"
for gen_mod in runtime_registration runtime_startup runtime_id_map runtime_result_queue runtime_pending_reply; do
    RS_FILE="$NATIVE_SRC/${gen_mod}.rs"
    if [ -f "$RS_FILE" ]; then
        if ! git ls-files --error-unmatch "$RS_FILE" >/dev/null 2>&1; then
            echo "FAIL: Generated module '${gen_mod}.rs' exists but is not tracked in git"
            FAILED=1
        fi
    fi
done

if [ $FAILED -eq 1 ]; then
    echo ""
    echo "UNTRACKED GENERATED MODULES GUARD FAILED"
    exit 1
fi

echo "All generated modules are properly tracked."
exit 0
