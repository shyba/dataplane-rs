#!/usr/bin/env bash
# Hermes active-run status consistency guard.
# If the active run ledger finalizes complete, active-run metadata must say so.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

echo "=== Hermes Active Run Status Guard ==="

python3 - <<'PY'
import json
import sys
from pathlib import Path

active_path = Path(".hermes-harness/active-run.json")
if not active_path.exists():
    print("No active-run.json found; skipping Hermes status guard.")
    sys.exit(0)

try:
    active = json.loads(active_path.read_text())
except json.JSONDecodeError as exc:
    print(f"FAIL: invalid JSON in {active_path}: {exc}")
    sys.exit(1)

run_id = active.get("run_id")
if not run_id:
    print("No run_id in active-run.json; skipping Hermes status guard.")
    sys.exit(0)

events_path = Path(".hermes-harness/runs") / run_id / "events.jsonl"
if not events_path.exists():
    if active.get("status") == "complete":
        print(
            "FAIL: active-run.json says status=\"complete\" "
            f"but missing run ledger {events_path}"
        )
        sys.exit(1)
    # Harness run ledgers can be local/ignored for in-progress or absent runs.
    print(f"Run ledger {events_path} not present; skipping Hermes status guard.")
    sys.exit(0)

latest_final = None
latest_session = None
for line_no, line in enumerate(events_path.read_text().splitlines(), 1):
    try:
        event = json.loads(line)
    except json.JSONDecodeError as exc:
        print(f"FAIL: invalid JSON in {events_path}:{line_no}: {exc}")
        sys.exit(1)
    if event.get("event") == "transform_final_response":
        latest_final = event
    if event.get("t") == "session" and event.get("event") == "ended":
        latest_session = event

if latest_final and latest_final.get("status") == "complete" and latest_final.get("completed") is True:
    if active.get("status") != "complete":
        print(
            "FAIL: active-run.json does not have status=\"complete\" "
            f"but latest finalization s={latest_final.get('s')} is complete"
        )
        sys.exit(1)
    if latest_session and latest_session.get("completed") is not True:
        print(
            "FAIL: latest finalization is complete but latest session ended "
            f"with completed={latest_session.get('completed')}"
        )
        sys.exit(1)

print("Hermes active-run status guard passed.")
PY
