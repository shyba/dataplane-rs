#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

kernel="crates/dataplane-x86_64-microkernel-smoke/src/kernel.rs"
scenarios="crates/dataplane-x86_64-microkernel-smoke/src/scenarios/common.rs"
fat32="crates/dataplane-x86_64-microkernel-smoke/src/fat32.rs"
http="crates/dataplane-x86_64-microkernel-smoke/src/http.rs"
services="crates/dataplane-x86_64-microkernel-smoke/src/services.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
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

reject_literal() {
  local file="$1"
  local literal="$2"
  local note="$3"
  if grep -Fq -- "$literal" "$file"; then
    fail "$note"
  fi
}

echo "=== x86_64 Microkernel FS Service Boundary Contract Guard ==="

for file in "$kernel" "$scenarios" "$fat32" "$http" "$services" "$runner" "$makefile"; do
  require_file "$file"
done

require_literal "$fat32" 'const FAT32_CHAIN_PATH: &str = "/CHAIN.HTM";' \
  "guest source must define the chain path"
require_literal "$fat32" 'const FAT32_CHAIN_NAME: [u8; 11] = *b"CHAIN   HTM";' \
  "guest source must define the chain short name"
require_literal "$fat32" 'const FAT32_CHAIN_CONTENT_BYTES: u32 = 834;' \
  "guest source must define the chain content length"
require_literal "$fat32" 'FsKnownPath::Chain' \
  "guest source must route Chain through FsKnownPath"
require_literal "$http" 'HttpTarget::Chain' \
  "guest source must route Chain through HTTP parsing"
require_literal "$http" 'HttpResponseKind::GetChain' \
  "guest source must expose a distinct chain response kind"
require_literal "$scenarios" 'self.load_http_file_read_at(root, chain_size, chain_buffer)?;' \
  "guest source must load the chain file through ReadAt"
reject_literal "$scenarios" 'self.load_http_file_chain(layout, root.chain_cluster, chain_size, chain_buffer)?;' \
  "guest source must not preload CHAIN.HTM outside ReadAt"
require_literal "$kernel" '.read_at(layout, &mut read_sector, memory, handle, offset, len, read_out)' \
  "guest source must keep read_at behind the service boundary"
require_literal "$fat32" 'if len > out.len() {' \
  "guest read_at must not reject multi-cluster lengths up front"
require_literal "$scenarios" 'DPMK:FS-SERVICE-MULTI-CLUSTER-OK' \
  "guest serial proof marker must remain present"
require_literal "$scenarios" 'DPMK:FS-SERVICE-READAT-OK' \
  "guest serial proof marker must remain present"
require_literal "$scenarios" 'DPMK:FS-SERVICE-READAT-CHAIN-OK' \
  "guest serial proof marker must prove CHAIN.HTM used ReadAt"
require_literal "$scenarios" 'DPMK:FS-SERVICE-HTTP-OK' \
  "guest serial proof marker must remain present"
require_literal "$services" 'DPMK:FS-SERVICE-CLI-OK' \
  "guest serial proof marker must remain present"
require_literal "$scenarios" 'DPMK:FS-SERVICE-BOUNDARY-OK' \
  "guest serial proof marker must remain present"
require_literal "$runner" '--fs-service-boundary-proof' \
  "runner must dispatch the fs service boundary proof"
require_literal "$runner" 'fs_service_path="/CHAIN.HTM"' \
  "runner must request CHAIN.HTM"
require_literal "$runner" 'fs_service_min_body_bytes="${DP_MICROKERNEL_FS_SERVICE_MIN_BODY:-513}"' \
  "runner must enforce the minimum body size"
require_literal "$runner" 'dataplane-fs-service-boundary-multicluster-proof' \
  "runner must verify the real multi-cluster body marker"
require_literal "$runner" 'grep -q "DPMK:FS-SERVICE-MULTI-CLUSTER-OK" "$serial_log"' \
  "runner must require the multi-cluster serial marker"
require_literal "$runner" 'grep -q "DPMK:FS-SERVICE-READAT-OK" "$serial_log"' \
  "runner must require the readat serial marker"
require_literal "$runner" 'grep -q "DPMK:FS-SERVICE-READAT-CHAIN-OK" "$serial_log"' \
  "runner must require the chain ReadAt serial marker"
require_literal "$runner" 'grep -q "DPMK:FS-SERVICE-HTTP-OK" "$serial_log"' \
  "runner must require the HTTP serial marker"
require_literal "$runner" 'grep -q "DPMK:FS-SERVICE-CLI-OK" "$serial_log"' \
  "runner must require the CLI serial marker"
require_literal "$runner" 'grep -q "DPMK:FS-SERVICE-BOUNDARY-OK" "$serial_log"' \
  "runner must require the final boundary marker"
reject_literal "$runner" "printf 'fs service boundary summary\\n'" \
  "runner must not write placeholder summary evidence"
require_literal "$makefile" 'x86_64-microkernel-fs-service-boundary-contract:' \
  "Makefile must expose the fs service boundary contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_fs_service_boundary_contract.sh' \
  "Makefile must wire the fs service boundary contract guard"
require_literal "$makefile" 'x86_64-microkernel-fs-service-boundary: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-fs-service-boundary-contract' \
  "Makefile must gate the proof on the fs service boundary contract"

echo "x86_64 microkernel FS service boundary contract OK"
