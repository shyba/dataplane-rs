#!/usr/bin/env bash
# RP2040 No-Alloc Static Guard
# Skips until the canonical no-alloc path exists or an explicit override is
# provided, then rejects heap-backed imports and usage patterns in the declared
# no-alloc surface.
set -euo pipefail

if [ -n "${BASH_SOURCE[0]:-}" ]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
else
    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
fi
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

echo "=== RP2040 No-Alloc Static Guard ==="

DEFAULT_PATH="crates/dataplane-core-reactor/src/noalloc_primitives.rs"
CONFIGURED_PATHS="${RP2040_NOALLOC_STATIC_PATHS:-${RP2040_NOALLOC_PATH:-}}"
if [ -z "$CONFIGURED_PATHS" ]; then
    if [ -e "$DEFAULT_PATH" ]; then
        CONFIGURED_PATHS="$DEFAULT_PATH"
    else
        echo "skipped: no configured RP2040 no-alloc path"
        exit 0
    fi
fi

FAILED=0
FOUND=0
CONFIGURED=0

scan_file() {
    local path="$1"
    if [ ! -e "$path" ]; then
        echo "FAIL: configured path missing: $path"
        FAILED=1
        return
    fi
    if [ ! -f "$path" ]; then
        echo "FAIL: configured path is not a regular file: $path"
        FAILED=1
        return
    fi

    FOUND=1
    echo "checking: $path"

    check_pattern() {
        local label="$1"
        local pattern="$2"
        set +e
        rg -n -F -- "$pattern" "$path" >/dev/null 2>&1
        local status=$?
        set -e
        case "$status" in
            0)
                echo "FAIL: $label found in $path"
                FAILED=1
                ;;
            1)
                ;;
            *)
                echo "FAIL: unable to scan $path for $label (rg exit $status)"
                FAILED=1
                ;;
        esac
    }

    check_pattern "alloc import or path" "alloc"
    check_pattern "Vec" "Vec"
    check_pattern "VecDeque" "VecDeque"
    check_pattern "Box" "Box"
    check_pattern "Arc" "Arc"
    check_pattern "String" "String"
    check_pattern "format!" "format!"
    check_pattern "ToString" "ToString"
    check_pattern "heap-backed channel" "std::sync::mpsc"
    check_pattern "heap-backed channel" "crossbeam_channel"
    check_pattern "heap-backed channel" "async_channel"
    check_pattern "heap-backed channel" "futures::channel"
    check_pattern "heap-backed channel" "oneshot"
}

IFS=':' read -r -a path_list <<<"$CONFIGURED_PATHS"
for path in "${path_list[@]}"; do
    [ -n "$path" ] || continue
    CONFIGURED=1
    if [ -d "$path" ]; then
        DIR_FOUND=0
        while IFS= read -r file; do
            DIR_FOUND=1
            scan_file "$file"
        done < <(rg --files "$path" -g '*.rs')
        if [ "$DIR_FOUND" -eq 0 ]; then
            echo "FAIL: configured directory has no Rust files: $path"
            FAILED=1
        fi
        continue
    fi
    scan_file "$path"
done

if [ "$CONFIGURED" -eq 0 ]; then
    echo "FAIL: no usable configured path in RP2040 no-alloc path list"
    exit 1
fi

if [ "$FAILED" -ne 0 ]; then
    echo ""
    echo "RP2040 no-alloc static guard FAILED."
    exit 1
fi

echo ""
echo "RP2040 no-alloc static guard passed."
exit 0
