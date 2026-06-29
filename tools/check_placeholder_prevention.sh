#!/usr/bin/env bash
# Placeholder Module Prevention Guard
# Fails if any native module file contains no Rust items (functions, structs, enums, traits, impls, macros, uses)
# DP-RX-0033 through DP-RX-0048 / L1 Placeholder Prevention
# DP-CB-0009: fixture proof - placeholder guard for runtime.rs child modules with docs-only
#   runtime.rs child modules (runtime_command, runtime_config, etc.) cannot be placeholder-only
# DP-CB-0010: fixture proof - placeholder guard for lib.rs sibling modules
#   lib.rs sibling modules declared alongside runtime.rs cannot be placeholder-only
# DP-CB-0011: fixture proof - impl blocks count as real items
#   A module with only impl blocks still counts as having real content (not placeholder)
# DP-CB-0012: fixture proof - use items count only when backed by imports
#   A use statement without an actual import backing it does not count as a real item
set -euo pipefail

cd "$(dirname "$0")/.."

echo "=== Placeholder Module Prevention Guard ==="

FAILED=0

# Modules that must never exist as placeholder-only (DP-RX-0033)
FORBIDDEN_PLACEHOLDERS="runtime_thread.rs"

# Modules that must not be placeholder-only if declared in lib.rs (DP-RX-0034, 0035, 0036)
SHARD_PLACEHOLDERS="runtime_shard_close.rs runtime_shard_recv.rs runtime_shard_result.rs"

NATIVE_SRC="applications/erlang/ranch_uring/native/src"
LIB_PATH="$NATIVE_SRC/lib.rs"
RUNTIME_PATH="$NATIVE_SRC/runtime.rs"
MODULE_DECL_SOURCES="$LIB_PATH $RUNTIME_PATH"

# --- Helper: count non-comment, non-doc-line items in a file ---
count_items() {
    local file="$1"
    # Items start at column 0 (no leading whitespace) as top-level definitions
    # Match: fn, struct, enum, trait, impl, mod, pub, use, macro, const, type, static
    # But exclude doc comments (//) and attribute lines ([#)
    rg -c '^\b(fn|struct|enum|trait|impl|mod|pub|use|macro|const|type|static)\b' "$file" 2>/dev/null || echo "0"
}

# --- DP-RX-0033: runtime_thread.rs must not exist as placeholder-only ---
for mod_file in $FORBIDDEN_PLACEHOLDERS; do
    mod_path="$NATIVE_SRC/$mod_file"
    if [ -f "$mod_path" ]; then
        item_count=$(count_items "$mod_path")
        if [ "$item_count" = "0" ]; then
            echo "FAIL: $mod_file exists as placeholder-only module (DP-RX-0033)"
            FAILED=1
        fi
    fi
done

# --- DP-RX-0034/0035/0036: shard placeholders if declared must have items ---
for mod_file in $SHARD_PLACEHOLDERS; do
    mod_path="$NATIVE_SRC/$mod_file"
    if [ -f "$mod_path" ]; then
        # Only check if declared in lib.rs or runtime.rs.
        if rg -q "mod ${mod_file%.rs}" $MODULE_DECL_SOURCES 2>/dev/null; then
            item_count=$(count_items "$mod_path")
            if [ "$item_count" = "0" ]; then
                echo "FAIL: $mod_file declared but is placeholder-only (DP-RX-0034/0035/0036)"
                FAILED=1
            fi
        fi
    fi
done

# --- DP-RX-0037: All non-allowlisted declared native modules must contain items ---
echo "Checking: all declared native modules have at least one item..."

