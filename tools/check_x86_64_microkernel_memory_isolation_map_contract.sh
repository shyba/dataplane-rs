#!/usr/bin/env bash
set -euo pipefail

# x86_64 microkernel memory isolation contract.
#
# This guard asserts the source-real memory isolation shape that the guest
# actually implements and exercises under QEMU:
#   * per-task statically-placed memory regions (block / fs / net),
#   * protected accessor wrappers that gate every task/driver memory touch,
#   * MMU-coverage range checks for each region,
#   * a page-fault containment proof that deliberately faults a task page and
#     confirms the fault is caught (DPMK:FAULT-CONTAINED).
#
# Historical note: an earlier round shipped an additional declarative
# "memory isolation map" scaffolding layer (MemoryRegionClass enum,
# TASK_POLICIES, authorize_service_send, a focused probe, and a runner
# --memory-isolation-map-proof mode). That layer was lost when the untracked
# guest crate and runner were rewritten for the control-protocol-v1 slice. The
# underlying isolation primitives below survived and remain the real proof of
# memory isolation, so this guard pins those primitives at full strength rather
# than a regenerated scaffolding shape. Restoring the declarative map is a
# dedicated isolation-model packet, not part of this regression fix.

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
kernel="crates/dataplane-x86_64-microkernel-smoke/src/kernel.rs"
block_runtime="crates/dataplane-x86_64-microkernel-smoke/src/block_runtime.rs"
arch="crates/dataplane-x86_64-microkernel-smoke/src/arch.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
matrix_runner="tools/x86_64_microkernel_validation_matrix_run.sh"
matrix_guard="tools/check_x86_64_microkernel_validation_matrix_contract.sh"
makefile="Makefile"
fat32_guard="tools/check_x86_64_microkernel_fat32_contract.sh"
operator_guard="tools/check_x86_64_microkernel_cli_operator_contract.sh"
nontls_guard="tools/check_x86_64_microkernel_nontls_network_service_contract.sh"
service_ipc_guard="tools/check_x86_64_microkernel_service_ipc_audit_contract.sh"
scheduler_guard="tools/check_x86_64_microkernel_scheduler_fairness_load_contract.sh"

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

reject_regex() {
  local file="$1"
  local regex="$2"
  local note="$3"
  local matches status
  set +e
  matches="$(rg -n -- "$regex" "$file")"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    printf '%s\n' "$matches"
    fail "$note"
  fi
  [[ "$status" -eq 1 ]] || fail "could not scan $file for forbidden regex: $regex"
}

