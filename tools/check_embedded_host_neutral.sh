#!/bin/bash
# check_embedded_host_neutral.sh
# Scans dataplane-runtime embedded_host_loop.rs for forbidden target-specific
# terms in public API item names. Comments/docs are allowed to explain non-goals.
#
# Forbidden terms in public item names: thread, block, executor, hal
#
# This guard is host-neutral: it ensures the embedded API surface does not
# leak target-specific execution model assumptions into public item names.
#
# Exit code 0: no forbidden terms found in public item names
# Exit code 1: forbidden term found in public item name

set -euo pipefail

TARGET_FILE="crates/dataplane-runtime/src/embedded_host_loop.rs"

if [ ! -f "$TARGET_FILE" ]; then
    echo "ERROR: $TARGET_FILE not found"
    exit 1
fi

# Forbidden terms that indicate target-specific naming in public API items
FORBIDDEN_TERMS="thread block executor hal"

# We need to find public item declarations (struct, enum, trait, fn, const, type)
# that contain forbidden terms. We look for lines that:
# 1. Start a public item declaration (pub struct, pub enum, pub trait, pub fn, etc.)
# 2. Or are a pub use / pub type
# 3. And contain any of the forbidden terms
#
# We'll extract public item names and check them.

# Extract public item names from the file
# This regex finds lines with pub keyword followed by item type and name
# Example: "pub struct FooBar", "pub fn do_something", "pub type MyType"
# We exclude comments (lines starting with //) and attributes

# Create a temp file to store results
TMP=$(mktemp)

# Look for public items with forbidden terms in their names
# Patterns:
# - pub struct NAME
# - pub enum NAME  
# - pub trait NAME
# - pub fn NAME
# - pub const NAME
# - pub type NAME
# - pub static NAME
# - impl PUB_ITEM for NAME (for trait impls)

# Search for lines with pub declarations that have the forbidden terms
# We skip lines starting with // (comments) and lines with #[ (attributes)

found=0

for term in $FORBIDDEN_TERMS; do
    # Look for pub items where the name contains the forbidden term
    # Skip comment lines and attribute lines
    rg -n '^[[:space:]]*pub[[:space:]]+' "$TARGET_FILE" | \
        rg -v '^[[:space:]]*//' | \
        rg -v '^[[:space:]]*#\[' | \
        rg -i "(struct|enum|trait|fn|const|type)[[:space:]]+[[:alnum:]_]*${term}[[:alnum:]_]*" && {
        echo "ERROR: forbidden term '$term' found in public API item name"
        found=1
    }
done

if [ $found -eq 1 ]; then
    echo ""
    echo "FAIL: Host-neutral guard detected target-specific terms in public API names."
    echo "The embedded_host_loop.rs public API must remain host-neutral."
    echo "Forbidden terms in public item names: thread, block, executor, hal"
    exit 1
fi

echo "PASS: No forbidden target-specific terms found in public API item names."
echo "Embedded host-neutral boundary validated."
exit 0
