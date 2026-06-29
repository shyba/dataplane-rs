#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
fat32="crates/dataplane-x86_64-microkernel-smoke/src/fat32.rs"
runner="tools/x86_64_microkernel_fat32_directory_index.sh"
makefile="Makefile"
spec="changes/__archived_changes_2026-06-01/x86_64-microkernel-fat32-directory-index/specs/fat32-directory-index/spec.md"
tasks="changes/__archived_changes_2026-06-01/x86_64-microkernel-fat32-directory-index/tasks.md"

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

reject_regex() {
  local file="$1"
  local regex="$2"
  if rg -n -- "$regex" "$file"; then
    fail "forbidden stop-line or overclaim in $file: $regex"
  fi
}

for file in "$main" "$fat32" "$runner" "$makefile" "$spec" "$tasks"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel FAT32 Directory Index Contract Guard ==="

for literal in \
  'const FAT32_DIRECTORY_INDEX_ENTRY_CAP: usize = 4;' \
  'const FAT32_DIRECTORY_INDEX_NAME_CAP: usize = 12;' \
  'const FAT32_DIRECTORY_INDEX_RESPONSE_CAP: usize = 192;' \
  'ListRoot,' \
  'directory_index_view('; do
  require_literal "$fat32" "$literal"
done

for literal in \
  'fn run_fat32_directory_index_proof(&mut self) -> Result<(), &' \
  'DPMK:FS-DIR-INDEX-OK' \
  'DPMK:FS-DIR-INDEX-HTTP-OK' \
  'DPMK:FS-DIR-INDEX-CLI-OK' \
  'DPFSIDX:rows count=4 cap=4 name_cap=12 response_cap=192 order=/HELLO.TXT,/INDEX.HTM,/LARGE.HTM,/CHAIN.HTM' \
  'DPFSIDX:unsupported_path=unsupported-path entry_cap=enforced name_cap=enforced response_cap=enforced deterministic=ok' \
  'DPFSIDX:counters list_root=2 entries=4 errors=1'; do
  require_literal "$main" "$literal"
done

