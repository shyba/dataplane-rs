#!/usr/bin/env bash
# OpenSpec closeout guard.
# Fails when completed OpenSpec changes leave unchecked tasks or malformed
# Requirement blocks behind.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

echo "=== OpenSpec Closeout Guard ==="

python3 - <<'PY'
import json
import re
import sys
from pathlib import Path

root = Path.cwd()
changes = root / "changes"
if not changes.exists():
    print("No changes/ directory found; skipping OpenSpec closeout guard.")
    sys.exit(0)

completed_paths = set()
active_run = root / ".hermes-harness" / "active-run.json"
if active_run.exists():
    try:
        active = json.loads(active_run.read_text())
    except json.JSONDecodeError as exc:
        print(f"FAIL: invalid JSON in {active_run}: {exc}")
        sys.exit(1)
    if active.get("status") == "complete":
        source_path = (active.get("metadata") or {}).get("source_path")
        if source_path:
            completed_paths.add(str(Path(source_path).resolve()))

failures = []

for spec in sorted(changes.glob("*/specs/*/spec.md")):
    lines = spec.read_text().splitlines()
    for idx, line in enumerate(lines):
        if line.startswith("### Requirement:"):
            next_line = ""
            for candidate in lines[idx + 1:]:
                if candidate.strip():
                    next_line = candidate
                    break
            if not re.search(r"\b(SHALL|MUST)\b", next_line):
                failures.append(
                    f"{spec}: Requirement at line {idx + 1} is not followed by SHALL/MUST"
                )

for tasks in sorted(changes.glob("*/tasks.md")):
    text = tasks.read_text()
    task_root = str(tasks.parent.resolve())
    has_closeout = "## Closeout Evidence" in text
    has_fresh_anchor = any(
        anchor in text
        for anchor in (
            "run_id",
            "updated_at",
            ".hermes-harness/active-run.json",
            "task.snapshot.json",
            "events.jsonl",
            "metadata.source_path",
            "sha256",
        )
    )
    unchecked = [
        (line_no, line.strip())
        for line_no, line in enumerate(text.splitlines(), 1)
        if line.lstrip().startswith("- [ ]")
    ]
    must_be_closed = has_closeout or task_root in completed_paths
    if must_be_closed and not has_closeout:
        failures.append(f"{tasks}: completed change is missing ## Closeout Evidence")
    if must_be_closed and not has_fresh_anchor:
        failures.append(
            f"{tasks}: completed change closeout evidence is missing a run_id/timestamp/hash/active-run anchor"
        )
    if must_be_closed and unchecked:
        sample = ", ".join(f"line {line_no}" for line_no, _ in unchecked[:5])
        failures.append(f"{tasks}: completed change has unchecked tasks ({sample})")

if failures:
    for failure in failures:
        print(f"FAIL: {failure}")
    sys.exit(1)

print("OpenSpec closeout guard passed.")
PY
