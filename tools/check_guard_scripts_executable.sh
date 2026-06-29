#!/usr/bin/env bash
# Static guard: fails when any ./tools/*.sh script invoked by Makefile lacks
# an executable bit. This intentionally covers smoke and perf scripts, not just
# tools/check_*.sh guards.
# DP-NB-0007
set -euo pipefail

cd "$(dirname "$0")/.."

echo "=== Guard Scripts Executable Bit Guard ==="

FAILED=0

mapfile -t MAKEFILE_SCRIPTS < <(rg -o '\./tools/[A-Za-z0-9_./-]+\.sh' Makefile | sort -u)

if [ "${#MAKEFILE_SCRIPTS[@]}" -eq 0 ]; then
    echo "FAIL: no ./tools/*.sh Makefile invocations found; guard cannot prove coverage"
    exit 1
fi

echo "Checking Makefile-invoked scripts..."
for script_ref in "${MAKEFILE_SCRIPTS[@]}"; do
    script="${script_ref#./}"
    if [ ! -f "$script" ]; then
        echo "FAIL: $script_ref is invoked by Makefile but does not exist"
        FAILED=1
        continue
    fi
    if [ ! -x "$script" ]; then
        echo "FAIL: $script is not executable"
        FAILED=1
    else
        echo "  $script: executable: ok"
    fi
done

echo ""
echo "Checking all tools/check_*.sh scripts..."
while IFS= read -r script; do
    [ -z "$script" ] && continue
    if [ ! -x "$script" ]; then
        echo "FAIL: $script is not executable"
        FAILED=1
    else
        echo "  $script: executable: ok"
    fi
done < <(find tools -maxdepth 1 -type f -name 'check_*.sh' | sort)

if [ $FAILED -eq 1 ]; then
    echo ""
    echo "EXECUTABLE BIT GUARD FAILED"
    exit 1
fi

echo ""
echo "All Makefile-invoked scripts and check guards have executable bit set."
exit 0