rust_fn_body() {
  local file="$1"
  local function="$2"
  local body status
  set +e
  body="$(awk -v target="fn ${function}" '
    BEGIN { depth = 0; found = 0; started = 0 }
    !started && index($0, target) {
      started = 1
      found = 1
    }
    started {
      print
      line = $0
      opens = gsub(/\{/, "{", line)
      line = $0
      closes = gsub(/\}/, "}", line)
      depth += opens - closes
      if (depth == 0 && started) {
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
  [[ "$status" -eq 0 ]] || fail "could not extract function body for ${function}"
  printf '%s\n' "$body"
}

require_fn_literal() {
  local file="$1"
  local function="$2"
  local needle="$3"
  local note="$4"
  local body
  body="$(rust_fn_body "$file" "$function")"
  [[ "$body" == *"$needle"* ]] || fail "$note"
}

require_literal_any() {
  local literal="$1"
  local note="$2"
  shift 2
  local file
  for file in "$@"; do
    if grep -Fq -- "$literal" "$file"; then
      return 0
    fi
  done
  fail "$note"
}

for file in \
  "$main" \
  "$kernel" \
  "$block_runtime" \
  "$arch" \
  "$runner" \
  "$matrix_runner" \
  "$matrix_guard" \
  "$makefile" \
  "$fat32_guard" \
  "$operator_guard" \
  "$nontls_guard" \
  "$service_ipc_guard" \
  "$scheduler_guard"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel Memory Isolation Contract Guard ==="

# Per-task memory regions must be distinct, statically placed, and typed.
for literal in \
  'struct TaskRegion(' \
  'struct FsRegion(' \
  'struct NetRegion(' \
  'static mut BLOCK_TASK_REGION: TaskRegion' \
  'static mut FS_TASK_REGION: FsRegion' \
  'static mut NET_TASK_REGION: NetRegion' \
  'fn protected_fs_region' \
  'fn protected_block_region' \
  'fn protected_net_region' \
  'fn fs_region_is_mmu_covered' \
  'fn block_region_is_mmu_covered' \
  'fn net_region_is_mmu_covered' \
  'fn prove_fault_containment() -> bool' \
  'fn page_fault_handler' \
  'static mut TASK_FAULT_SEEN: bool' \
  'fn trigger_task_fault'; do
  require_literal "$arch" "$literal" "arch source must contain memory isolation contract literal: $literal"
done

for literal in \
  'DPMK:FAULT-CONTAINED' \
  'DPMK:OK'; do
  require_literal "$kernel" "$literal" "guest source must contain memory isolation contract literal: $literal"
done

# Every region accessor must be exercised against its owning driver/task path.
require_literal_any 'protected_fs_region(|memory|' \
  "fs memory access must go through the protected fs region accessor" \
  "$kernel" crates/dataplane-x86_64-microkernel-smoke/src/services.rs \
  crates/dataplane-x86_64-microkernel-smoke/src/scenarios.rs
require_literal_any 'protected_block_region(|memory|' \
  "block memory access must go through the protected block region accessor" \
  "$kernel" crates/dataplane-x86_64-microkernel-smoke/src/virtio_block.rs
require_literal_any 'protected_net_region(|memory|' \
  "network memory access must go through the protected net region accessor" \
  "$kernel" crates/dataplane-x86_64-microkernel-smoke/src/virtio_net.rs \
  crates/dataplane-x86_64-microkernel-smoke/src/scenarios.rs

# The fault containment proof must deliberately fault an in-bounds task page and
# observe the fault, not merely range-check.
require_fn_literal "$arch" prove_fault_containment 'block_region_is_mmu_covered()' \
  "fault containment proof must confirm the faulted region is MMU-covered first"
require_fn_literal "$arch" prove_fault_containment 'protected_block_region(|_| {})' \
  "fault containment proof must exercise the protected accessor before faulting"
require_fn_literal "$arch" prove_fault_containment 'trigger_task_fault(block_region_addr())' \
  "fault containment proof must trigger a fault on the owned task page"
require_fn_literal "$arch" prove_fault_containment 'TASK_FAULT_SEEN' \
  "fault containment proof must observe the page fault was caught"

# The boot proof path must run containment and emit the contained marker.
require_literal "$kernel" 'if !prove_fault_containment() {' \
  "boot proof must gate on the fault containment result"

# MMU coverage checks must bound each region inside the identity-mapped window.
for literal in \
  'block_region_is_mmu_covered() || !fs_region_is_mmu_covered()'; do
  require_literal "$block_runtime" "$literal" \
    "protected accessors must reject regions outside MMU coverage: $literal"
done

# Isolation must stay capability/page based: no generic memory manager drift.
reject_regex "$kernel" 'generic[[:space:]]+memory[[:space:]]+manager|MemoryManager|memory_manager|alloc_page|free_page|map_any|mmap' \
  "memory isolation must not add a generic memory manager"

# x86_64-only isolation names must not leak into shared / no-alloc crates.
for shared in \
  crates/dataplane-microkernel-core/src/lib.rs \
  crates/dataplane-core-reactor/src/noalloc_primitives.rs \
  crates/dataplane-runtime/src/lib.rs; do
  if [[ -f "$shared" ]]; then
    reject_regex "$shared" 'virtio|pci|Pci|MMU|mmu::|page_fault|x86_64|port I/O|port_io' \
      "x86_64-only memory isolation names must not leak into shared/no-alloc file $shared"
  fi
done

# Evidence must stay under the mounted workspace, not /tmp or target/.
reject_regex "$runner" '/tmp/[^ ]*microkernel|target/[^ ]*\.summary' \
  "memory isolation evidence must stay under /home/user/mnt/dataplane in $runner"

echo "x86_64 microkernel memory isolation contract OK"
