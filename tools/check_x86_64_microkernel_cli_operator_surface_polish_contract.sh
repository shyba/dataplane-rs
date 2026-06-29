#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

guest="crates/dataplane-x86_64-microkernel-smoke/src/cli.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
makefile="Makefile"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_file() {
  [[ -f "$1" ]] || fail "missing required file: $1"
}

require_literal() {
  local file="$1"
  local needle="$2"
  local note="$3"
  rg -Fq -- "$needle" "$file" || fail "$note"
}

rust_fn_body() {
  local file="$1"
  local name="$2"
  local body status
  set +e
  body="$(awk -v target="fn ${name}(" '
    BEGIN { depth = 0; found = 0; started = 0; seen_open = 0 }
    !started && index($0, target) { started = 1; found = 1 }
    started {
      print
      line = $0
      opens = gsub(/\{/, "{", line)
      seen_open += opens
      line = $0
      closes = gsub(/\}/, "}", line)
      depth += opens - closes
      if (seen_open && depth == 0) { exit 0 }
    }
    END { if (!found || !seen_open || depth != 0) { exit 42 } }
  ' "$file")"
  status=$?
  set -e
  [[ "$status" -eq 0 ]] || fail "could not extract complete fn ${name} from $file"
  printf '%s\n' "$body"
}

require_fn_literal() {
  local body
  body="$(rust_fn_body "$1" "$2")"
  [[ "$body" == *"$3"* ]] || fail "$4"
}

reject_fn_literal() {
  local body
  body="$(rust_fn_body "$1" "$2")"
  if [[ "$body" == *"$3"* ]]; then
    fail "$4"
  fi
}

python_list_body() {
  local file="$1"
  local name="$2"
  local body status
  set +e
  body="$(awk -v target="${name} = [" '
    BEGIN { found = 0; started = 0; depth = 0 }
    !started && index($0, target) { started = 1; found = 1 }
    started {
      print
      line = $0
      opens = gsub(/\[/, "[", line)
      line = $0
      closes = gsub(/\]/, "]", line)
      depth += opens - closes
      if (depth == 0) { exit 0 }
    }
    END { if (!found || depth != 0) { exit 42 } }
  ' "$file")"
  status=$?
  set -e
  [[ "$status" -eq 0 ]] || fail "could not extract complete ${name} list from $file"
  printf '%s\n' "$body"
}

require_list_literal() {
  local body
  body="$(python_list_body "$1" "$2")"
  [[ "$body" == *"$3"* ]] || fail "$4"
}

echo "=== x86_64 Microkernel CLI Operator Surface Polish Contract Guard ==="

require_file "$guest"
require_file "$runner"
require_file "$makefile"

require_literal "$runner" "DPBOUNDS:cli line=64 commands=24" \
  "runner must use the fixed 24-command CLI proof table"

for command in \
  "help" \
  "tasks" \
  "fs ls /" \
  "fs cat /HELLO.TXT" \
  "fs cat /INDEX.HTM" \
  "fs stat /HELLO.TXT" \
  "fs stat /INDEX.HTM" \
  "fs stat /MISSING.TXT" \
  "fs stat /THISNAMEISTOOLONG.TXT" \
  "fs write /OUT.TXT append" \
  "task timer" \
  "task fs" \
  "task block" \
  "task tcpip" \
  "queues" \
  "parity" \
  "shell" \
  "json status" \
  "restart" \
  "reset" \
  "raw memory" \
  "page table dump" \
  "mmio dump" \
  "debug"; do
  require_list_literal "$runner" COMMAND_CHECKS "$command" \
    "runner must keep fixed 24-command CLI proof entry: $command"
done
for marker in \
  "DPMK:FS-NEGATIVE-OK:LONG-FILENAME" \
  "DPMK:FS-NEGATIVE-OK:UNSUPPORTED-WRITE" \
  "REJECT:shell:cli-command" \
  "REJECT:json status:cli-command" \
  "REJECT:restart:cli-capability" \
  "REJECT:reset:cli-capability" \
  "REJECT:raw memory:cli-capability" \
  "REJECT:page table dump:cli-capability" \
  "REJECT:mmio dump:cli-capability" \
  "REJECT:debug:cli-capability"; do
  require_list_literal "$runner" COMMAND_CHECKS "$marker" \
    "runner must check CLI surface marker: $marker"
done

require_literal "$runner" "DP_MICROKERNEL_CLI_OPERATOR_SURFACE_POLISH_PROOF" \
  "runner must expose the focused CLI polish env proof"
require_literal "$runner" "--cli-operator-surface-polish-proof" \
  "runner must expose the focused CLI polish mode"
require_literal "$runner" "cli_operator_surface_polish_summary_status=pass" \
  "runner must write a focused CLI polish summary"
require_literal "$runner" "cli_operator_surface_polish_tls_deferred=true" \
  "runner must keep TLS explicitly deferred in the focused summary"
require_literal "$makefile" "x86_64-microkernel-cli-operator-surface-polish-contract:" \
  "Makefile must expose the CLI polish contract target"
require_literal "$makefile" "x86_64-microkernel-cli-operator-surface-polish:" \
  "Makefile must expose the CLI polish proof target"

for forbidden in \
  "debugger" \
  "page-table" \
  "task stack" \
  "restart fs" \
  "fault fs" \
  "fault block" \
  "memdump" \
  "readmem" \
  "writemem" \
  "peek" \
  "poke" \
  "driver" \
  "pci" \
  "page-table" \
  "pagetable" \
  "cr3" \
  "pte" \
  "ioport" \
  "registers" \
  "TLS" \
  "HTTPS" \
  "OpenSSL" \
  "certificate"; do
  reject_fn_literal "$guest" parse_cli_command "$forbidden" \
    "CLI parser must not add forbidden operator command: $forbidden"
done

echo "x86_64 microkernel CLI operator surface polish contract guard passed."
