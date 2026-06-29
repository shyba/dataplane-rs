#!/usr/bin/env bash
# Static guard: fails when Makefile runtime-boundary omits an executable guard under tools/check_runtime_*.sh
# DP-NB-0006
set -euo pipefail

cd "$(dirname "$0")/.."

echo "=== Runtime-Boundary Makefile Completeness Guard ==="

FAILED=0

# All check_runtime_*.sh scripts that should be invoked by runtime-boundary target
REQUIRED_RUNTIME_CHECKS="check_runtime_table_boundaries.sh check_runtime_extraction_wiring.sh check_runtime_api_async_guards.sh"

MAKEFILE="Makefile"
RUNTIME_BOUNDARY_TARGET="runtime-boundary:"

# Check each required script is called in the runtime-boundary target
for check_script in $REQUIRED_RUNTIME_CHECKS; do
    if ! grep -q "$RUNTIME_BOUNDARY_TARGET" "$MAKEFILE" 2>/dev/null; then
        echo "FAIL: Makefile does not define runtime-boundary target"
        exit 1
    fi
    
    # Extract the runtime-boundary target content and check if it includes the script
    # Using awk to get content between "runtime-boundary:" and next target line
    TARGET_CONTENT=$(awk "/^${RUNTIME_BOUNDARY_TARGET//:/:\\s*/:/}/,/^[a-zA-Z0-9_-]+:/ {if (/^[a-zA-Z0-9_-]+:/) exit; print}" "$MAKEFILE")
    
    if ! echo "$TARGET_CONTENT" | grep -q "$check_script"; then
        echo "FAIL: Makefile runtime-boundary target does not invoke $check_script"
        FAILED=1
    else
        echo "  $check_script: included in runtime-boundary target: ok"
    fi
done

if [ $FAILED -eq 1 ]; then
    echo ""
    echo "RUNTIME-BOUNDARY MAKEFILE GUARD FAILED"
    exit 1
fi

echo ""
echo "Runtime-boundary Makefile completeness guard passed."
exit 0
