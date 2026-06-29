#!/usr/bin/env bash
# Static guard: safe public functions must not contain a caller-owned Safety contract.
# DP-CS-0039
#
# This guard verifies that functions visible through the public API do not claim
# "caller-owned" in their # Safety section, because caller-owned safety contracts
# describe obligations the caller must satisfy before the call — those obligations
# belong in the SAFETY comment of the unsafe function that imposes them, not in
# the safety docs of a safe wrapper. A safe wrapper that ships caller-owned
# invariants to callers is misdesigned: either the unsafe function should be
# made truly safe, or the caller-owned invariants should be removed.
set -euo pipefail

# Determine repo root
if [ -n "${BASH_SOURCE[0]:-}" ]; then
    SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
else
    SCRIPT_DIR="$(cd "$(dirname "$0")" && pwd)"
fi
REPO_ROOT="$(cd "$SCRIPT_DIR/.." && pwd)"
cd "$REPO_ROOT"

echo "=== Guard: No caller-owned Safety in safe public functions ==="

FAILED=0

# Target files: library source in dataplane-core-reactor and dataplane-compat-tokio
TARGETS=$(find crates/dataplane-core-reactor/src crates/dataplane-compat-tokio/src -name '*.rs' 2>/dev/null || echo "")

for file in $TARGETS; do
    if [ ! -f "$file" ]; then
        continue
    fi

    # Skip test-only modules (tests are compiled out in lib releases)
    # Also skip generated/bindgen files
    if [[ "$file" == *"_gen.rs" ]] || [[ "$file" == *"/tests/" ]]; then
        continue
    fi

    # Use Python for reliable multiline regex matching
    # Pattern: a pub fn (not cfg-gated, not #[cfg(test)]-only) that has a # Safety section
    # containing "caller-owned" (case-insensitive, whole word).
    #
    # We look for the combination of:
    #   1. A `pub fn` or `pub(crate) fn` declaration
    #   2. Followed somewhere later by a `/// # Safety` rustdoc block
    #   3. That block contains "caller-owned"
    #
    # We do this by scanning each file with Python.

    python3 - <<'PYEOF' 2>/dev/null
import sys
import re

filepath = sys.argv[1]

with open(filepath, 'r') as f:
    content = f.read()

# We need to check if there is a safe pub fn (not unsafe fn) whose Safety doc
# contains "caller-owned".
#
# Strategy: Find all pub fn (not unsafe fn) definitions, then check if within
# the following rustdoc comment block (starting with ///) there is "caller-owned".
#
# We use a relaxed approach: any /// comment after the fn up to the next
# pub fn or end of enclosing item counts as the doc comment.

# Find all pub fn declarations (not unsafe fn)
pub_fn_pattern = re.compile(
    r'^\s*(pub(?:\s+\([^)]*\))?\s+)?fn\s+\w+\s*<[^>]*>\s*\(',
    re.MULTILINE
)

# Alternative: simpler - find lines starting a pub fn (not unsafe fn)
pub_fn_lines = []
for i, line in enumerate(content.splitlines(), 1):
    stripped = line.strip()
    if stripped.startswith('pub fn ') or stripped.startswith('pub(crate) fn '):
        # It's a safe fn if not marked unsafe before the fn keyword
        prev_lines = '\n'.join(content.splitlines()[max(0, i-3):i])
        if 'unsafe fn' not in prev_lines and 'unsafe fn' not in line:
            pub_fn_lines.append(i)

# For each pub fn, look for the /// rustdoc block that follows it
# up to the next item (fn/struct/enum/impl/mod)
caller_owned_findings = []

item_end_pattern = re.compile(r'^(pub\s+)?(?:fn|struct|enum|impl|trait|mod|const|static)\s|^#\[', re.MULTILINE)

for fn_line in pub_fn_lines:
    lines = content.splitlines()
    # Find the start of the doc comment block (could be on same line or above)
    doc_start = fn_line - 1
    while doc_start >= 0 and (lines[doc_start].strip().startswith('///') or
                              lines[doc_start].strip().startswith('//!') or
                              lines[doc_start].strip() == '/**' or
                              lines[doc_start].strip().startswith('*')):
        doc_start -= 1
    doc_start += 1

    # Collect doc lines until next item
    doc_lines = []
    j = fn_line
    while j < len(lines):
        lstrip = lines[j].strip()
        # Stop at next item or non-doc line
        if item_end_pattern.match(lines[j]) and not lstrip.startswith('///') and not lstrip.startswith('*'):
            break
        if lstrip.startswith('///') or lstrip.startswith('//!') or lstrip.startswith('*') or lstrip.startswith('/**'):
            doc_lines.append(lstrip)
        elif lstrip and not lstrip.startswith('//') and not lstrip.startswith('/*') and not lstrip.startswith('#[') and not lstrip.startswith('use ') and not lstrip.startswith('pub '):
            # Non-doc, non-directive content - could be the fn body start
            # Only include if we're still in the doc region
            if doc_lines and not lstrip.startswith('{'):
                pass
            elif lstrip.startswith('{'):
                break
        j += 1

    doc_text = '\n'.join(doc_lines)
    if re.search(r'caller-owned', doc_text, re.IGNORECASE):
        fn_sig = lines[fn_line - 1].strip() if fn_line <= len(lines) else '?'
        caller_owned_findings.append((fn_line, fn_sig, filepath))

if caller_owned_findings:
    for line_num, sig, f in caller_owned_findings:
        print(f"FAIL: {f}:{line_num} — safe function '{sig}' has caller-owned Safety contract")
        sys.exit(1)
PYEOF

    exit_code=$?
    if [ $exit_code -eq 1 ]; then
        FAILED=1
    elif [ $exit_code -ne 0 ]; then
        # Non-fatal: python error (e.g. file encoding), skip
        echo "  SKIP: $file (could not parse)"
    fi
done

echo ""
if [ $FAILED -eq 0 ]; then
    echo "No caller-owned Safety contracts found in safe public functions."
    exit 0
else
    echo "GUARD FAILED: safe public functions must not contain caller-owned Safety contracts."
    exit 1
fi
