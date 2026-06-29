#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

spec="changes/__archived_changes_2026-05-31/x86_64-microkernel-cli-http-operator-parity/specs/cli-http-operator-parity/spec.md"
proposal="changes/__archived_changes_2026-05-31/x86_64-microkernel-cli-http-operator-parity/proposal.md"
tasks="changes/__archived_changes_2026-05-31/x86_64-microkernel-cli-http-operator-parity/tasks.md"
makefile="Makefile"
roadmap_files=(
  "aidocs/055_microkernel_tls_deferred_robust_appliance_plan_2026-05-31.md"
  "aidocs/050_microkernel_robust_design_frontier_2026-05-30.md"
  "aidocs/050_microkernel_robust_design_diary_2026-05-30.md"
  "aidocs/019_runtime_plan_pointer_todo_2026-04-18.md"
)

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_file() {
  [[ -f "$1" ]] || fail "missing required file: $1"
}

require_literal() {
  local file="$1"
  local literal="$2"
  local note="$3"
  grep -Fq -- "$literal" "$file" || fail "$note"
}

for file in "$spec" "$proposal" "$tasks" "$makefile"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel CLI/HTTP Operator Parity Contract ==="

require_literal "$spec" '# Capability: CLI/HTTP Operator Parity' "spec heading must match the new capability"
require_literal "$spec" 'The system SHALL expose the same bounded operator facts through serial CLI and HTTP for routes, service caps, storage mode, network counters, timer status, fault status, and generation ID.' \
  "spec must define the parity set"
require_literal "$spec" 'The system MUST keep the parity surface read-only and bounded so it cannot be used as a debugger or a privileged inspection channel.' \
  "spec must deny debugger use"
require_literal "$spec" 'The system MUST enforce the same caps on CLI and HTTP responses for the parity set and reject over-cap requests without widening the operator surface.' \
  "spec must enforce caps consistently"
require_literal "$spec" 'The system MUST produce parity summary evidence, a CLI transcript, an HTTP body, a cap-enforcement negative, and a forbidden-command guard result after the last relevant mutation.' \
  "spec must require fresh evidence"

require_literal "$tasks" '## Completion Gate' "tasks must include a completion gate"
require_literal "$tasks" 'fresh after the last mutation' "tasks must require fresh evidence"

require_literal "$makefile" 'x86_64-microkernel-cli-http-operator-parity-contract:' \
  "Makefile must expose the parity contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_cli_http_operator_parity_contract.sh' \
  "Makefile must run the parity contract guard"
require_literal "$makefile" 'x86_64-microkernel-cli-http-operator-parity:' \
  "Makefile must expose the parity packet target"
require_literal "$makefile" 'DP_MICROKERNEL_CLI_HTTP_OPERATOR_PARITY_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --cli-http-operator-parity-proof' \
  "Makefile must run the parity proof mode"
require_literal "$makefile" '--cli-http-operator-parity-proof' \
  "Makefile or runner must expose the parity proof flag"
require_literal "tools/x86_64_microkernel_fat32_run.sh" 'DPBOUNDS:cli line=64 commands=24' \
  "runner must reflect the updated CLI command bound"
require_literal "tools/x86_64_microkernel_fat32_run.sh" 'DPMK:CLI-BEGIN:15:parity' \
  "runner must insert parity before queues"
require_literal "tools/x86_64_microkernel_fat32_run.sh" '-chardev "socket,id=cli,path=$serial_sock,server=on,wait=on"' \
  "parity proof must use the established socket-based serial harness"
require_literal "tools/x86_64_microkernel_fat32_run.sh" 'cli-http-operator-parity.summary' \
  "runner must allocate the parity summary artifact"
require_literal "tools/x86_64_microkernel_fat32_run.sh" 'cli-http-operator-parity.http.body' \
  "runner must allocate the HTTP body artifact"
require_literal "tools/x86_64_microkernel_fat32_run.sh" 'cli-http-operator-parity.cap-negative.status' \
  "runner must allocate the cap-negative status artifact"
require_literal "tools/x86_64_microkernel_fat32_run.sh" 'cli-http-operator-parity.forbidden-guard.txt' \
  "runner must allocate the forbidden-command guard artifact"
require_literal "tools/x86_64_microkernel_fat32_run.sh" 'parity_forbidden_guard_status=pass' \
  "runner must label forbidden evidence as pass after rejected probes are recorded"

for file in "${roadmap_files[@]}"; do
  if git ls-files --error-unmatch "$file" >/dev/null 2>&1; then
    require_file "$file"
    require_literal "$file" 'x86_64-microkernel-cli-http-operator-parity' \
      "tracked route docs must reference the active packet"
  fi
done

if git ls-files --error-unmatch aidocs/019_runtime_plan_pointer_todo_2026-04-18.md >/dev/null 2>&1; then
  require_literal "aidocs/019_runtime_plan_pointer_todo_2026-04-18.md" 'changes/__archived_changes_2026-05-31/x86_64-microkernel-cli-http-operator-parity' \
    "tracked runtime plan pointer must route the archived packet"
fi

echo "x86_64 microkernel CLI/HTTP operator parity contract OK"
