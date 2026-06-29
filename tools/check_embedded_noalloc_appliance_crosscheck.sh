#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

echo "=== Embedded No-Alloc Appliance Crosscheck Contract ==="

makefile="Makefile"
plan="aidocs/051_microkernel_pre_tls_appliance_plan_2026-05-30.md"
summary_script="tools/embedded_noalloc_appliance_crosscheck_summary.sh"
core_lib="crates/dataplane-microkernel-core/src/lib.rs"
rp2040_noalloc="crates/dataplane-core-reactor/src/noalloc_primitives.rs"
runtime_lib="crates/dataplane-runtime/src/lib.rs"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_file() {
  local path="$1"
  [[ -f "$path" ]] || fail "required file missing: $path"
}

require_literal() {
  local path="$1"
  local literal="$2"
  local message="$3"
  rg -Fq -- "$literal" "$path" || fail "$message"
}

reject_regex() {
  local path="$1"
  local regex="$2"
  local message="$3"
  local tmp status
  tmp="$(mktemp)"
  set +e
  rg -n -- "$regex" "$path" >"$tmp" 2>&1
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    cat "$tmp" >&2
    rm -f "$tmp"
    fail "$message"
  fi
  if [[ "$status" -ne 1 ]]; then
    cat "$tmp" >&2
    rm -f "$tmp"
    fail "could not scan $path for forbidden regex: $regex"
  fi
  rm -f "$tmp"
}

scan_forbidden_literal() {
  local path="$1"
  local literal="$2"
  local note="$3"
  local tmp status
  tmp="$(mktemp)"
  set +e
  rg -n -F -- "$literal" "$path" >"$tmp" 2>&1
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
    fail "could not scan $path for forbidden literal: $literal"
  fi
  rm -f "$tmp"
}

for path in "$makefile" "$plan" "$summary_script" \
  "$core_lib" "$rp2040_noalloc" "$runtime_lib"; do
  require_file "$path"
done

require_literal "$core_lib" "#![no_std]" \
  "dataplane-microkernel-core must remain no_std"
require_literal "$core_lib" "#![forbid(unsafe_code)]" \
  "dataplane-microkernel-core must forbid unsafe code"
reject_regex "$core_lib" '(^|[^A-Za-z0-9_])(extern crate alloc|use alloc::|alloc::|Vec<|VecDeque<|String\b|Box<|Rc<|Arc<|format!|std::)' \
  "dataplane-microkernel-core must not expose heap-backed or std APIs"

reject_regex "$rp2040_noalloc" '(^|[^A-Za-z0-9_])(extern crate alloc|use alloc::|alloc::|Vec<|VecDeque<|String\b|Box<|Rc<|Arc<|format!|std::sync|std::vec|std::string)' \
  "noalloc_primitives.rs must remain no-alloc and no-std friendly"

require_literal "$runtime_lib" '#![cfg_attr(target_os = "none", no_std)]' \
  "dataplane-runtime must keep target_os=none no_std crate gating"
require_literal "$runtime_lib" 'any(feature = "noalloc", feature = "rp2040-noalloc")' \
  "dataplane-runtime must keep noalloc re-export feature-gated"
require_literal "$runtime_lib" 'target_os = "none"' \
  "dataplane-runtime must keep noalloc re-export target_os=none gated"
require_literal "$runtime_lib" 'pub mod noalloc' \
  "dataplane-runtime must expose the noalloc target surface"

shared_paths=(
  "crates/dataplane-microkernel-core/src"
  "crates/dataplane-core-reactor/src/noalloc_primitives.rs"
  "crates/dataplane-runtime/src/lib.rs"
)

