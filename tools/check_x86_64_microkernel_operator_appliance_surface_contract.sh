#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

cli="crates/dataplane-x86_64-microkernel-smoke/src/cli.rs"
kernel="crates/dataplane-x86_64-microkernel-smoke/src/kernel.rs"
network_task="crates/dataplane-x86_64-microkernel-smoke/src/network_task/task.rs"
network_protocol="crates/dataplane-x86_64-microkernel-smoke/src/network_task/protocol_tcp_http.rs"
scenario="crates/dataplane-x86_64-microkernel-smoke/src/scenarios/cli_operator.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
manifest="crates/dataplane-x86_64-microkernel-smoke/Cargo.toml"
workspace_manifest="Cargo.toml"
makefile="Makefile"
matrix_runner="tools/x86_64_microkernel_validation_matrix_run.sh"

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

require_file "$cli" "missing x86_64 microkernel CLI source"
require_file "$kernel" "missing x86_64 microkernel kernel source"
require_file "$network_task" "missing x86_64 microkernel network task source"
require_file "$network_protocol" "missing x86_64 microkernel TCP protocol source"
require_file "$scenario" "missing x86_64 microkernel CLI operator scenario source"
require_file "$runner" "missing x86_64 microkernel FAT32 runner"
require_file "$manifest" "missing x86_64 microkernel Cargo manifest"
require_file "$workspace_manifest" "missing workspace Cargo manifest"
require_file "$makefile" "missing Makefile"
require_file "$matrix_runner" "missing validation matrix runner"

echo "=== x86_64 Microkernel Operator Appliance Surface Contract Guard ==="

echo "Checking bounded operator status surface emitters..."
for symbol in \
  "DPCLI:TASK " \
  "DPCLI:TASKS timer=ready cli=ready fs=ready block=ready" \
  "DPCLI:TASKS-NET net=candidate tcpip=candidate" \
  "DPCLI:TASKS-DHCP dhcp=bounded" \
  "DPCLI:QUEUES timer=" \
  "mailbox=" \
  "enqueued=" \
  "dequeued=" \
  "DPMK:CLI-OPERATOR-OK"; do
  require_literal "$cli" "$symbol" "guest must keep the bounded operator surface literal: $symbol"
done

echo "Checking operator appliance proof summary keys..."
require_literal "$kernel" 'DPCLI:STAT ' \
  "guest source must emit the filesystem stat prefix used by the source-scan summary"
require_literal "$kernel" 'readonly=1' \
  "guest source must emit the read-only filesystem field used by the source-scan summary"
require_literal "$network_task" 'too_many_sessions' \
  "guest source must track session overflow counts used by the source-scan summary"
require_literal "$network_task" 'active_sessions' \
  "guest source must track active session counts used by the source-scan summary"
require_literal "$network_protocol" 'active_sessions' \
  "guest source must track active session counts used by the source-scan summary"
require_literal "$scenario" 'DPMK:CLI-OPERATOR-OK' \
  "guest scenario must keep the operator appliance proof marker"
require_literal "$runner" '--operator-appliance-surface-proof' \
  "runner must dispatch operator appliance proof mode"
require_literal "$runner" 'operator_appliance_surface_proof_kind=source_scan' \
  "runner must mark the operator appliance proof as a source scan"
require_literal "$runner" 'operator_appliance_surface_read_only=true' \
  "runner must record the read-only operator surface"
require_literal "$runner" 'operator_appliance_surface_filesystem_metadata=DPCLI:STAT,readonly=1' \
  "runner must record the source-real filesystem metadata fields"
require_literal "$runner" 'operator_appliance_surface_network_session_counters=active_sessions,too_many_sessions' \
  "runner must record the source-real network session counters"
require_literal "$runner" 'operator_appliance_surface_sources=source_scan' \
  "runner must record the source-scan proof source"
require_literal "$runner" 'operator_appliance_surface_artifacts=summary,source_scan' \
  "runner must record the honest source-scan artifacts"
require_literal "$runner" 'x86_64 microkernel operator appliance source-scan proof passed.' \
  "runner must emit a distinct operator appliance source-scan success marker"

echo "Checking validation-matrix wiring..."
require_literal "$matrix_runner" "operator-appliance-surface) echo 'make x86_64-microkernel-operator-appliance-surface' ;;" \
  "matrix runner must dispatch the operator appliance scenario through Make"
require_literal "$matrix_runner" "operator-appliance-surface) echo 'x86_64 microkernel operator appliance source-scan proof passed.' ;;" \
  "matrix runner must require the operator appliance source-scan marker"
require_literal "$matrix_runner" "operator-appliance-surface) echo 'operator appliance surface summary|operator appliance surface source scan' ;;" \
  "matrix runner must require the source-scan artifacts"

echo "Checking Makefile wiring..."
require_literal "$makefile" 'x86_64-microkernel-operator-appliance-surface-contract:' \
  "Makefile must expose the operator appliance contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_operator_appliance_surface_contract.sh' \
  "Makefile operator appliance contract target must run the packet-specific guard"
require_literal "$makefile" 'x86_64-microkernel-operator-appliance-surface: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-operator-appliance-surface-contract' \
  "Makefile must expose the operator appliance proof target"

echo "Checking guarded source constraints..."
reject_literal "$cli" "DPMK:HTTP-STATUS-PAGE-OK" \
  "operator appliance surface guard must not claim an HTTP status page proof that the guest does not emit"
reject_literal "$kernel" "DPMK:HTTP-STATUS-PAGE-OK" \
  "operator appliance surface guard must not claim an HTTP status page proof that the guest does not emit"
reject_literal "$scenario" "DPMK:HTTP-STATUS-PAGE-OK" \
  "operator appliance surface guard must not claim an HTTP status page proof that the guest does not emit"
reject_literal "$runner" 'operator_appliance_surface_sources=cli_ipc,service_http' \
  "runner must not claim service_http sourcing after narrowing to source-scan proof"
reject_literal "$runner" 'operator_appliance_surface_artifacts=client_log,network_host_log,network_pcap,serial_log,qemu_log' \
  "runner must not claim execution artifacts after narrowing to source-scan proof"

echo "x86_64 microkernel operator appliance surface contract guard passed."
