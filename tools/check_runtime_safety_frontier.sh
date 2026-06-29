#!/usr/bin/env bash
# Host-neutral guard for runtime safety-frontier invariants.
#
# This guard does not prove io_uring behavior. It prevents the known safety
# frontier from silently drifting: raw fd closes and POD zero-init must carry
# local SAFETY comments, and registered-buffer slot generation checks/tests must
# remain present.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

echo "=== Runtime Safety Frontier Guard ==="

python3 - <<'PY'
from pathlib import Path
import sys

targets = [
    Path("applications/erlang/ranch_uring/native/src/runtime.rs"),
    Path("applications/erlang/ranch_uring/native/src/runtime_shard.rs"),
    Path("applications/erlang/ranch_uring/native/src/runtime_dispatch.rs"),
    Path("applications/erlang/ranch_uring/native/src/runtime_driver.rs"),
    Path("applications/erlang/ranch_uring/native/src/runtime_reactor.rs"),
]

patterns = ("libc::close", "std::mem::zeroed", "mem::zeroed")
failures = []

for path in targets:
    if not path.exists():
        continue
    lines = path.read_text().splitlines()
    for index, line in enumerate(lines):
        if not any(pattern in line for pattern in patterns):
            continue
        window = lines[max(0, index - 4):index + 1]
        if not any("SAFETY:" in candidate for candidate in window):
            failures.append(f"{path}:{index + 1}: unsafe frontier operation lacks nearby SAFETY comment")

reactor = Path("applications/erlang/ranch_uring/native/src/runtime_reactor.rs")
reactor_text = reactor.read_text()
required_reactor_snippets = [
    "slot.generation != slot_id.generation",
    "slot.leased = false",
    "slot.generation = next_generation(slot.generation)",
    "registered_iovecs_stable_with_ids",
]
for snippet in required_reactor_snippets:
    if snippet not in reactor_text:
        failures.append(f"{reactor}: missing registered-buffer stability snippet: {snippet}")

shard = Path("applications/erlang/ranch_uring/native/src/runtime_shard.rs")
shard_text = shard.read_text()
required_test_names = [
    "fixed_read_recv_cqe_returns_data_from_stable_slot",
    "provided_recv_slot_remains_inflight_until_successful_recv_completion",
]
for test_name in required_test_names:
    if test_name not in shard_text:
        failures.append(f"{shard}: missing registered-buffer stability test: {test_name}")

if failures:
    for failure in failures:
        print(f"FAIL: {failure}")
    sys.exit(1)

print("Runtime safety frontier guard passed.")
PY
