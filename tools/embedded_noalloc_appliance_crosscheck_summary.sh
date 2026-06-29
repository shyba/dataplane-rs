#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

log_root="${DP_EMBEDDED_NOALLOC_LOG_ROOT:-/home/user/mnt/dataplane/logs}"
mkdir -p "$log_root"

run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
summary="$log_root/embedded-noalloc-appliance-crosscheck-$run_id.summary"
target="thumbv6m-none-eabi"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

if [[ "${DP_EMBEDDED_NOALLOC_GATES_PASSED:-}" != "1" ]]; then
  fail "summary must be written by make embedded-noalloc-appliance-crosscheck after embedded-ci passes"
fi

record_size() {
  local label="$1"
  local evidence_class="$2"
  local elf="$3"

  [[ -f "$elf" ]] || fail "expected built ELF is missing: $elf"
  if ! command -v arm-none-eabi-size >/dev/null 2>&1; then
    fail "arm-none-eabi-size is not available"
  fi

  local size_line
  size_line="$(arm-none-eabi-size "$elf" | tail -n 1)"
  [[ -n "$size_line" ]] || fail "empty size output for $elf"

  local text data bss dec hex filename
  read -r text data bss dec hex filename <<<"$size_line"
  [[ -n "${text:-}" && -n "${data:-}" && -n "${bss:-}" && -n "${dec:-}" && -n "${hex:-}" ]] ||
    fail "could not parse size output for $elf: $size_line"

  {
    echo "artifact=$label"
    echo "evidence_class=$evidence_class"
    echo "elf=$elf"
    echo "text=$text data=$data bss=$bss dec=$dec hex=$hex"
    echo ""
  } >>"$summary"
}

{
  echo "packet=embedded-noalloc-appliance-crosscheck"
  echo "repo=$repo_root"
  echo "run_id=$run_id"
  echo "target=$target"
  echo "hardware_ready=false"
  echo "hardware_ready_reason=compile_and_qemu_evidence_only"
  echo "embedded_ci_status=passed"
  echo "evidence_classes=rp2040-compile-noalloc,rp2040-compile-alloc,cortexm0-qemu-noalloc"
  echo "log_root=$log_root"
  echo ""
} >"$summary"

record_size \
  "rp2040-noalloc-smoke" \
  "rp2040-compile-noalloc" \
  "target/$target/release/dataplane-rp2040-noalloc-smoke"

record_size \
  "rp2040-alloc-smoke" \
  "rp2040-compile-alloc" \
  "target/$target/release/dataplane-rp2040-smoke"

record_size \
  "qemu-cortexm0-smoke" \
  "cortexm0-qemu-noalloc" \
  "target/$target/release/dataplane-qemu-cortexm0-smoke"

{
  echo "required_gates=make embedded-ci; make rp2040-check; make qemu-cortexm0-smoke; make qemu-cortexm0-uart-sessions"
  echo "status=complete"
} >>"$summary"

echo "Embedded no-alloc appliance crosscheck summary: $summary"
