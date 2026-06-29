#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
matrix_runner="tools/x86_64_microkernel_validation_matrix_run.sh"
matrix_guard="tools/check_x86_64_microkernel_validation_matrix_contract.sh"
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

for file in "$main" "$runner" "$matrix_runner" "$matrix_guard" "$makefile"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel Filesystem Read Matrix Contract Guard ==="

require_literal "$makefile" 'x86_64-microkernel-filesystem-read-matrix-contract:' \
  "Makefile must expose the filesystem read matrix contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_filesystem_read_matrix_contract.sh' \
  "Makefile filesystem read matrix contract must run the focused guard"
require_literal "$makefile" 'x86_64-microkernel-filesystem-read-matrix: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-filesystem-read-matrix-contract' \
  "Makefile filesystem read matrix proof target must preserve neighboring contract coverage"

require_literal "$matrix_runner" 'filesystem-read-matrix' \
  "validation matrix must route the filesystem read matrix scenario"
require_literal "$matrix_runner" "filesystem-read-matrix) echo 'make x86_64-microkernel-filesystem-read-matrix' ;;" \
  "validation matrix must dispatch filesystem read matrix through Make"
require_literal "$matrix_runner" "filesystem-read-matrix) echo 'x86_64 microkernel filesystem read matrix proof passed.' ;;" \
  "validation matrix must require the filesystem read matrix marker"
require_literal "$matrix_runner" "filesystem-read-matrix) echo 'filesystem read matrix summary|client log|serial log|qemu log|network pcap' ;;" \
  "validation matrix must require filesystem read matrix artifacts"

require_literal "$matrix_guard" 'filesystem-read-matrix' \
  "validation-matrix guard must cover filesystem read matrix scenario"

reject_regex "$main" 'extern crate alloc|Vec<|String::|Box<|BTreeMap|HashMap|std::fs|host[[:space:]_-]*filesystem|LongFileNameSupport|CrashConsistency|DurabilityClaim|GenericSocket|SocketApi|DPMK:TLS|HTTPS-' \
  "filesystem read matrix packet must stay no-alloc, target-local, non-socket, non-TLS, and non-durable"
reject_regex "$runner" 'curl[[:space:]]+-k|https://|openssl|rustls|embedded-tls|webpki|aws-lc-rs|DPMK:TLS|HTTPS-|STRICT_FIVE_CALIBRATION|benchmark[[:space:]]+tuning|retune[[:space:]]+benchmarks|retuning[[:space:]]+benchmarks' \
  "filesystem read matrix packet must not reopen TLS or tune benchmarks"

echo "x86_64 microkernel filesystem read matrix contract OK"