for path in "${shared_paths[@]}"; do
  scan_forbidden_literal "$path" "virtio" \
    "shared no-alloc surface must not mention virtio"
  scan_forbidden_literal "$path" "Virtio" \
    "shared no-alloc surface must not mention Virtio"
  scan_forbidden_literal "$path" "PCI" \
    "shared no-alloc surface must not mention PCI"
  scan_forbidden_literal "$path" "MMU" \
    "shared no-alloc surface must not mention MMU"
  scan_forbidden_literal "$path" "FAT32" \
    "shared no-alloc surface must not mention FAT32"
  scan_forbidden_literal "$path" "qemu" \
    "shared no-alloc surface must not mention qemu"
  scan_forbidden_literal "$path" "QEMU" \
    "shared no-alloc surface must not mention QEMU"
  scan_forbidden_literal "$path" "hostfwd" \
    "shared no-alloc surface must not mention hostfwd"
  scan_forbidden_literal "$path" "x86_64" \
    "shared no-alloc surface must not mention x86_64"
  scan_forbidden_literal "$path" "aarch64" \
    "shared no-alloc surface must not mention aarch64"
  scan_forbidden_literal "$path" "raspi3b" \
    "shared no-alloc surface must not mention raspi3b"
  scan_forbidden_literal "$path" "RNDIS" \
    "shared no-alloc surface must not mention RNDIS"
  scan_forbidden_literal "$path" "usb-net" \
    "shared no-alloc surface must not mention usb-net"
  scan_forbidden_literal "$path" "pcap" \
    "shared no-alloc surface must not mention pcap"
  scan_forbidden_literal "$path" "curl" \
    "shared no-alloc surface must not mention curl"
  scan_forbidden_literal "$path" "io_uring" \
    "shared no-alloc surface must not mention io_uring"
  scan_forbidden_literal "$path" "/home/user/mnt" \
    "shared no-alloc surface must not mention host log paths"
done

require_literal "$makefile" "embedded-noalloc-appliance-crosscheck-contract:" \
  "Makefile must expose embedded no-alloc appliance contract target"
require_literal "$makefile" "./tools/check_embedded_noalloc_appliance_crosscheck.sh" \
  "Makefile contract target must run this guard"
require_literal "$makefile" "embedded-noalloc-appliance-crosscheck: guard-scripts-executable embedded-noalloc-appliance-crosscheck-contract embedded-ci" \
  "Makefile embedded-noalloc appliance target must run contract plus hosted embedded CI"
require_literal "$makefile" "DP_EMBEDDED_NOALLOC_GATES_PASSED=1 ./tools/embedded_noalloc_appliance_crosscheck_summary.sh" \
  "Makefile embedded-noalloc appliance target must write a size/evidence summary"

require_literal "$summary_script" 'log_root="${DP_EMBEDDED_NOALLOC_LOG_ROOT:-/home/user/mnt/dataplane/logs}"' \
  "summary script must default artifacts to /home/user/mnt/dataplane/logs"
require_literal "$summary_script" 'DP_EMBEDDED_NOALLOC_GATES_PASSED' \
  "summary script must reject standalone use that bypasses embedded-ci"
require_literal "$summary_script" "embedded_ci_status=passed" \
  "summary script must record that the embedded gate packet already passed"
require_literal "$summary_script" "rp2040-compile-noalloc" \
  "summary script must keep RP2040 no-alloc evidence class separate"
require_literal "$summary_script" "cortexm0-qemu-noalloc" \
  "summary script must keep Cortex-M0 QEMU evidence class separate"
require_literal "$summary_script" "hardware_ready=false" \
  "summary script must not claim hardware readiness from QEMU/compile evidence"
require_literal "$summary_script" "arm-none-eabi-size" \
  "summary script must record ELF size evidence"

require_literal "$plan" "## Phase G: Embedded/No-Alloc Crosscheck" \
  "pre-TLS plan must define Phase G"
require_literal "$plan" "no claiming RP2040 hardware readiness from QEMU alone" \
  "pre-TLS plan must keep the RP2040 hardware-readiness stop line"

echo ""
echo "Embedded no-alloc appliance crosscheck contract passed."
