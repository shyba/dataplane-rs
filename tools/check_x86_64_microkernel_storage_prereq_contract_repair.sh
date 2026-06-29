#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

spec="changes/x86_64-microkernel-storage-prereq-contract-repair/specs/storage-prereq-contract-repair/spec.md"
proposal="changes/x86_64-microkernel-storage-prereq-contract-repair/proposal.md"
design="changes/x86_64-microkernel-storage-prereq-contract-repair/design.md"
tasks="changes/x86_64-microkernel-storage-prereq-contract-repair/tasks.md"
makefile="Makefile"
active_run=".hermes-harness/active-run.json"
prereq_contracts=(
  "./tools/check_x86_64_microkernel_storage_durability_design_contract.sh"
  "./tools/check_x86_64_microkernel_fat32_integrity_readonly_contract.sh"
  "./tools/check_x86_64_microkernel_preallocated_journal_file_contract.sh"
  "./tools/check_x86_64_microkernel_fat32_service_errors_contract.sh"
  "./tools/check_x86_64_microkernel_fat32_directory_index_contract.sh"
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
  grep -Fq -- "$literal" "$file" || fail "missing literal in $file: $literal"
}

for file in "$spec" "$proposal" "$design" "$tasks" "$makefile" "$active_run" "${prereq_contracts[@]}"; do
  require_file "$file"
done

for literal in \
  'changes/__archived_changes_2026-06-01/x86_64-microkernel-fat32-service-errors/' \
  'changes/__archived_changes_2026-06-01/x86_64-microkernel-fat32-directory-index/' \
  'feature-gated proof/design only' \
  'no default write enablement' \
  'no crash-consistency claim' \
  'no broad write path' \
  'storage-write-reentry-review'; do
  require_literal "$spec" "$literal"
done

require_literal "$active_run" '"run_id": null' \
  "active run ledger must keep the packet root unlaunched until a real run exists"

for literal in \
  'archived roots' \
  'fresh source-backed evidence' \
  'feature-gated proof/design surface'; do
  require_literal "$proposal" "$literal"
  require_literal "$design" "$literal"
done

require_literal "$makefile" 'x86_64-microkernel-storage-prereq-contract-repair-contract:'
require_literal "$makefile" './tools/check_x86_64_microkernel_storage_prereq_contract_repair.sh'

for contract in "${prereq_contracts[@]}"; do
  "$contract" || fail "prerequisite contract failed: $contract"
done

echo "x86_64 microkernel storage prereq contract repair guard passed."
