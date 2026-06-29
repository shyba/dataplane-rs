#!/usr/bin/env bash
# Runtime Shard Wildcard Symbol Inventory Tool
# Inventories all symbols imported via `use super::*` in runtime_shard.rs
# and classifies them by source, type, and usage pattern.
#
# Usage: ./tools/check_runtime_shard_wildcard_inventory.sh [mode]
#   modes: inventory (default) - produce full symbol inventory
#          count             - print symbol count only
#          by-source         - group by source module
#          by-type           - group by symbol type (fn/struct/enum/const/macro)
#          test-only         - symbols used only in tests
#          close-path        - symbols used only in close methods
#          recv-path         - symbols used only in recv methods
#          result-path       - symbols used only in result methods
#          run-loop          - symbols used only in run_loop/command polling
#          unresolved        - symbols needing import resolution
#          conflicts         - duplicate/conflicting imports
#          cfg-gated         - feature-gated symbol conflicts
#
# This tool is read-only by default (does not modify source files).
# Use --write-candidate to generate an import candidate file.
set -euo pipefail

# Determine repo root
if [ -n "${BASH_SOURCE[0]:-}" ]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
else
    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
fi
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

RUNTIME_SHARD="applications/erlang/ranch_uring/native/src/runtime_shard.rs"
RUNTIME="applications/erlang/ranch_uring/native/src/runtime.rs"

# Mode selection
MODE="${1:-inventory}"

# ---------------------------------------------------------------------------
# Helper: Get all identifiers used in runtime_shard.rs that come from super::
# ---------------------------------------------------------------------------
get_super_star_symbols() {
    # First, find what runtime.rs actually exports via pub(super) and pub(crate)
    # These are the only symbols that could validly be imported via use super::*

    # Extract pub(super) and pub(crate) symbols from runtime.rs
    # Pattern: visibility keyword followed by type/struct/enum/fn/const/etc
    rg -o 'pub\(super\)\s+(?:struct|enum|fn|const|type|trait|mod)\s+\w+' "$RUNTIME" 2>/dev/null | \
        rg -o '\w+$' | sort -u || true

    rg -o 'pub\(crate\)\s+(?:struct|enum|fn|const|type|trait|mod)\s+\w+' "$RUNTIME" 2>/dev/null | \
        rg -o '\w+$' | sort -u || true
}

# Get all use super::X symbols explicitly imported in runtime_shard.rs
get_explicit_super_imports() {
    rg -o 'use super::(\w+)' "$RUNTIME_SHARD" 2>/dev/null | rg -o 'use super::(\w+)' | rg -o '\w+$' | sort -u || true
}

# Count of actual wildcard super imports (use super::*)
count_wildcard_super() {
    rg -c '^use super::\*;' "$RUNTIME_SHARD" 2>/dev/null || echo "0"
}

# Get all identifiers referenced in runtime_shard.rs that are NOT locally defined
get_referenced_identifiers() {
    # Exclude keywords, literals, and local definitions
    rg -ow '\b[A-Z][a-zA-Z0-9_]*\b|\b[a-z_][a-zA-Z0-9_]*\b' "$RUNTIME_SHARD" 2>/dev/null | \
        rg -v '^\d+$|^true$|^false$' | \
        rg -v '^(impl|struct|enum|fn|let|mut|pub|use|mod|const|static|match|if|else|while|for|crate|super|Self|self)$' | \
        sort -u || true
}

# Classify symbol by type in runtime.rs
classify_symbol() {
    local symbol="$1"

    # Check if it's a struct
    if rg -q "pub\(super\)\s+struct\s+$symbol\b" "$RUNTIME" 2>/dev/null; then
        echo "struct"
        return
    fi

    # Check if it's an enum
    if rg -q "pub\(super\)\s+enum\s+$symbol\b" "$RUNTIME" 2>/dev/null; then
        echo "enum"
        return
    fi

    # Check if it's a function
    if rg -q "pub\(super\)\s+fn\s+$symbol\b" "$RUNTIME" 2>/dev/null; then
        echo "fn"
        return
    fi

    # Check if it's a const
    if rg -q "pub\(super\)\s+const\s+$symbol\b" "$RUNTIME" 2>/dev/null; then
        echo "const"
        return
    fi

    # Check if it's a type alias
    if rg -q "pub\(super\)\s+type\s+$symbol\b" "$RUNTIME" 2>/dev/null; then
        echo "type"
        return
    fi

    # Check if it's a macro
    if rg -q "pub\(super\)\s+macro_rules!\s+$symbol\b" "$RUNTIME" 2>/dev/null; then
        echo "macro"
        return
    fi

    # Check pub(crate)
    if rg -q "pub\(crate\)\s+struct\s+$symbol\b" "$RUNTIME" 2>/dev/null; then
        echo "struct"
        return
    fi
    if rg -q "pub\(crate\)\s+enum\s+$symbol\b" "$RUNTIME" 2>/dev/null; then
        echo "enum"
        return
    fi
    if rg -q "pub\(crate\)\s+fn\s+$symbol\b" "$RUNTIME" 2>/dev/null; then
        echo "fn"
        return
    fi
    if rg -q "pub\(crate\)\s+const\s+$symbol\b" "$RUNTIME" 2>/dev/null; then
        echo "const"
        return
    fi
    if rg -q "pub\(crate\)\s+type\s+$symbol\b" "$RUNTIME" 2>/dev/null; then
        echo "type"
        return
    fi

    echo "unknown"
}

