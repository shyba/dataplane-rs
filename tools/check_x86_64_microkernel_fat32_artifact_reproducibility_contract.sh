#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

runner="tools/x86_64_microkernel_fat32_reproducibility.sh"
builder="tools/microkernel_make_fat32_image.py"
makefile="Makefile"
plan="aidocs/055_microkernel_tls_deferred_robust_appliance_plan_2026-05-31.md"
archive_root="changes/__archived_changes_2026-05-31/x86_64-microkernel-fat32-artifact-reproducibility"
archive_proposal="$archive_root/proposal.md"
archive_tasks="$archive_root/tasks.md"
route_docs=(
  "AGENTS.md"
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

reject_regex() {
  local file="$1"
  local regex="$2"
  local note="$3"
  local status
  set +e
  rg -n -- "$regex" "$file" >/tmp/dataplane-fat32-repro.$$
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    cat /tmp/dataplane-fat32-repro.$$
    rm -f /tmp/dataplane-fat32-repro.$$
    fail "$note"
  fi
  rm -f /tmp/dataplane-fat32-repro.$$
  [[ "$status" -eq 1 ]] || fail "could not scan $file for forbidden regex: $regex"
}

for file in "$runner" "$builder" "$makefile" "$plan"; do
  require_file "$file"
done

require_file "$archive_proposal"
require_file "$archive_tasks"

route_recorded=false
for route_doc in "${route_docs[@]}"; do
  if grep -Fq -- "$archive_root" "$route_doc"; then
    route_recorded=true
    break
  fi
done
[[ "$route_recorded" == true ]] || fail "routing docs must record the archived FAT32 artifact reproducibility path"

require_literal "$archive_tasks" '## Closeout Evidence' \
  "archived FAT32 artifact reproducibility tasks must include closeout evidence"
require_literal "$archive_tasks" 'make x86_64-microkernel-fat32-artifact-reproducibility-contract` passed.' \
  "archived FAT32 artifact reproducibility tasks must record the contract closeout"
require_literal "$archive_tasks" 'TMPDIR=/home/user/mnt/dataplane/tmp make x86_64-microkernel-fat32-artifact-reproducibility` passed and wrote ' \
  "archived FAT32 artifact reproducibility tasks must record the reproducibility closeout"
require_literal "$archive_proposal" 'packet is the FAT32 artifact reproducibility work already reflected' \
  "archived FAT32 artifact reproducibility proposal must identify the archived packet"

echo "=== x86_64 Microkernel FAT32 Artifact Reproducibility Contract ==="

for literal in \
  'mnt_root="/home/user/mnt/dataplane"' \
  'python3 tools/microkernel_make_fat32_image.py "$image_a"' \
  'python3 tools/microkernel_make_fat32_image.py "$image_b"' \
  'cmp -s "$image_a" "$image_b"' \
  'fat32_artifact_reproducibility_summary_status=pass' \
  'fat32_artifact_reproducibility_image_sha256=' \
  'fat32_artifact_reproducibility_images_identical=true' \
  '"HELLO.TXT": b"hello from dataplane microkernel fat32' \
  '"INDEX.HTM": (' \
  '"LARGE.HTM": (' \
  '"CHAIN.HTM": (' \
  'fat32_artifact_reproducibility_{slug}_sha256=' \
  'fat32_artifact_reproducibility_repair_mode=false' \
  'fat32_artifact_reproducibility_default_writable=false'; do
  require_literal "$runner" "$literal" "repro runner must preserve literal: $literal"
done

for literal in \
  'TOTAL_SECTORS = 131_072' \
  'HELLO_NAME = b"HELLO   TXT"' \
  'INDEX_NAME = b"INDEX   HTM"' \
  'LARGE_NAME = b"LARGE   HTM"' \
  'CHAIN_NAME = b"CHAIN   HTM"' \
  'write_file(image, HELLO_CLUSTER, HELLO_CONTENT)' \
  'write_file(image, INDEX_CLUSTER, INDEX_CONTENT)' \
  'write_file(image, LARGE_CLUSTER, LARGE_CONTENT)' \
  'write_file_chain(image, (CHAIN_CLUSTER_FIRST, CHAIN_CLUSTER_SECOND), CHAIN_CONTENT)'; do
  require_literal "$builder" "$literal" "FAT32 image builder must preserve deterministic route literal: $literal"
done

require_literal "$makefile" 'x86_64-microkernel-fat32-artifact-reproducibility-contract:' \
  "Makefile must expose FAT32 artifact reproducibility contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_fat32_artifact_reproducibility_contract.sh' \
  "Makefile must run FAT32 artifact reproducibility guard"
require_literal "$makefile" 'x86_64-microkernel-fat32-artifact-reproducibility:' \
  "Makefile must expose FAT32 artifact reproducibility target"
require_literal "$makefile" './tools/x86_64_microkernel_fat32_reproducibility.sh' \
  "Makefile must run FAT32 artifact reproducibility proof"
require_literal "$plan" 'x86_64-microkernel-fat32-artifact-reproducibility' \
  "plan must keep the FAT32 artifact reproducibility packet"

reject_regex "$runner" 'fsck|mkfs|repair_mode=true|default_writable=true|writable_default=true' \
  "FAT32 reproducibility packet must not add repair or default-writable behavior"
reject_regex "$runner" 'https://|curl[[:space:]]+-k|OpenAPI|serde_json|serde::|JSON' \
  "FAT32 reproducibility packet must not drift into TLS or schema tooling"

echo "x86_64 microkernel FAT32 artifact reproducibility contract OK"
