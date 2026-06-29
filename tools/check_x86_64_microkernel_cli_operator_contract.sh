#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

guest_cli="crates/dataplane-x86_64-microkernel-smoke/src/cli.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
manifest="crates/dataplane-x86_64-microkernel-smoke/Cargo.toml"
workspace_manifest="Cargo.toml"
makefile="Makefile"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_file() {
  local file="$1"
  local note="$2"
  [[ -f "$file" ]] || fail "$note"
}

require_literal() {
  local file="$1"
  local needle="$2"
  local note="$3"
  rg -q --fixed-strings -- "$needle" "$file" || fail "$note"
}

reject_literal() {
  local file="$1"
  local needle="$2"
  local note="$3"
  local matches
  local status
  set +e
  matches="$(rg -n --fixed-strings -- "$needle" "$file")"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    printf '%s\n' "$matches"
    fail "$note"
  fi
  if [[ "$status" -ne 1 ]]; then
    fail "could not scan $file for forbidden literal: $needle"
  fi
}

rust_fn_body() {
  local file="$1"
  local name="$2"
  local body
  local status
  set +e
  body="$(awk -v target="fn ${name}(" '
    BEGIN { depth = 0; found = 0; started = 0; seen_open = 0 }
    !started && index($0, target) {
      started = 1
      found = 1
    }
    started {
      print
      line = $0
      opens = gsub(/\{/, "{", line)
      seen_open += opens
      line = $0
      closes = gsub(/\}/, "}", line)
      depth += opens - closes
      if (seen_open && depth == 0) {
        exit 0
      }
    }
    END {
      if (!found || !seen_open || depth != 0) {
        exit 42
      }
    }
  ' "$file")"
  status=$?
  set -e
  [[ "$status" -eq 0 ]] || fail "could not extract complete fn ${name} body from $file"
  printf '%s\n' "$body"
}

require_fn_literal() {
  local file="$1"
  local name="$2"
  local needle="$3"
  local note="$4"
  local body
  body="$(rust_fn_body "$file" "$name")"
  [[ "$body" == *"$needle"* ]] || fail "$note"
}

reject_fn_literal() {
  local file="$1"
  local name="$2"
  local needle="$3"
  local note="$4"
  local body
  body="$(rust_fn_body "$file" "$name")"
  if [[ "$body" == *"$needle"* ]]; then
    printf '%s\n' "$body" | rg -n --fixed-strings -- "$needle" || true
    fail "$note"
  fi
}

python_list_body() {
  local file="$1"
  local name="$2"
  local body
  local status
  set +e
  body="$(awk -v target="${name} = [" '
    BEGIN { found = 0; started = 0; depth = 0 }
    !started && index($0, target) {
      started = 1
      found = 1
    }
    started {
      print
      line = $0
      opens = gsub(/\[/, "[", line)
      line = $0
      closes = gsub(/\]/, "]", line)
      depth += opens - closes
      if (depth == 0) {
        exit 0
      }
    }
    END {
      if (!found || depth != 0) {
        exit 42
      }
    }
  ' "$file")"
  status=$?
  set -e
  [[ "$status" -eq 0 ]] || fail "could not extract complete ${name} list from $file"
  printf '%s\n' "$body"
}

require_list_literal() {
  local file="$1"
  local name="$2"
  local needle="$3"
  local note="$4"
  local body
  body="$(python_list_body "$file" "$name")"
  [[ "$body" == *"$needle"* ]] || fail "$note"
}

reject_list_literal() {
  local file="$1"
  local name="$2"
  local needle="$3"
  local note="$4"
  local body
  body="$(python_list_body "$file" "$name")"
  if [[ "$body" == *"$needle"* ]]; then
    printf '%s\n' "$body" | rg -n --fixed-strings -- "$needle" || true
    fail "$note"
  fi
}

require_file "$guest_cli" "missing x86_64 microkernel guest CLI source"
require_file "$runner" "missing x86_64 microkernel FAT32 runner"
require_file "$manifest" "missing x86_64 microkernel Cargo manifest"
require_file "$workspace_manifest" "missing workspace Cargo manifest"
require_file "$makefile" "missing Makefile"

echo "=== x86_64 Microkernel CLI Operator Contract Guard ==="

echo "Checking bounded capability table and read-only allowlist..."
for symbol in \
  "CLI_CAPABILITY_TABLE" \
  "CliCapability" \
  "CAP_CLI_HELP" \
  "CAP_CLI_TASKS" \
  "CAP_CLI_FS_LS" \
  "CAP_CLI_FS_CAT" \
  "CAP_CLI_FS_STAT" \
  "CAP_CLI_FS_NEGATIVE" \
  "CAP_CLI_TASK_TIMER" \
  "CAP_CLI_TASK_FS" \
  "CAP_CLI_TASK_BLOCK" \
  "CAP_CLI_TASK_TCPIP" \
  "CAP_CLI_QUEUES" \
  "authorize_cli_command" \
  "is_read_only_operator_command"; do
  require_literal "$guest_cli" "$symbol" "guest CLI must define bounded capability-table symbol: $symbol"
done

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
  "queues"; do
  require_fn_literal "$guest_cli" parse_cli_command "$command" \
    "guest CLI parser must recognize bounded CLI command: $command"
  require_list_literal "$runner" command_checks "$command" \
    "runner default command checks must send bounded CLI command: $command"
done

for command in \
  "debug" \
  "restart" \
  "reset" \
  "replay" \
  "raw memory" \
  "mmio dump" \
  "page table dump" \
  "wrong task"; do
  require_fn_literal "$guest_cli" parse_cli_command "$command" \
    "guest CLI parser must recognize bounded rejected command: $command"
done

for command in "task timer" "task tcpip" "queues"; do
  require_list_literal "$runner" fairness_command_checks "$command" \
    "runner fairness command checks must include representative operator command: $command"
done

echo "Checking deterministic operator output evidence..."
for output in \
  "DPCLI:HELP help tasks fs ls / fs cat /HELLO.TXT fs cat /INDEX.HTM" \
  "DPCLI:HELP-FS fs stat /HELLO.TXT fs stat /INDEX.HTM fs stat /MISSING.TXT fs stat /THISNAMEISTOOLONG.TXT fs write /OUT.TXT append" \
  "DPCLI:HELP-OPS task timer task fs task block task tcpip queues" \
  "DPCLI:TASKS timer=ready cli=ready fs=ready block=ready" \
  "DPCLI:TASK timer" \
  "DPCLI:TASK fs" \
  "DPCLI:TASK block" \
  "DPCLI:TASK tcpip" \
  "DPCLI:QUEUES" \
  "DPCLI:FS-ERR " \
  "DPMK:CLI-OPERATOR-OK"; do
  require_list_literal "$runner" command_checks "$output" \
    "runner default command checks must verify operator output marker: $output"
done
require_fn_literal "$guest_cli" dispatch_cli_command 'CliCommand::TaskTimer => self.cli_task_detail("timer", TASK_TIMER)' \
  "guest dispatch must route task timer to deterministic task-detail output"
require_fn_literal "$guest_cli" dispatch_cli_command 'CliCommand::TaskFs => self.cli_task_detail("fs", TASK_FS)' \
  "guest dispatch must route task fs to deterministic task-detail output"
require_fn_literal "$guest_cli" dispatch_cli_command 'CliCommand::TaskBlock => self.cli_task_detail("block", TASK_BLOCK)' \
  "guest dispatch must route task block to deterministic task-detail output"
require_fn_literal "$guest_cli" dispatch_cli_command 'CliCommand::TaskTcpip => self.cli_task_detail("tcpip", TASK_TCPIP)' \
  "guest dispatch must route task tcpip to deterministic task-detail output"
require_fn_literal "$guest_cli" cli_task_detail "DPCLI:TASK " \
  "guest task-detail helper must emit the task output prefix"
require_fn_literal "$guest_cli" cli_queues "DPCLI:QUEUES" \
  "guest queue helper must emit the queue output marker"
require_literal "$guest_cli" "DPMK:CLI-OPERATOR-OK" \
  "guest must emit the operator completion marker"

for output in "DPCLI:TASK timer" "DPCLI:TASK tcpip" "DPCLI:QUEUES"; do
  require_list_literal "$runner" fairness_command_checks "$output" \
    "runner fairness command checks must verify operator output marker: $output"
done

echo "Checking task detail is derived from TaskTable state..."
require_fn_literal "$guest_cli" cli_task_detail "self.tasks.get(" \
  "cli_task_detail must read task slots through TaskTable::get"
require_fn_literal "$guest_cli" cli_task_detail "slot.endpoint" \
  "cli_task_detail must print endpoint from the TaskTable slot"
require_fn_literal "$guest_cli" cli_task_detail "slot.status" \
  "cli_task_detail must print status from the TaskTable slot"
require_fn_literal "$guest_cli" cli_task_detail "slot.counters" \
  "cli_task_detail must print counters from the TaskTable slot"
require_fn_literal "$guest_cli" cli_task_detail "mailbox_depth" \
  "cli_task_detail must include bounded mailbox depth, not hard-coded queue output"
require_fn_literal "$guest_cli" mailbox_depth ".len()" \
  "mailbox_depth must read bounded Mailbox::len"
require_fn_literal "$guest_cli" cli_queues ".len()" \
  "cli_queues must derive queue output from bounded Mailbox::len"
require_fn_literal "$guest_cli" cli_queues "DPCLI:QUEUES" \
  "cli_queues must emit the deterministic queue summary marker"

require_fn_literal "$guest_cli" dispatch_cli_command "cli_command_capability" \
  "dispatcher must consult the fixed capability table"
require_fn_literal "$guest_cli" dispatch_cli_command "authorize_cli_command" \
  "dispatcher must authorize only read-only commands through the capability table"
require_fn_literal "$guest_cli" dispatch_cli_command 'Err("cli-forbidden")' \
  "dispatcher must reject debugger-style and wrong-task commands"

echo "Checking bounded rejection paths..."
require_fn_literal "$guest_cli" parse_cli_command 'b"page table dump" => Ok(CliCommand::DebuggerStyle(' \
  "guest parser must keep the bounded page-table-dump rejection arm explicit"
require_fn_literal "$guest_cli" parse_cli_command 'DebuggerStyleCommand::PageTableDump' \
  "guest parser must keep the bounded page-table-dump rejection command explicit"
for forbidden in \
  'CliCommand::DebuggerStyle(DebuggerStyleCommand::Debug)' \
  'CliCommand::DebuggerStyle(DebuggerStyleCommand::Restart)' \
  'CliCommand::DebuggerStyle(DebuggerStyleCommand::Reset)' \
  'CliCommand::DebuggerStyle(DebuggerStyleCommand::Replay)' \
  'CliCommand::DebuggerStyle(DebuggerStyleCommand::RawMemory)' \
  'CliCommand::DebuggerStyle(DebuggerStyleCommand::MmioDump)' \
  'CliCommand::Forbidden(ForbiddenCommand::WrongTask)' \
  'cli-forbidden'; do
  require_literal "$guest_cli" "$forbidden" "guest must keep bounded rejection path explicit: $forbidden"
done
require_fn_literal "$guest_cli" parse_cli_command 'b"page table dump" => Ok(CliCommand::DebuggerStyle(' \
  "guest parser must keep the bounded page-table-dump rejection arm explicit"
require_fn_literal "$guest_cli" parse_cli_command 'DebuggerStyleCommand::PageTableDump' \
  "guest parser must keep the bounded page-table-dump rejection command explicit"
for command in \
  "debug" \
  "restart" \
  "reset" \
  "replay" \
  "raw memory" \
  "mmio dump" \
  "page table dump" \
  "wrong task"; do
  require_fn_literal "$guest_cli" parse_cli_command "$command" \
    "guest parser must keep bounded rejected command: $command"
done

echo "Checking existing Stage K CLI evidence remains covered..."
for command in \
  "fs stat /HELLO.TXT" \
  "fs stat /INDEX.HTM" \
  "fs stat /MISSING.TXT" \
  "fs stat /THISNAMEISTOOLONG.TXT" \
  "fs write /OUT.TXT append"; do
  require_list_literal "$runner" command_checks "$command" \
    "runner must keep existing Stage K command check: $command"
  require_fn_literal "$guest_cli" parse_cli_command "$command" \
    "guest parser must keep existing Stage K command: $command"
done

echo "Checking CLI does not become a privileged debugger..."
for command in \
  "restart" \
  "reset" \
  "raw memory" \
  "mmio dump" \
  "page table dump"; do
  require_list_literal "$runner" command_checks "REJECT:${command}:cli-capability" \
    "runner default checks must keep debugger/operator-escape command rejected: $command"
done
require_list_literal "$runner" command_checks "REJECT:debug:cli-capability" \
  "runner default checks must keep debugger/operator-escape command rejected: debug"

for command in \
  "restart" \
  "reset" \
  "replay" \
  "raw memory" \
  "mmio dump" \
  "page table dump" \
  "wrong task"; do
  require_fn_literal "$guest_cli" parse_cli_command "$command" \
    "guest CLI parser must keep the bounded reject command explicit: $command"
  reject_list_literal "$runner" fairness_command_checks "$command" \
    "runner fairness checks must not promote debugger/operator-escape command: $command"
done

for command in \
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
  "registers"; do
  reject_fn_literal "$guest_cli" parse_cli_command "$command" \
    "guest CLI parser must not accept debugger/operator-escape command: $command"
  reject_list_literal "$runner" command_checks "$command" \
    "runner default checks must not promote debugger/operator-escape command: $command"
  reject_list_literal "$runner" fairness_command_checks "$command" \
    "runner fairness checks must not promote debugger/operator-escape command: $command"
done

require_fn_literal "$guest_cli" dispatch_cli_command 'CliCommand::DebuggerStyle(_) | CliCommand::Forbidden(_) => Err("cli-forbidden")' \
  "dispatcher must reject debugger-style and wrong-task commands"

for function in dispatch_cli_command cli_task_detail cli_queues; do
  for forbidden in \
    "protected_block_region" \
    "protected_net_region" \
    "BLOCK_TASK_REGION" \
    "NET_TASK_REGION" \
    "FS_TASK_REGION" \
    "BlockDriverTask" \
    "NetDriverTask" \
    "VirtioLegacyPciBlock" \
    "VirtioLegacyPciNet" \
    "submit_read_sector" \
    "read_sector" \
    "poll_receive_frame" \
    "transmit_frame" \
    "receive_raw_frame_for" \
    "&mut self.block_task" \
    "&mut self.net_task" \
    "self.block_task.driver" \
    "self.net_task.driver"; do
    reject_fn_literal "$guest_cli" "$function" "$forbidden" \
      "$function must not access driver internals or protected regions: $forbidden"
  done
done

reject_fn_literal "$guest_cli" parse_cli_command "fs stat /OUT.TXT" \
  "default CLI parser must not promote /OUT.TXT stat outside write-proof scope"
reject_list_literal "$runner" command_checks "fs stat /OUT.TXT" \
  "default CLI runner must not promote /OUT.TXT stat outside write-proof scope"
reject_list_literal "$runner" fairness_command_checks "fs stat /OUT.TXT" \
  "fairness CLI runner must not promote /OUT.TXT stat outside write-proof scope"

echo "Checking Stage L does not reopen TLS frontier..."
for forbidden in \
  "rustls" \
  "webpki" \
  "x509" \
  "certificate" \
  "entropy" \
  "DPHTTPS" \
  "DPMK:TLS" \
  "DPCLI:TLS"; do
  reject_literal "$manifest" "$forbidden" "microkernel smoke manifest must not add TLS dependency or marker: $forbidden"
  reject_literal "$workspace_manifest" "$forbidden" "workspace manifest must not add TLS dependency or marker: $forbidden"
  reject_literal "$guest_cli" "$forbidden" "guest must not add TLS dependency or marker: $forbidden"
  reject_literal "$runner" "$forbidden" "runner must not add TLS dependency or marker: $forbidden"
done
for forbidden in 'ring =' '"ring"'; do
  reject_literal "$manifest" "$forbidden" "microkernel smoke manifest must not add TLS dependency: $forbidden"
  reject_literal "$workspace_manifest" "$forbidden" "workspace manifest must not add TLS dependency: $forbidden"
done

echo "PASS: x86_64 microkernel CLI operator contract satisfied."
