#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

makefile="Makefile"
proposal="changes/x86_64-microkernel-embedded-shared-surface-refresh/proposal.md"
spec="changes/x86_64-microkernel-embedded-shared-surface-refresh/specs/embedded-shared-surface-refresh/spec.md"
tasks="changes/x86_64-microkernel-embedded-shared-surface-refresh/tasks.md"
core_lib="crates/dataplane-microkernel-core/src/lib.rs"
rp2040_noalloc="crates/dataplane-core-reactor/src/noalloc_primitives.rs"
x86_manifest="crates/dataplane-x86_64-microkernel-smoke/Cargo.toml"

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
  rg -Fq -- "$literal" "$file" || fail "$note"
}

reject_regex() {
  local file="$1"
  local regex="$2"
  local note="$3"
  local tmp status
  tmp="$(mktemp)"
  set +e
  rg -n -- "$regex" "$file" >"$tmp" 2>&1
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    cat "$tmp" >&2
    rm -f "$tmp"
    fail "$note"
  fi
  if [[ "$status" -ne 1 ]]; then
    cat "$tmp" >&2
    rm -f "$tmp"
    fail "could not scan $file for forbidden regex: $regex"
  fi
  rm -f "$tmp"
}

for file in "$makefile" "$proposal" "$spec" "$tasks" "$core_lib" "$rp2040_noalloc" "$x86_manifest"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel Embedded Shared Surface Refresh Contract ==="

# OpenSpec shape check: fail closed if the packet loses SHALL/MUST language.
awk '
  BEGIN { saw_shall = 0; saw_must = 0; saw_requirement = 0 }
  /^### Requirement:/ { saw_requirement = 1 }
  /SHALL/ { saw_shall = 1 }
  /MUST/ { saw_must = 1 }
  END {
    if (!saw_requirement || !saw_shall || !saw_must) {
      exit 42
    }
  }
' "$spec" || fail "OpenSpec SHALL/MUST awk check failed for $spec"

# The packet must stay rooted in the concrete source-owned shared surfaces.
require_literal "$proposal" 'crates/dataplane-microkernel-core' \
  "proposal must name the shared microkernel core crate"
require_literal "$proposal" 'crates/dataplane-core-reactor/src/noalloc_primitives.rs' \
  "proposal must name the no-alloc primitives source file"
require_literal "$spec" 'NetworkFrameDescriptor' \
  "spec must pin the shared network-frame type"
require_literal "$spec" 'MessageBody' \
  "spec must pin the shared message-body type"
require_literal "$spec" 'NativeHotTaskBudget' \
  "spec must pin the shared no-alloc scheduler type"
require_literal "$spec" 'FixedLocalExecCounts' \
  "spec must pin the fixed-capacity scheduler count type"
require_literal "$tasks" 'TMPDIR=/home/user/mnt/dataplane/tmp make embedded-ci' \
  "tasks must record the exact embedded-ci evidence command"
require_literal "$tasks" 'TMPDIR=/home/user/mnt/dataplane/tmp make rp2040-check' \
  "tasks must record the exact rp2040-check evidence command"
require_literal "$makefile" 'x86_64-microkernel-embedded-shared-surface-refresh-contract:' \
  "Makefile must expose the new contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_embedded_shared_surface_refresh_contract.sh' \
  "Makefile contract target must invoke the new guard"
require_literal "$makefile" 'x86_64-microkernel-embedded-shared-surface-refresh: guard-scripts-executable x86_64-microkernel-embedded-shared-surface-refresh-contract' \
  "Makefile must expose the refresh proof target"
require_literal "$makefile" 'embedded-ci: embedded-validation qemu-cortexm0-smoke qemu-cortexm0-uart-sessions rp2040-check' \
  "Makefile must keep the hosted embedded gate wired through rp2040-check"
require_literal "$makefile" 'rp2040-check: guard-scripts-executable' \
  "Makefile must keep the RP2040 check packet visible"
require_literal "$makefile" '$(MAKE) rp2040-core-check' \
  "Makefile must keep the RP2040 core subcheck visible"
require_literal "$makefile" '$(MAKE) rp2040-noalloc-smoke-size' \
  "Makefile must keep the RP2040 no-alloc smoke subcheck visible"
require_literal "$x86_manifest" 'dataplane-microkernel-core = { path = "../dataplane-microkernel-core" }' \
  "x86_64 smoke must depend on the shared microkernel core crate"

# Keep the x86_64 compatibility boundary concrete.
require_literal "$core_lib" '#![no_std]' \
  "shared microkernel core must remain no_std"
require_literal "$core_lib" 'pub const MESSAGE_INLINE_BYTES: usize = 32;' \
  "shared microkernel core must keep the fixed inline message surface"
require_literal "$core_lib" 'pub const NETWORK_FRAME_MAX_BYTES: usize = 1522;' \
  "shared microkernel core must keep the fixed network frame surface"
require_literal "$rp2040_noalloc" 'pub struct FixedLocalExecCounts' \
  "rp2040 no-alloc surface must expose fixed-capacity execution counts"
require_literal "$rp2040_noalloc" 'pub struct NativeHotTaskBudget' \
  "rp2040 no-alloc surface must expose the hot-task budget type"

# No overclaims: the packet is not about hardware readiness, TLS, or generic sockets.
for file in "$proposal" "$spec" "$tasks" "$makefile"; do
  reject_regex "$file" 'hardware-ready|silicon-ready' \
    "packet must not claim hardware readiness"
  reject_regex "$file" 'wifi|Ethernet driver|USB networking|flash driver' \
    "packet must not widen into unrelated device claims"
done

echo "x86_64 microkernel embedded shared surface refresh contract OK"