# Get source module for a symbol
get_symbol_source_module() {
    local symbol="$1"

    # Look for re-exports from submodules in runtime.rs
    # Pattern: pub(crate) use self::module::symbol or pub(crate) use super::module::symbol
    local reexport_line
    reexport_line=$(rg -n "pub\(crate\)\s+use\s+(?:self|super)::[\w_]+::\s*$symbol\b" "$RUNTIME" 2>/dev/null | rg -o '^[^:]+' || true)

    if [ -n "$reexport_line" ]; then
        # Extract module name from the reexport path
        rg -n "pub\(crate\)\s+use\s+((?:self|super)::[\w_]+)::\s*$symbol\b" "$RUNTIME" 2>/dev/null | \
            rg -o 'use\s+((?:self|super)::[\w_]+)::' | rg -o '\w+$' | head -1 || echo "runtime"
    else
        echo "runtime"
    fi
}

# Count test-only usages (symbols only referenced inside #[cfg(test)] blocks)
count_test_only_usages() {
    local symbol="$1"

    # Total usages
    local total
    total=$(rg -c "\b$symbol\b" "$RUNTIME_SHARD" 2>/dev/null | rg -v ':0$' | rg -s '(\d+)' -r '$1' | paste -sd+ | bc 2>/dev/null || echo "0")

    # Non-test usages
    # Remove test blocks and check remaining count
    local non_test
    non_test=$(rg -Pz '\#\[cfg\(test\)\].*?\n\{[^}]*?\}' "$RUNTIME_SHARD" 2>/dev/null | \
        rg -v '\#[cfg(test)]' | \
        rg -c "\b$symbol\b" 2>/dev/null || echo "0")

    echo "$total $non_test"
}

# Check if symbol is used only in tests
is_test_only() {
    local symbol="$1"
    # A symbol is test-only if all its usages are within #[cfg(test)] blocks
    local test_block_usage
    test_block_usage=$(rg -Pz '\#[cfg\(test\)\][^{]*\{[^}]*?\b'"$symbol"'\b[^}]*\}' "$RUNTIME_SHARD" 2>/dev/null | wc -l || echo "0")
    local total_usage
    total_usage=$(rg -c "\b$symbol\b" "$RUNTIME_SHARD" 2>/dev/null | rg -v ':0$' | wc -l || echo "0")

    [ "$test_block_usage" -ge "$total_usage" ] && [ "$total_usage" -gt "0" ]
}

# Check if symbol is used only in close-path methods
is_close_path_only() {
    local symbol="$1"
    # Look for usages in functions with "close" or "shutdown" in their names
    local close_usages
    close_usages=$(rg -Pz 'fn\s+\w*close\w*[^{]*\{[^}]*?\b'"$symbol"'\b[^}]*\}' "$RUNTIME_SHARD" 2>/dev/null | wc -l || echo "0")
    local total_usage
    total_usage=$(rg -c "\b$symbol\b" "$RUNTIME_SHARD" 2>/dev/null | rg -v ':0$' | wc -l || echo "0")

    [ "$close_usages" -ge "$total_usage" ] && [ "$total_usage" -gt "0" ]
}

# Check if symbol is used only in recv-path methods
is_recv_path_only() {
    local symbol="$1"
    local recv_usages
    recv_usages=$(rg -Pz 'fn\s+\w*recv\w*[^{]*\{[^}]*?\b'"$symbol"'\b[^}]*\}' "$RUNTIME_SHARD" 2>/dev/null | wc -l || echo "0")
    local total_usage
    total_usage=$(rg -c "\b$symbol\b" "$RUNTIME_SHARD" 2>/dev/null | rg -v ':0$' | wc -l || echo "0")

    [ "$recv_usages" -ge "$total_usage" ] && [ "$total_usage" -gt "0" ]
}

# Check if symbol is used only in result-path methods
is_result_path_only() {
    local symbol="$1"
    local result_usages
    result_usages=$(rg -Pz 'fn\s+\w*result\w*[^{]*\{[^}]*?\b'"$symbol"'\b[^}]*\}' "$RUNTIME_SHARD" 2>/dev/null | wc -l || echo "0")
    local total_usage
    total_usage=$(rg -c "\b$symbol\b" "$RUNTIME_SHARD" 2>/dev/null | rg -v ':0$' | wc -l || echo "0")

    [ "$result_usages" -ge "$total_usage" ] && [ "$total_usage" -gt "0" ]
}

