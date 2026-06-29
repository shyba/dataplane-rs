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

echo "=== x86_64 Microkernel Status Snapshot Consistency Contract Guard ==="

for literal in \
  'fn run_status_snapshot_consistency_probe(&mut self)' \
  'struct OperatorStatusSnapshot' \
  'http: HttpPolicyCounters' \
  'tcp: TcpControlCounters' \
  'drops: NetworkDropCounters' \
  'operator_status_snapshot_from_sizes' \
  'render_operator_status_body' \
  'DPCLI:STATUS-SNAPSHOT-BEGIN' \
  'DPHTTP:STATUS-SNAPSHOT-BEGIN' \
  'DPMK:STATUS-SNAPSHOT-BOUNDED-BYTES:' \
  'DPMK:STATUS-SNAPSHOT-CLI-HTTP-SAME-BODY-OK' \
  'DPMK:STATUS-SNAPSHOT-CONSISTENCY-OK' \
  'DPSTATUS:HTTP-COUNTERS status_page=1 response_200=' \
  'DPSTATUS:NET-COUNTERS ready='; do
  require_literal "$main" "$literal" "guest source must contain status snapshot literal: $literal"
done

for literal in \
  'status_snapshot_consistency_summary="$log_dir/x86_64-microkernel-fat32-$run_id.status-snapshot-consistency.summary"' \
  'DP_MICROKERNEL_STATUS_SNAPSHOT_CONSISTENCY_PROOF' \
  '--status-snapshot-consistency-proof' \
  'DP_STATUS_SNAPSHOT_CONSISTENCY_PROOF' \
  'status_snapshot_consistency_mode=cli-http-shared-fixed-snapshot' \
  'status_snapshot_consistency_summary_status=pass' \
  'status_snapshot_consistency_cli_status_ok=true' \
  'status_snapshot_consistency_http_status_ok=true' \
  'status_snapshot_consistency_same_body_ok=true' \
  'status_snapshot_consistency_tasks_match_ok=true' \
  'status_snapshot_consistency_fs_sizes_match_ok=true' \
  'status_snapshot_consistency_net_ready_match_ok=true' \
  'status_snapshot_consistency_http_counters_match_ok=true' \
  'status_snapshot_consistency_faults_match_ok=true' \
  'status_snapshot_consistency_recovery_no_restart_ok=true' \
  'status_snapshot_consistency_bounded_ok=true' \
  'x86_64 microkernel status snapshot consistency proof passed.' \
  'status snapshot consistency summary: $status_snapshot_consistency_summary'; do
  require_literal "$runner" "$literal" "runner must contain status snapshot literal: $literal"
done

for literal in \
  'status-snapshot-consistency' \
  "status-snapshot-consistency) echo 'make x86_64-microkernel-status-snapshot-consistency' ;;" \
  "status-snapshot-consistency) echo 'x86_64 microkernel status snapshot consistency proof passed.' ;;" \
  "status-snapshot-consistency) echo 'status snapshot consistency summary|client log|serial log|qemu log|network pcap' ;;"; do
  require_literal "$matrix_runner" "$literal" "validation matrix must contain status snapshot literal: $literal"
done

require_literal "$makefile" 'x86_64-microkernel-status-snapshot-consistency-contract:' \
  "Makefile must expose the status snapshot contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_status_snapshot_consistency_contract.sh' \
  "Makefile status snapshot contract must run the focused guard"
require_literal "$makefile" 'x86_64-microkernel-status-snapshot-consistency: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-status-snapshot-consistency-contract' \
  "Makefile status snapshot proof target must preserve neighboring contract coverage"
require_literal "$makefile" 'DP_MICROKERNEL_STATUS_SNAPSHOT_CONSISTENCY_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --status-snapshot-consistency-proof' \
  "Makefile status snapshot target must run the focused proof mode"

for literal in \
  'status-snapshot-consistency' \
  '--status-snapshot-consistency-proof' \
  'DP_MICROKERNEL_STATUS_SNAPSHOT_CONSISTENCY_PROOF' \
  'status_snapshot_consistency_summary_status=pass'; do
  require_literal "$matrix_guard" "$literal" "validation-matrix guard must cover status snapshot literal: $literal"
done

reject_regex "$main" 'serde_json|extern crate alloc|Vec<|String::|Box<|BTreeMap|HashMap|TcpListener|GenericSocket|SocketApi|debugger|shell|restart task|reset task|peek memory|poke memory|page-table dump|mmio dump' \
  "status snapshot packet must stay bounded, non-alloc, non-debugger, and non-socket"
reject_regex "$runner" 'curl[[:space:]]+-k|https://|openssl|rustls|embedded-tls|webpki|aws-lc-rs|DPMK:TLS|HTTPS-|STRICT_FIVE_CALIBRATION|benchmark[[:space:]]+tuning|retun(e|ing)' \
  "status snapshot packet must not reopen TLS or tune benchmarks"

echo "x86_64 microkernel status snapshot consistency contract OK"
