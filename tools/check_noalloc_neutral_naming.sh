#!/usr/bin/env bash
# No-alloc neutral naming guard.
#
# Rejects rp2040_noalloc as a Rust module path or source file name anywhere
# in crates/. Allows the feature name rp2040-noalloc (hyphen, not underscore)
# and the smoke crate name dataplane-rp2040-noalloc-smoke.
set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_DIR="$(dirname "$SCRIPT_DIR")"
cd "$REPO_DIR"

echo "=== No-Alloc Neutral Naming Guard ==="

FAILED=0

echo "Checking for rp2040_noalloc source file names in crates/..."
while IFS= read -r f; do
    echo "FAIL: file still carries board-named module name: $f"
    FAILED=1
done < <(find crates -name 'rp2040_noalloc.rs' -type f 2>/dev/null)

echo "Checking for rp2040_noalloc Rust module path references in crates/ source..."
while IFS= read -r match; do
    echo "FAIL: rp2040_noalloc module path found: $match"
    FAILED=1
done < <(rg --type rust -l 'rp2040_noalloc' crates/ 2>/dev/null | sort -u || true)

if [ "$FAILED" -ne 0 ]; then
    echo ""
    echo "No-alloc neutral naming guard FAILED."
    exit 1
fi

echo "  No rp2040_noalloc source files or module paths found in crates/."
echo ""
echo "PASS: no-alloc neutral naming guard passed."
exit 0
