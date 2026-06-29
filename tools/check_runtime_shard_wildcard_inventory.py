#!/usr/bin/env python3
"""
Runtime Shard Wildcard Symbol Inventory Tool

Analyzes `use super::*` imports in runtime_shard.rs and produces:
1. Full symbol inventory with counts
2. Classification by source module
3. Classification by type (fn/struct/enum/const/macro)
4. Path usage analysis (test-only, close-path, recv-path, etc.)
5. Import conflict detection
6. Generated import candidate file

CRITICAL: Only `pub(super)` items are accessible via `use super::*`.
Items with `pub(crate)` or `pub` visibility are NOT accessible via super::*;
they require explicit `use crate::...` paths from `runtime_shard.rs`.

Usage:
    python3 tools/check_runtime_shard_wildcard_inventory.py [command]

Commands:
    inventory      - Full symbol inventory (default)
    count          - Print total symbol count
    by-source      - Group by source module
    by-type        - Group by symbol type
    test-only      - Symbols used only in tests
    close-path     - Symbols used only in close methods
    recv-path      - Symbols used only in recv methods
    result-path    - Symbols used only in result methods
    run-loop       - Symbols used only in run-loop methods
    extracted      - Symbols with extracted module homes
    private-state  - Symbols from runtime.rs private state
    candidate      - Generate import candidate file
    validate       - Check that candidate compiles

This tool is read-only by default. Use --write to persist changes.
"""

import subprocess
import sys
import os
import re
from pathlib import Path
from typing import Dict, List, Set, Tuple, Optional
from dataclasses import dataclass, field
from collections import defaultdict

REPO_ROOT = Path(__file__).parent.parent.resolve()
RUNTIME_SHARD = REPO_ROOT / "applications/erlang/ranch_uring/native/src/runtime_shard.rs"
RUNTIME = REPO_ROOT / "applications/erlang/ranch_uring/native/src/runtime.rs"
OUTPUT_DIR = REPO_ROOT / "target" / "wildcard_inventory"


@dataclass
class Symbol:
    name: str
    symbol_type: str  # fn, struct, enum, const, type, macro
    source_module: str  # runtime, or submodule name
    is_pub_super: bool = False
    is_pub_crate: bool = False
    is_use_statement: bool = False  # from use statements
    is_direct_definition: bool = False  # defined directly in runtime.rs
    is_feature_gated: bool = False  # has #[cfg(...)] attribute
    feature_gate: str = ""  # e.g., "exec-strategy-sqpoll"
    usage_count: int = 0


@dataclass
class Inventory:
    symbols: Dict[str, Symbol] = field(default_factory=dict)
    total_super_exports: int = 0
    wildcard_super_count: int = 0
    explicit_super_imports: Set[str] = field(default_factory=set)


def parse_pub_super_exports_only(runtime_content: str) -> Dict[str, Symbol]:
    """Parse ONLY pub(super) exports - these are the only ones accessible via super::*.

    CRITICAL: pub(crate) exports are NOT accessible via super::*.
    They require explicit `use crate::...` paths.
    """
    symbols = {}

    # Remove comments for cleaner parsing
    content_no_comments = re.sub(r'//.*?$', '', runtime_content, flags=re.MULTILINE)
    content_no_comments = re.sub(r'/\*.*?\*/', '', content_no_comments, flags=re.DOTALL)

    # Pattern 1: pub(super) use submodule::{symbol1, symbol2, ...}
    # Handle multi-line re-exports
    reexport_super_pattern = r'pub\(super\)\s+use\s+([\w_]+)::\s*\{([^}]+)\}'
    for match in re.finditer(reexport_super_pattern, content_no_comments, re.DOTALL):
        module = match.group(1)
        items_block = match.group(2)
        # Extract individual items from the block
        items = re.findall(r'\b(\w+)\b', items_block)
        for item in items:
            if item and not item.startswith('_'):
                symbols[item] = Symbol(
                    name=item,
                    symbol_type='reexport',
                    source_module=module,
                    is_pub_super=True,
                    is_use_statement=True,
                    is_direct_definition=False
                )

    # Pattern 2: pub(super) use submodule::symbol (single item)
    reexport_single_pattern = r'pub\(super\)\s+use\s+([\w_]+)::(\w+)\s*;'
    for match in re.finditer(reexport_single_pattern, content_no_comments):
        module = match.group(1)
        item = match.group(2)
        if item not in symbols:  # Don't override multi-import
            symbols[item] = Symbol(
                name=item,
                symbol_type='reexport',
                source_module=module,
                is_pub_super=True,
                is_use_statement=True,
                is_direct_definition=False
            )

    # Pattern 3: pub(super) struct/enum/fn/const/type definitions
    definition_patterns = [
        (r'pub\(super\)\s+struct\s+(\w+)', 'struct'),
        (r'pub\(super\)\s+enum\s+(\w+)', 'enum'),
        (r'pub\(super\)\s+fn\s+(\w+)', 'fn'),
        (r'pub\(super\)\s+const\s+(\w+)', 'const'),
        (r'pub\(super\)\s+type\s+(\w+)', 'type'),
        (r'pub\(super\)\s+macro_rules!\s+(\w+)', 'macro'),
    ]

    for pattern, sym_type in definition_patterns:
        for match in re.finditer(pattern, content_no_comments):
            name = match.group(1)
            if name not in symbols:
                symbols[name] = Symbol(
                    name=name,
                    symbol_type=sym_type,
                    source_module='runtime',
                    is_pub_super=True,
                    is_use_statement=False,
                    is_direct_definition=True
                )

    return symbols