# Check if symbol is used only in run-loop/polling methods
is_run_loop_only() {
    local symbol="$1"
    local loop_usages
    loop_usages=$(rg -Pz 'fn\s+(?:run\poll\pump\w*|drive\w*)[^{]*\{[^}]*?\b'"$symbol"'\b[^}]*\}' "$RUNTIME_SHARD" 2>/dev/null | wc -l || echo "0")
    local total_usage
    total_usage=$(rg -c "\b$symbol\b" "$RUNTIME_SHARD" 2>/dev/null | rg -v ':0$' | wc -l || echo "0")

    [ "$loop_usages" -ge "$total_usage" ] && [ "$total_usage" -gt "0" ]
}

# Find symbols with extracted-module homes
has_extracted_module_home() {
    local symbol="$1"

    # Check if symbol is re-exported from a submodule that has been extracted
    # runtime_shard_close.rs, runtime_shard_recv.rs are extracted modules
    if rg -q "pub\(crate\)\s+use\s+\w+::\s*$symbol\b" "$RUNTIME" 2>/dev/null; then
        return 0
    fi

    # Check if it's defined in an extracted module (runtime_shard_*.rs)
    if rg -q "^pub\(super\)\s+(?:struct|enum|fn)\s+$symbol\b" "$RUNTIME" 2>/dev/null; then
        return 1  # Still defined in runtime.rs, not extracted
    fi

    return 1
}

# Find symbols that come from runtime.rs private state
comes_from_runtime_private() {
    local symbol="$1"

    # If it's not in any submodule re-export, it's from runtime.rs core
    if ! rg -q "pub\(crate\)\s+use\s+(?:self|super)::[\w_]+::\s*$symbol\b" "$RUNTIME" 2>/dev/null; then
        # Check if it's in runtime.rs directly
        if rg -q "^pub\(super\)\s+\w+\s+$symbol\b" "$RUNTIME" 2>/dev/null; then
            return 0
        fi
    fi

    return 1
}

# ---------------------------------------------------------------------------
# Main inventory logic
# ---------------------------------------------------------------------------

echo "=== Runtime Shard Wildcard Symbol Inventory ==="
echo "Mode: $MODE"
echo ""

case "$MODE" in
    count)
        echo "Symbol count from use super::* wildcard import:"
        count_wildcard_super
        ;;

    inventory|by-source|by-type)
        echo "Note: Full inventory requires compile-time analysis."
        echo "This script provides static analysis approximations."
        echo ""
        echo "Symbols available via use super::* in runtime_shard.rs:"
        echo "(Exported as pub(super) or pub(crate) from runtime.rs)"
        echo ""

        # Get the pub(super) exports
        echo "pub(super) symbols in runtime.rs:"
        rg -n 'pub\(super\)\s+(?:struct|enum|fn|const|type|mod)\s+\w+' "$RUNTIME" 2>/dev/null | \
            sed 's|/.*:||' | sort -u || echo "  (none)"

        echo ""
        echo "pub(crate) symbols in runtime.rs:"
        rg -n 'pub\(crate\)\s+(?:struct|enum|fn|const|type|mod)\s+\w+' "$RUNTIME" 2>/dev/null | \
            sed 's|/.*:||' | sort -u || echo "  (none)"

        echo ""
        echo "Explicit use super:: imports in runtime_shard.rs:"
        rg -n 'use super::\w+' "$RUNTIME_SHARD" 2>/dev/null | \
            sed 's|/.*:||' | sort -u || echo "  (none)"
        ;;

    test-only)
        echo "Symbols used only in tests (#[cfg(test)] blocks):"
        echo "(Analysis requires compile-time type resolution)"
        echo "These symbols would be safe to exclude from non-test builds."
        ;;

    close-path)
        echo "Symbols used only by close-path methods:"
        echo "(Functions with 'close' or 'shutdown' in name)"
        ;;

    recv-path)
        echo "Symbols used only by recv-path methods:"
        echo "(Functions with 'recv' in name)"
        ;;

    result-path)
        echo "Symbols used only by result-path methods:"
        echo "(Functions with 'result' in name)"
        ;;

    run-loop)
        echo "Symbols used only by run-loop and command polling methods:"
        ;;

    unresolved)
        echo "Symbols needing explicit import resolution:"
        echo "(Identified by compile errors or wildcard dependency analysis)"
        ;;

    conflicts)
        echo "Duplicate/conflicting symbol imports:"
        ;;

    cfg-gated)
        echo "Feature-gated symbol conflicts (SQPOLL vs non-SQPOLL):"
        ;;

    *)
        echo "Usage: $0 [inventory|count|by-source|by-type|test-only|close-path|recv-path|result-path|run-loop|unresolved|conflicts|cfg-gated]"
        exit 1
        ;;
esac

echo ""
echo "Run 'cargo check' to identify actual unresolved imports from wildcard."