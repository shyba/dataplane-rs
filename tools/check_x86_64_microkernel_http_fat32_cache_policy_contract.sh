#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
kernel="crates/dataplane-x86_64-microkernel-smoke/src/kernel.rs"
kernel_text="crates/dataplane-x86_64-microkernel-smoke/src/kernel_text.rs"
fat32="crates/dataplane-x86_64-microkernel-smoke/src/fat32.rs"
http="crates/dataplane-x86_64-microkernel-smoke/src/http.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
makefile="Makefile"
frontier="aidocs/050_microkernel_robust_design_frontier_2026-05-30.md"
plan="aidocs/055_microkernel_tls_deferred_robust_appliance_plan_2026-05-31.md"
diary="aidocs/058_x86_64_microkernel_main_split_diary_2026-06-01.md"

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

for file in "$main" "$kernel" "$kernel_text" "$fat32" "$http" "$runner" "$makefile" "$frontier" "$plan" "$diary"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel HTTP/FAT32 Cache Policy Contract Guard ==="

for literal in \
  'const FAT32_INDEX_PATH: &str = "/INDEX.HTM";' \
  'const FAT32_CHAIN_PATH: &str = "/CHAIN.HTM";'; do
  require_literal "$fat32" "$literal" "guest source must contain moved FAT32 path literal: $literal"
done

for literal in \
  'const HTTP_CHAIN_FILE_BYTES: usize = 1024;'; do
  require_literal "$kernel" "$literal" "guest source must contain cache-policy literal: $literal"
done

for literal in \
  'storage_service_counters_read_only=true' \
  'storage_service_counters_write_rejection_evidence=DPCLI:FS-ERR,DPMK:FS-NEGATIVE-OK' \
  'storage_service_counters_cache_scratch_counters=absent' \
  'storage_service_counters_filesystem_error_classes=unsupported-path,long-filename,unsupported-write-shape'; do
  require_literal "$runner" "$literal" "runner must contain cache-policy literal: $literal"
done

for literal in \
  'self.build_ok_response(files.index, true, HttpResponseKind::GetIndex, out)' \
  'self.build_ok_response(files.large, false, HttpResponseKind::GetLarge, out)' \
  'self.build_ok_response(files.chain, false, HttpResponseKind::GetChain, out)' \
  'self.build_ok_response(files.index, false, HttpResponseKind::HeadIndex, out)' \
  'self.build_ok_response(files.large, true, HttpResponseKind::GetLarge, out)' \
  'self.build_ok_response(files.chain, true, HttpResponseKind::GetChain, out)' \
  'fn build_ok_response(' \
  'append_decimal_bytes(out, len, GENERATION_ID)?;' \
  'append_decimal_bytes(out, len, content_len)?;'; do
  require_literal "$http" "$literal" "guest source must contain cache-policy literal: $literal"
done

require_literal "$kernel_text" "pub(crate) fn append_decimal_bytes(" \
  "text helper owner must define decimal byte appending"
require_literal "$kernel_text" "pub(crate) fn append_bytes(" \
  "text helper owner must define byte appending"

for literal in \
  'storage_service_counters_proof_kind=source_scan' \
  'storage_service_counters_read_only=true' \
  'storage_service_counters_write_rejection_evidence=DPCLI:FS-ERR,DPMK:FS-NEGATIVE-OK' \
  'storage_service_counters_write_proof_gate=fat32-write-proof' \
  'storage_service_counters_cache_scratch_counters=absent' \
  'storage_service_counters_filesystem_error_classes=unsupported-path,long-filename,unsupported-write-shape' \
  'storage_service_counters_artifacts=summary,source_scan'; do
  require_literal "$runner" "$literal" "runner must contain cache-policy evidence literal: $literal"
done

for literal in \
  'x86_64-microkernel-http-fat32-cache-policy-contract:' \
  './tools/check_x86_64_microkernel_http_fat32_cache_policy_contract.sh' \
  'x86_64-microkernel-http-fat32-cache-policy: guard-scripts-executable x86_64-microkernel-fat32-contract x86_64-microkernel-storage-service-counters-contract x86_64-microkernel-filesystem-read-matrix-contract x86_64-microkernel-http-static-appliance-polish-contract x86_64-microkernel-http-fat32-cache-policy-contract'; do
  require_literal "$makefile" "$literal" "Makefile must expose cache-policy literal: $literal"
done

for literal in \
  'x86_64-microkernel-http-fat32-cache-policy' \
  'Status: complete as of 2026-05-31.' \
  'should remain direct service reads' \
  'no heap cache, no LRU map, no broad filesystem write' \
  'guarded no-cache/direct-read contract'; do
  require_literal "$plan" "$literal" "plan must preserve cache-policy decision literal: $literal"
done

require_literal "$frontier" '`x86_64-microkernel-http-fat32-cache-policy` is complete as a guarded' \
  "frontier must record cache-policy closeout"
require_literal "$diary" 'If not adding a cache, leave a guard-readable no-cache decision with evidence' \
  "diary must preserve the no-cache decision branch"
require_literal "$diary" 'Do not add a cache now.' \
  "diary must record the no-cache closeout decision"

reject_regex "$main" 'extern crate alloc|Vec<|String::|Box<|BTreeMap|HashMap|LruCache|lru_cache|LinkedHashMap|cache[[:space:]_-]*invalidation|cache[[:space:]_-]*invalidate|write[[:space:]_-]*invalidate|OpenAPI|serde_json|serde::|DPMK:TLS|HTTPS-|https://|openssl|rustls|embedded-tls|webpki|aws-lc-rs' \
  "cache-policy packet must not introduce heap caches, schema frameworks, TLS, or write-invalidation claims"
reject_regex "$runner" 'curl[[:space:]]+-k|https://|openssl|rustls|embedded-tls|webpki|aws-lc-rs|DPMK:TLS|HTTPS-|STRICT_FIVE_CALIBRATION|STRICT_FIVE_|run_strict_five|strict-five' \
  "cache-policy packet must not reopen TLS or retune strict-five benchmarks"

echo "x86_64 microkernel HTTP/FAT32 cache policy contract OK"
