#!/bin/bash
# Plan-stack contradiction check for PRD execution
# Fails only if active claims contradict current code state

set -e

cd "$(dirname "$0")/.."

echo "=== Plan-Stack Contradiction Guard ==="

# Stale phrase: "no dataplane-nif crate" or "does not exist dataplane-nif"
# as an active/current claim (not historical reference)
if rg -q "no dataplane-nif crate|does not exist dataplane-nif|dataplane-nif does not exist" PLAN.*.md aidocs/*.md 2>/dev/null; then
    echo "FAIL: stale dataplane-nif non-existence claim found in plan docs"
    rg -l "no dataplane-nif crate|does not exist dataplane-nif|dataplane-nif does not exist" PLAN.*.md aidocs/*.md 2>/dev/null
    exit 1
fi
echo "Checking: no stale dataplane-nif non-existence claims... ok"

# Verify dataplane-nif crate exists
if [ ! -d "crates/dataplane-nif/src" ]; then
    echo "FAIL: crates/dataplane-nif/src does not exist"
    exit 1
fi
echo "Checking: crates/dataplane-nif exists... ok"

# Verify runtime_adapter is in dataplane-nif
if ! rg -q "pub mod runtime_adapter" crates/dataplane-nif/src/lib.rs 2>/dev/null; then
    echo "FAIL: runtime_adapter not found in dataplane-nif lib.rs"
    exit 1
fi
echo "Checking: runtime_adapter is in dataplane-nif... ok"

# Verify runtime_adapter is NOT directly in native crate (only re-exported)
if rg -q "^mod runtime_adapter|^pub mod runtime_adapter" applications/erlang/ranch_uring/native/src/ 2>/dev/null; then
    echo "FAIL: runtime_adapter found as direct module in native crate (should be re-exported only)"
    exit 1
fi
echo "Checking: runtime_adapter not directly in native src/... ok"

# Verify runtime table boundary guard exists
if [ ! -f "tools/check_runtime_table_boundaries.sh" ]; then
    echo "FAIL: tools/check_runtime_table_boundaries.sh not found"
    exit 1
fi
echo "Checking: runtime table boundary guard exists... ok"

echo "All plan-stack contradiction checks passed."
