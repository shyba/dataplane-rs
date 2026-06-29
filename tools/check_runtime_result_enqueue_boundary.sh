#!/usr/bin/env bash
# Runtime Result Enqueue Boundary Guard
# Fails if queue_reply directly calls self.result_reduce.pending.push_back or
# directly assigns self.result_reduce.first_result_ns — both must go through the
# enqueue_event helper owned by runtime_result_queue.rs.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
RUNTIME_RS="$REPO_ROOT/applications/erlang/ranch_uring/native/src/runtime.rs"
FAILED=0

echo "=== Runtime Result Enqueue Boundary Guard ==="

# ── 1. Extract queue_reply body ───────────────────────────────────────────────
# Parse braces from the function signature. This intentionally fails closed if
# the function cannot be found or the closing brace cannot be matched.
echo -n "Extracting queue_reply body... "
if ! QUEUE_REPLY_BODY=$(python3 - "$RUNTIME_RS" <<'PY'
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
try:
    src = path.read_text()
except OSError as exc:
    print(f"read failed: {exc}", file=sys.stderr)
    sys.exit(1)

match = re.search(r"(?m)^    fn queue_reply\s*\(", src)
if match is None:
    print("queue_reply signature not found", file=sys.stderr)
    sys.exit(1)

brace_start = src.find("{", match.end())
if brace_start < 0:
    print("queue_reply opening brace not found", file=sys.stderr)
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

print("queue_reply closing brace not found", file=sys.stderr)
sys.exit(1)
PY
); then
    echo "FAIL: could not extract queue_reply body from $RUNTIME_RS"
    exit 1
fi

if [ -z "$QUEUE_REPLY_BODY" ]; then
    echo "FAIL: could not extract queue_reply body from $RUNTIME_RS"
    exit 1
fi
echo "ok"

grep_queue_reply_body() {
    local pattern="$1"
    grep -qE "$pattern" <<<"$QUEUE_REPLY_BODY"
}

# ── 2. Fail if queue_reply directly calls pending.push_back ──────────────────
echo -n "Checking queue_reply does NOT directly call pending.push_back... "
set +e
grep_queue_reply_body "self\.result_reduce\.pending\.push_back"
GREP_STATUS=$?
set -e
if [ "$GREP_STATUS" -eq 0 ]; then
    echo "FAIL: queue_reply directly calls self.result_reduce.pending.push_back"
    echo "      The enqueue must go through the enqueue_event helper."
    FAILED=1
elif [ "$GREP_STATUS" -eq 1 ]; then
    echo "ok"
else
    echo "FAIL: grep error while checking pending.push_back"
    exit 1
fi

# ── 3. Fail if queue_reply directly assigns first_result_ns ──────────────────
echo -n "Checking queue_reply does NOT directly assign first_result_ns... "
set +e
grep_queue_reply_body "self\.result_reduce\.first_result_ns\s*=\s*Some"
GREP_STATUS=$?
set -e
if [ "$GREP_STATUS" -eq 0 ]; then
    echo "FAIL: queue_reply directly assigns self.result_reduce.first_result_ns"
    echo "      The enqueue helper sets first_result_ns internally."
    FAILED=1
elif [ "$GREP_STATUS" -eq 1 ]; then
    echo "ok"
else
    echo "FAIL: grep error while checking first_result_ns assignment"
    exit 1
fi

# ── 4. Verify enqueue_event helper IS called in the deferred path ───────────
echo -n "Checking queue_reply DOES call enqueue_event for deferred path... "
set +e
grep_queue_reply_body "result_reduce\.enqueue_event"
GREP_STATUS=$?
set -e
if [ "$GREP_STATUS" -eq 0 ]; then
    echo "ok"
elif [ "$GREP_STATUS" -eq 1 ]; then
    echo "FAIL: queue_reply does not call result_reduce.enqueue_event"
    echo "      Deferred results must be enqueued via the helper."
    FAILED=1
else
    echo "FAIL: grep error while checking enqueue_event call"
    exit 1
fi

# ── 5. Fail-closed on empty body (parser error) ───────────────────────────────
echo -n "Verifying queue_reply body is non-empty... "
BODY_LINES=$(echo "$QUEUE_REPLY_BODY" | wc -l)
if [ "$BODY_LINES" -lt 3 ]; then
    echo "FAIL: queue_reply body looks empty or truncated ($BODY_LINES lines)"
    exit 1
fi
if [ "$BODY_LINES" -gt 160 ]; then
    echo "FAIL: queue_reply body looks over-captured ($BODY_LINES lines)"
    exit 1
fi
echo "ok ($BODY_LINES lines)"

if [ $FAILED -eq 1 ]; then
    echo ""
    echo "RUNTIME RESULT ENQUEUE BOUNDARY GUARD FAILED"
    exit 1
fi

echo ""
echo "Runtime result enqueue boundary guard passed."
exit 0