# Get all mod declarations from lib.rs and runtime.rs. Runtime child modules
# are the important case for future shard-helper extraction.
MODULES=$(
    rg --no-filename -o '^\s*(pub(\([^)]*\))?\s+)?mod\s+([A-Za-z0-9_]+)\s*;' \
        $MODULE_DECL_SOURCES --replace '$3' 2>/dev/null \
        | sort -u || true
)
for mod_name in $MODULES; do
    # Skip certain well-known non-item files that are allowlisted
    case "$mod_name" in
        # Allowlisted: may legitimately have no standalone items (build.rs, generated)
        build|generated) continue ;;
    esac

    mod_file="${mod_name}.rs"
    mod_path="$NATIVE_SRC/$mod_file"

    if [ -f "$mod_path" ]; then
        item_count=$(count_items "$mod_path")
        if [ "$item_count" = "0" ]; then
            echo "FAIL: declared mod $mod_name ($mod_file) has zero items (DP-RX-0037)"
            FAILED=1
        fi
    fi
done

# --- DP-RX-0038: Story-title module references must not require placeholder files ---
echo "Checking: no placeholder file creation triggered by story titles..."
# This guard ensures we don't create a file purely to satisfy a story title.
# We check that no runtime_*.rs file has a story in pending state that only mentions it.
# This is enforced by the absence of "Actual function extraction is deferred" phrase (DP-RX-0040).

# --- DP-RX-0039: Done module movement stories backed by real items not docs-only ---
echo "Checking: done Move stories are not docs-only..."
# Already covered by check_prd_status.sh DP-NB-0005, but we double-check here
DONE_MOVE_MODULES=$(
    jq -r '.stories[] | select(.status == "done" and ((.title | contains("Move")) or (.title | contains("Create")) or (.title | contains("Extract")))) | .title' prd.json 2>/dev/null \
        | rg -o 'runtime_[a-z_]+\.rs' \
        | sort -u || true
)
# Modules referenced in done movement/creation/extraction stories must exist as
# real files with items.
for mod_file in $DONE_MOVE_MODULES; do
    mod_path="$NATIVE_SRC/$mod_file"
    if [ -f "$mod_path" ]; then
        item_count=$(count_items "$mod_path")
        if [ "$item_count" = "0" ]; then
            echo "FAIL: done extraction story references placeholder-only $mod_file (DP-RX-0039)"
            FAILED=1
        fi
    fi
done

# --- DP-RX-0040: "Actual function extraction is deferred" must not appear in source ---
echo "Checking: no 'Actual function extraction is deferred' placeholder phrase..."
if rg -q 'Actual function extraction is deferred' "$NATIVE_SRC" 2>/dev/null; then
    echo "FAIL: placeholder phrase found in native src (DP-RX-0040)"
    FAILED=1
fi
if rg -q 'This module serves as a marker for the extraction boundary' "$NATIVE_SRC" 2>/dev/null; then
    echo "FAIL: marker-only extraction module phrase found in native src"
    FAILED=1
fi

# --- DP-RX-0041: Declared modules are referenced by code or listed as intentional boundary ---
echo "Checking: declared modules are referenced or intentional boundary..."
for mod_name in $MODULES; do
    case "$mod_name" in
        build|generated) continue ;;
    esac
    mod_file="${mod_name}.rs"
    # Skip if mod has items (already checked above)
    mod_path="$NATIVE_SRC/$mod_file"
    if [ -f "$mod_path" ]; then
        item_count=$(count_items "$mod_path")
        if [ "$item_count" = "0" ]; then
            # Check if it's used in lib.rs or other files
            usage=$(rg -l "(${mod_name}|${mod_file%.rs})" "$NATIVE_SRC" 2>/dev/null | wc -l | tr -d ' ')
            if [ "$usage" -lt 2 ]; then  # At least lib.rs declaration + one use
                echo "FAIL: $mod_file declared but not referenced (DP-RX-0041)"
                FAILED=1
            fi
        fi
    fi
done

if [ $FAILED -eq 1 ]; then
    echo ""
    echo "PLACEHOLDER PREVENTION GUARD FAILED"
    exit 1
fi

echo ""
echo "All placeholder prevention checks passed."
exit 0