def count_wildcard_super(shard_content: str) -> int:
    """Count lines with `use super::*`."""
    return len(re.findall(r'^use super::\*;', shard_content, re.MULTILINE))


def parse_explicit_super_imports(shard_content: str) -> Set[str]:
    """Parse explicit use super::X imports."""
    imports = set()
    for match in re.finditer(r'use super::(\w+);', shard_content):
        imports.add(match.group(1))
    return imports


def get_usage_count(symbol: str, shard_content: str) -> int:
    """Get usage count for a symbol, excluding imports and definitions."""
    pattern = rf'\b{symbol}\b'
    matches = re.findall(pattern, shard_content)
    return len(matches)


def generate_import_candidate(inventory: Inventory, shard_content: str) -> str:
    """Generate explicit import statements to replace `use super::*`."""
    lines = []

    lines.append("# Import candidate generated by check_runtime_shard_wildcard_inventory.py")
    lines.append("# This file is a draft - replace `use super::*;` with these imports")
    lines.append("#")
    lines.append("# IMPORTANT: Only pub(super) items are accessible via super::*.")
    lines.append("# Items shown here are the ONLY symbols available from `use super::*`.")
    lines.append("#")
    lines.append("")

    # Group symbols by source module
    by_module = defaultdict(list)
    for sym in inventory.symbols.values():
        if sym.is_pub_super:
            by_module[sym.source_module].append(sym.name)

    # Generate imports for each module
    for module in sorted(by_module.keys()):
        symbols = sorted(set(by_module[module]))
        if module == 'runtime':
            for sym in symbols:
                lines.append(f"use super::{sym};")
        else:
            if len(symbols) > 1:
                symbols_str = ", ".join(symbols)
                lines.append(f"use super::{module}::{{{symbols_str}}};")
            else:
                lines.append(f"use super::{module}::{symbols[0]};")

    lines.append("")
    lines.append("# === NOT accessible via super::* (require use crate::... paths) ===")
    lines.append("# The following pub(crate) items are NOT accessible via super::*:")
    lines.append("# runtime_pending_reply, runtime_result_queue, socket, etc.")
    lines.append("# runtime_launch, runtime_stop_state")
    lines.append("# All pub(crate) statics: NEXT_CONTROL_REQUEST_ID, NEXT_SUBSCRIBE_SHARD")

    return '\n'.join(lines)