if awk '
  /fn load_http_files/ { in_body=1 }
  in_body && /fn load_http_file_chain/ { in_body=0 }
  in_body && /fs_service_request\(FsServiceRequest::ListRoot/ { found=1 }
  END { exit found ? 0 : 1 }
' "$main"; then
  :
else
  fail "load_http_files must request directory/index data through FsTask ListRoot"
fi

if awk '
  /fn load_http_files/ { in_body=1 }
  in_body && /fn load_http_file_chain/ { in_body=0 }
  in_body && /load_root_directory|parse_root_directory|read_sector_for_fs/ { found=1 }
  END { exit found ? 0 : 1 }
' "$main"; then
  fail "load_http_files must not bypass FsTask for directory/index data"
fi

if awk '
  /fn cli_fs_ls_root/ { in_body=1 }
  in_body && /fn cli_fs_cat/ { in_body=0 }
  in_body && /fs_service_request\(FsServiceRequest::ListRoot/ { found=1 }
  END { exit found ? 0 : 1 }
' "$main"; then
  :
else
  fail "cli_fs_ls_root must request directory/index data through FsTask ListRoot"
fi

if awk '
  /fn cli_fs_ls_root/ { in_body=1 }
  in_body && /fn cli_fs_cat/ { in_body=0 }
  in_body && /load_root_directory|parse_root_directory|read_sector_for_fs/ { found=1 }
  END { exit found ? 0 : 1 }
' "$main"; then
  fail "cli_fs_ls_root must not bypass FsTask for directory/index data"
fi

if ! awk '
  /fn cli_fs_ls_root/ { in_body=1 }
  in_body && /fn cli_fs_cat/ { in_body=0 }
  in_body && /serial::write_str\("DPCLI:LS \/ HELLO\.TXT "/ { stage=1 }
  stage == 1 && /serial::write_decimal\(reply\.hello\.size\)/ { stage=2 }
  stage == 2 && /serial::write_str\(" INDEX\.HTM "/ { stage=3 }
  stage == 3 && /serial::write_decimal\(reply\.index\.size\)/ { stage=4 }
  stage == 4 && /serial::write_str\(" LARGE\.HTM "/ { stage=5 }
  stage == 5 && /serial::write_decimal\(reply\.large\.size\)/ { stage=6 }
  stage == 6 && /serial::write_str\(" CHAIN\.HTM "/ { stage=7 }
  stage == 7 && /serial::write_decimal\(reply\.chain\.size\)/ { found_order=1 }
  END { exit !found_order }
' "$main"; then
  fail "cli_fs_ls_root source must emit the four-row DPCLI:LS transcript in order"
fi

for literal in \
  'DP_MICROKERNEL_CURL_PROOF=1' \
  './tools/x86_64_microkernel_fat32_run.sh --curl-proof >"$curl_run_log" 2>&1' \
  'TMPDIR="$tmp_dir" \' \
  './tools/x86_64_microkernel_fat32_run.sh >"$cli_run_log" 2>&1' \
  'fat32_directory_index_summary_status=pass' \
  'fat32_directory_index_host_client_status=pass' \
  'fat32_directory_index_http_status=200' \
  'fat32_directory_index_cli_rows=HELLO.TXT,INDEX.HTM,LARGE.HTM,CHAIN.HTM' \
  'fat32_directory_index_cli_line=DPCLI:LS / HELLO.TXT 40 INDEX.HTM 110 LARGE.HTM 480 CHAIN.HTM 834' \
  'fat32_directory_index_order=/HELLO.TXT,/INDEX.HTM,/LARGE.HTM,/CHAIN.HTM' \
  'fat32_directory_index_entry_cap=4' \
  'fat32_directory_index_name_cap=12' \
  'fat32_directory_index_response_cap=192' \
  'fat32_directory_index_unsupported_path=unsupported-path' \
  'fat32_directory_index_http_cli_same_fs_task=true' \
  'fat32_directory_index_cache_policy=false'; do
  require_literal "$runner" "$literal"
done

for literal in \
  'x86_64-microkernel-fat32-directory-index-contract:' \
  './tools/check_x86_64_microkernel_fat32_directory_index_contract.sh' \
  'x86_64-microkernel-fat32-directory-index:' \
  './tools/x86_64_microkernel_fat32_directory_index.sh'; do
  require_literal "$makefile" "$literal"
done

for literal in \
  'Requirement: Bound FAT32 access behind FsTask' \
  'Requirement: Expose the fixed directory/index view' \
  'Requirement: Prove deterministic ordering and caps' \
  'Requirement: Keep HTTP and CLI evidence aligned' \
  'Requirement: Fail closed on helper bypass and stale evidence' \
  'Requirement: Enforce the stop line'; do
  require_literal "$spec" "$literal"
done

if rg -n 'FS-DIR-INDEX|DPFSIDX|directory_index' "$main" |
  rg -n '\bfsck\b|crash-consisten|journaling|page-table|raw memory|MMIO|TLS|HTTPS|certificate|entropy|generic socket|generic IPC|heap-backed mailbox|benchmark retun|arbitrary host filesystem|default write mode|long filename support|directory expansion|repair'; then
  fail "directory-index implementation contains stop-line scope"
fi

if ! awk '
  /DPFSIDX:rows count=4 cap=4 name_cap=12 response_cap=192 order=\/HELLO.TXT,\/INDEX.HTM,\/LARGE.HTM,\/CHAIN.HTM/ { found_order=1 }
  /DPCLI:LS \/ HELLO.TXT 40 INDEX\.HTM 110 LARGE\.HTM 480 CHAIN\.HTM 834/ { found_cli_line=1 }
  /fat32_directory_index_cli_rows=HELLO.TXT,INDEX.HTM,LARGE.HTM,CHAIN.HTM/ { found_cli_rows=1 }
  /DPFSIDX:row 2 path=\/LARGE\.HTM status=ok cluster=/ { found_large_row=1 }
  /DPFSIDX:row 3 path=\/CHAIN\.HTM status=ok cluster=/ { found_chain_row=1 }
  END { exit !(found_order && found_cli_line && found_cli_rows && found_large_row && found_chain_row) }
' "$main" "$runner"; then
  fail "directory-index CLI evidence must include LARGE.HTM and preserve the four-row order"
fi

for literal in \
  'fat32_directory_index_default_writable=false' \
  'fat32_directory_index_long_filenames=false' \
  'fat32_directory_index_directory_expansion=false' \
  'fat32_directory_index_journaling=false' \
  'fat32_directory_index_cache_policy=false'; do
  require_literal "$runner" "$literal"
done

echo "x86_64 microkernel FAT32 directory-index contract guard passed."
