#!/usr/bin/env bash
# Runtime Drive Reduce Boundary Guard
# Fails if drive_reduce_results directly assigns reducer_scheduled, directly
# pops from pending, or directly resets first_result_ns — all must go through
# the drain helpers owned by runtime_result_queue.rs.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUNTIME_RS="$REPO_ROOT/applications/erlang/ranch_uring/native/src/runtime.rs"
FAILED=0

echo "=== Runtime Drive Reduce Boundary Guard ==="

# ── 1. Extract drive_reduce_results body ──────────────────────────────────────
echo -n "Extracting drive_reduce_results body... "
BODY=$(python3 - "$RUNTIME_RS" <<'PY'
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
try:
    src = path.read_text()
except OSError as exc:
    print(f"read failed: {exc}", file=sys.stderr)
    sys.exit(1)

match = re.search(r"(?m)^    fn drive_reduce_results\s*\(&mut self\)", src)
if match is None:
    print("drive_reduce_results signature not found", file=sys.stderr)
    sys.exit(1)

brace_start = src.find("{", match.end())
if brace_start < 0:
    print("drive_reduce_results opening brace not found", file=sys.stderr)
    sys.exit(1)

depth = 1
body_start = brace_start + 1
idx = body_start
while idx < len(src):
    char = src[idx]
    if char == "{":
        depth += 1
    elif char == "}":
        depth -= 1
        if depth == 0:
            print(src[body_start:idx])
            sys.exit(0)
    idx += 1

print("drive_reduce_results closing brace not found", file=sys.stderr)
sys.exit(1)
PY
)

if [ -z "$BODY" ]; then
    echo "FAIL: could not extract drive_reduce_results body from $RUNTIME_RS"
    exit 1
fi
echo "ok"

grep_body() {
    local pattern="$1"
    grep -qE "$pattern" <<<"$BODY"
}

# ── 2. Fail if drive_reduce_results directly assigns reducer_scheduled ────────
echo -n "Checking drive_reduce_results does NOT directly assign reducer_scheduled... "
set +e
grep_body "self\.result_reduce\.reducer_scheduled\s*=\s*false"
GREP_STATUS=$?
set -e
if [ "$GREP_STATUS" -eq 0 ]; then
    echo "FAIL: drive_reduce_results directly assigns reducer_scheduled"
    echo "      The transition must go through result_reduce.begin_drain()."
    FAILED=1
elif [ "$GREP_STATUS" -eq 1 ]; then
    echo "ok"
else
    echo "FAIL: grep error while checking reducer_scheduled assignment"
    exit 1
fi

# ── 3. Fail if drive_reduce_results directly pops from pending ─────────────────
echo -n "Checking drive_reduce_results does NOT directly call pending.pop_front... "
set +e
grep_body "self\.result_reduce\.pending\.pop_front"
GREP_STATUS=$?
set -e
if [ "$GREP_STATUS" -eq 0 ]; then
    echo "FAIL: drive_reduce_results directly calls pending.pop_front"
    echo "      Event removal must go through result_reduce.pop_drain_event()."
    FAILED=1
elif [ "$GREP_STATUS" -eq 1 ]; then
    echo "ok"
else
    echo "FAIL: grep error while checking pending.pop_front"
    exit 1
fi

# ── 4. Fail if drive_reduce_results directly resets first_result_ns ───────────
echo -n "Checking drive_reduce_results does NOT directly assign first_result_ns... "
set +e
grep_body "self\.result_reduce\.first_result_ns\s*=\s*None"
GREP_STATUS=$?
set -e
if [ "$GREP_STATUS" -eq 0 ]; then
    echo "FAIL: drive_reduce_results directly assigns first_result_ns = None"
    echo "      The reset must go through result_reduce.finish_drain()."
    FAILED=1
elif [ "$GREP_STATUS" -eq 1 ]; then
    echo "ok"
else
    echo "FAIL: grep error while checking first_result_ns assignment"
    exit 1
fi

# ── 5. Verify drain helpers ARE called ────────────────────────────────────────
echo -n "Checking drive_reduce_results DOES call begin_drain... "
set +e
grep_body "result_reduce\.begin_drain"
GREP_STATUS=$?
set -e
if [ "$GREP_STATUS" -eq 0 ]; then
    echo "ok"
elif [ "$GREP_STATUS" -eq 1 ]; then
    echo "FAIL: drive_reduce_results does not call result_reduce.begin_drain()"
    echo "      Drain must use the helper."
    FAILED=1
else
    echo "FAIL: grep error while checking begin_drain call"
    exit 1
fi

echo -n "Checking drive_reduce_results DOES call pop_drain_event... "
set +e
grep_body "result_reduce\.pop_drain_event"
GREP_STATUS=$?
set -e
if [ "$GREP_STATUS" -eq 0 ]; then
    echo "ok"
elif [ "$GREP_STATUS" -eq 1 ]; then
    echo "FAIL: drive_reduce_results does not call result_reduce.pop_drain_event()"
    echo "      Drain must use the helper."
    FAILED=1
else
    echo "FAIL: grep error while checking pop_drain_event call"
    exit 1
fi

echo -n "Checking drive_reduce_results DOES call finish_drain... "
set +e
grep_body "result_reduce\.finish_drain"
GREP_STATUS=$?
set -e
if [ "$GREP_STATUS" -eq 0 ]; then
    echo "ok"
elif [ "$GREP_STATUS" -eq 1 ]; then
    echo "FAIL: drive_reduce_results does not call result_reduce.finish_drain()"
    echo "      Drain must use the helper."
    FAILED=1
else
    echo "FAIL: grep error while checking finish_drain call"
    exit 1
fi

# ── 6. Fail-closed on empty/over-captured body ────────────────────────────────
echo -n "Verifying drive_reduce_results body is non-empty... "
BODY_LINES=$(echo "$BODY" | wc -l)
if [ "$BODY_LINES" -lt 5 ]; then
    echo "FAIL: drive_reduce_results body looks empty or truncated ($BODY_LINES lines)"
    exit 1
fi
if [ "$BODY_LINES" -gt 120 ]; then
    echo "FAIL: drive_reduce_results body looks over-captured ($BODY_LINES lines)"
    exit 1
fi
echo "ok ($BODY_LINES lines)"

# ── 7. Fail-closed on search/parse errors ─────────────────────────────────────
echo -n "Verifying drain helper calls are present... "
set +e
grep_body "result_reduce\.(begin_drain|pop_drain_event|finish_drain)"
GREP_STATUS=$?
set -e
if [ "$GREP_STATUS" -eq 0 ]; then
    echo "ok"
elif [ "$GREP_STATUS" -eq 1 ]; then
    echo "FAIL: no drain helper calls found in body (search may have failed)"
    exit 1
else
    echo "FAIL: unexpected grep status $GREP_STATUS"
    exit 1
fi

if [ $FAILED -eq 1 ]; then
    echo ""
    echo "RUNTIME DRIVE REDUCE BOUNDARY GUARD FAILED"
    exit 1
fi

echo ""
echo "Runtime drive reduce boundary guard passed."
exit 0
