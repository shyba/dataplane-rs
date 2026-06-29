#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
matrix_runner="tools/x86_64_microkernel_validation_matrix_run.sh"
matrix_guard="tools/check_x86_64_microkernel_validation_matrix_contract.sh"
makefile="Makefile"
frontier="aidocs/050_microkernel_robust_design_frontier_2026-05-30.md"
plan="aidocs/051_microkernel_pre_tls_appliance_plan_2026-05-30.md"

require_file() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    echo "missing required file: $path" >&2
    exit 1
  fi
}

require_literal() {
  local path="$1"
  local literal="$2"
  local message="$3"
  if ! rg -Fq -- "$literal" "$path"; then
    echo "$message" >&2
    echo "missing literal in $path: $literal" >&2
    exit 1
  fi
}

reject_regex() {
  local path="$1"
  local regex="$2"
  local message="$3"
  local tmp
  tmp="$(mktemp)"
  set +e
  rg -n -- "$regex" "$path" >"$tmp" 2>&1
  local status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    echo "$message" >&2
    cat "$tmp" >&2
    rm -f "$tmp"
    exit 1
  fi
  if [[ "$status" -ne 1 ]]; then
    echo "rg failed while scanning $path for $regex" >&2
    cat "$tmp" >&2
    rm -f "$tmp"
    exit 1
  fi
  rm -f "$tmp"
}

echo "=== x86_64 Microkernel FS Policy And Directory Slice Contract Guard ==="

for path in "$main" "$runner" "$matrix_runner" "$matrix_guard" "$makefile" "$frontier" "$plan"; do
  require_file "$path"
done

require_literal "$plan" "### Seed A: Filesystem Policy And Directory Slice" \
  "pre-TLS plan must keep the filesystem policy seed"
require_literal "$plan" '`x86_64-microkernel-fs-policy-and-directory-slice` | complete' \
  "pre-TLS plan must mark the filesystem policy packet complete after closeout"
require_literal "$frontier" "x86_64-microkernel-fs-policy-and-directory-slice" \
  "frontier must name the active filesystem policy packet"

for literal in \
  "struct FsRootListing" \
  "fn list_root(&self, root: RootDirectoryProof) -> Result<FsRootListing, FsError>" \
  "FsServiceRequest::Open { path }" \
  "FsServiceRequest::Stat { handle }" \
  "FsServiceRequest::ListRoot" \
  "FsServiceRequest::ReadAt {" \
  "fn validate_handle(&self, handle: FsFileHandle) -> Result<(), FsError>" \
  "fn open(&self, root: RootDirectoryProof, path: FsKnownPath) -> Result<FsFileHandle, FsError>" \
  "fn stat(&self, handle: FsFileHandle) -> Result<FsFileStat, FsError>" \
  "fn list_root(&self, root: RootDirectoryProof) -> Result<FsRootListing, FsError>" \
  "fn read_at(" \
  "        &self," \
  "        memory: &FsTaskMemory<'_>," \
  "        handle: FsFileHandle," \
  "        offset: u32," \
  "        len: u32," \
  "        out: &mut [u8]," \
  "    ) -> Result<FsReadResult, FsError>"; do
  require_literal "$main" "$literal" \
    "guest source must keep filesystem policy and directory-slice literal: $literal"
done
require_literal "$matrix_runner" 'fs-policy-and-directory-slice' \
  "validation matrix must contain filesystem policy and directory-slice scenario"
require_literal "$matrix_runner" "fs-policy-and-directory-slice) echo 'make x86_64-microkernel-fs-policy-and-directory-slice' ;;" \
  "validation matrix must dispatch filesystem policy and directory-slice through Make"
require_literal "$matrix_runner" "fs-policy-and-directory-slice) echo 'x86_64 microkernel fs policy and directory slice proof passed.' ;;" \
  "validation matrix must require filesystem policy and directory-slice marker"
require_literal "$matrix_runner" "fs-policy-and-directory-slice) echo 'fs policy and directory slice summary|client log|serial log|qemu log|network pcap' ;;" \
  "validation matrix must require filesystem policy and directory-slice artifacts"
require_literal "$matrix_guard" 'fs-policy-and-directory-slice' \
  "validation matrix guard must require filesystem policy and directory-slice scenario"

require_literal "$makefile" 'x86_64-microkernel-fs-policy-and-directory-slice-contract:' \
  "Makefile must expose the filesystem policy and directory-slice contract"
require_literal "$makefile" 'x86_64-microkernel-fs-policy-and-directory-slice: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-filesystem-read-matrix-contract x86_64-microkernel-fs-policy-and-directory-slice-contract' \
  "Makefile must expose the filesystem policy and directory-slice proof target"
require_literal "$makefile" 'x86_64-microkernel-fs-policy-and-directory-slice: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-filesystem-read-matrix-contract x86_64-microkernel-fs-policy-and-directory-slice-contract' \
  "Makefile must expose the filesystem policy and directory-slice proof target"

for path in "$main" "$matrix_runner"; do
  reject_regex "$path" 'HTTPS|OpenSSL|curl -k|rustls|embedded-tls|webpki|DPMK:TLS|--tls|--https' \
    "filesystem policy packet must not add TLS/HTTPS drift in $path"
  reject_regex "$path" 'LongFileNameSupport|Journal|CrashConsistency|DurabilityClaim|host filesystem|GenericFileSystem|std::fs' \
    "filesystem policy packet must not overclaim filesystem scope in $path"
done

echo "FS policy and directory slice contract guard passed."