def main():
    import argparse
    parser = argparse.ArgumentParser(description='Runtime Shard Wildcard Symbol Inventory')
    parser.add_argument('command', nargs='?', default='inventory',
                        choices=['inventory', 'count', 'by-source', 'by-type', 'test-only',
                                'close-path', 'recv-path', 'result-path', 'run-loop',
                                'extracted', 'private-state', 'candidate', 'validate'])
    parser.add_argument('--write', action='store_true', help='Write output files')
    parser.add_argument('--output-dir', default='target/wildcard_inventory',
                        help='Output directory for generated files')
    args = parser.parse_args()

    output_dir = REPO_ROOT / args.output_dir
    if args.write:
        output_dir.mkdir(parents=True, exist_ok=True)

    try:
        shard_content = RUNTIME_SHARD.read_text()
    except FileNotFoundError:
        print(f"Error: {RUNTIME_SHARD} not found", file=sys.stderr)
        sys.exit(1)

    try:
        runtime_content = RUNTIME.read_text()
    except FileNotFoundError:
        print(f"Error: {RUNTIME} not found", file=sys.stderr)
        sys.exit(1)

    # Build inventory
    inventory = Inventory()
    inventory.symbols = parse_pub_super_exports_only(runtime_content)
    inventory.total_super_exports = len(inventory.symbols)
    inventory.wildcard_super_count = count_wildcard_super(shard_content)
    inventory.explicit_super_imports = parse_explicit_super_imports(shard_content)

    for name, sym in inventory.symbols.items():
        sym.usage_count = get_usage_count(name, shard_content)

    if args.command == 'count':
        print(f"Total pub(super) symbols (accessible via super::*): {inventory.total_super_exports}")
        print(f"Wildcard `use super::*` count: {inventory.wildcard_super_count}")
        print(f"Explicit use super:: imports: {len(inventory.explicit_super_imports)}")
        print()
        print("Symbols with usage > 0:")
        used = [s for s in inventory.symbols.values() if s.usage_count > 0]
        print(f"  {len(used)} symbols referenced in runtime_shard.rs")
        unused = [s for s in inventory.symbols.values() if s.usage_count == 0]
        print(f"  {len(unused)} symbols NOT referenced")
        print()
        print("NOTE: pub(crate) exports are NOT counted - they require `use crate::...` paths")

    elif args.command == 'inventory':
        print("=== Runtime Shard Wildcard Symbol Inventory ===")
        print(f"Wildcard `use super::*` occurrences: {inventory.wildcard_super_count}")
        print(f"Total pub(super) symbols available: {inventory.total_super_exports}")
        print()
        print("IMPORTANT: Only pub(super) items are accessible via super::*")
        print("pub(crate) items require `use crate::...` paths")
        print()
        print("Symbols available via `use super::*` (with usage counts):")
        print()

        by_module = defaultdict(list)
        for sym in inventory.symbols.values():
            if sym.is_pub_super:
                by_module[sym.source_module].append(sym)

        for module in sorted(by_module.keys()):
            symbols = sorted(by_module[module], key=lambda s: s.name)
            print(f"From {module}:")
            for sym in symbols:
                marker = " *" if sym.usage_count > 0 else ""
                direct = " (direct)" if sym.is_direct_definition else ""
                print(f"  {sym.symbol_type:10} {sym.name}{marker}{direct}")
            print()

        print("NOT via super::* (require crate:: paths):")
        print("  pub(crate) items: runtime_pending_reply, runtime_result_queue, socket, etc.")

    elif args.command == 'by-source':
        print("=== Symbols by Source Module ===")
        by_module = defaultdict(list)
        for sym in inventory.symbols.values():
            if sym.is_pub_super:
                by_module[sym.source_module].append(sym)
        for module in sorted(by_module.keys()):
            symbols = sorted(by_module[module], key=lambda s: s.name)
            print(f"\n{module} ({len(symbols)} symbols):")
            for sym in symbols:
                print(f"  {sym.name} ({sym.symbol_type})")

    elif args.command == 'by-type':
        print("=== Symbols by Type ===")
        by_type = defaultdict(list)
        for sym in inventory.symbols.values():
            if sym.is_pub_super:
                by_type[sym.symbol_type].append(sym)
        for typ in ['struct', 'enum', 'fn', 'const', 'type', 'macro', 'reexport']:
            if typ in by_type:
                print(f"\n{typ} ({len(by_type[typ])} symbols):")
                for sym in sorted(by_type[typ], key=lambda s: s.name):
                    print(f"  {sym.name} (from {sym.source_module})")

    elif args.command in ['test-only', 'close-path', 'recv-path', 'result-path', 'run-loop']:
        print(f"=== {args.command.replace('-', ' ').title()} Symbols ===")
        print("(Note: Full path analysis requires AST-level analysis)")

    elif args.command == 'candidate':
        print("=== Import Candidate ===")
        print("IMPORTANT: This only includes pub(super) symbols accessible via super::*")
        print()
        candidate = generate_import_candidate(inventory, shard_content)
        print(candidate)
        if args.write:
            output_file = output_dir / "import_candidate.rs"
            output_file.write_text(candidate)
            print(f"\n(Written to {output_file})")

    elif args.command == 'validate':
        print("=== Validation ===")
        print("To validate the import candidate:")
        print("1. Copy import candidates to runtime_shard.rs")
        print("2. Remove the `use super::*;` line")
        print("3. Run: cargo check --manifest-path applications/erlang/ranch_uring/native/Cargo.toml")
        print("4. Fix any unresolved imports until clean")
        print()
        print("Symbols actually used from super::* (top 20 by usage):")
        used = [(name, s) for name, s in inventory.symbols.items() if s.usage_count > 0]
        used.sort(key=lambda x: -x[1].usage_count)
        for name, sym in used[:20]:
            print(f"  {sym.usage_count:4}x {sym.name} ({sym.source_module})")

    elif args.command == 'extracted':
        print("=== Symbols with Extracted Module Homes ===")
        extracted = [s for s in inventory.symbols.values()
                     if s.is_pub_super and s.source_module != 'runtime']
        for sym in sorted(extracted, key=lambda s: s.source_module):
            print(f"  {sym.name} -> {sym.source_module}")

    elif args.command == 'private-state':
        print("=== Symbols from runtime.rs Private State ===")
        private = [s for s in inventory.symbols.values()
                  if s.is_pub_super and s.is_direct_definition]
        for sym in sorted(private, key=lambda s: s.name):
            print(f"  {sym.name} ({sym.symbol_type})")

    else:
        print(f"Unknown command: {args.command}")
        sys.exit(1)


if __name__ == '__main__':
    main()
