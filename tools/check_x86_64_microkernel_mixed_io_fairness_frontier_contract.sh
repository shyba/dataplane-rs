#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
makefile="Makefile"
plan="aidocs/055_microkernel_tls_deferred_robust_appliance_plan_2026-05-31.md"
scheduler_guard="tools/check_x86_64_microkernel_scheduler_fairness_load_contract.sh"
mixed_guard="tools/check_x86_64_microkernel_mixed_appliance_fairness_contract.sh"

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
  local note="$3"
  local status
  set +e
  rg -q --fixed-strings -- "$literal" "$path"
  status="$?"
  set -e
  if [[ "$status" -ne 0 ]]; then
    [[ "$status" -eq 1 ]] || fail "rg failed while scanning $path for required literal: $literal"
    fail "$note"
  fi
}

reject_regex() {
  local path="$1"
  local regex="$2"
  local note="$3"
  local status
  set +e
  rg -q --pcre2 -- "$regex" "$path"
  status="$?"
  set -e
  if [[ "$status" -eq 0 ]]; then
    fail "$note"
  fi
  [[ "$status" -eq 1 ]] || fail "rg failed while scanning $path for forbidden regex: $regex"
}

require_body_literal() {
  local body="$1"
  local literal="$2"
  local note="$3"
  if [[ "$body" != *"$literal"* ]]; then
    fail "$note"
  fi
}

reject_body_regex() {
  local body="$1"
  local regex="$2"
  local note="$3"
  local status
  set +e
  grep -Pq -- "$regex" <<<"$body"
  status="$?"
  set -e
  if [[ "$status" -eq 0 ]]; then
    fail "$note"
  fi
  [[ "$status" -eq 1 ]] || fail "grep failed while scanning extracted block for forbidden regex: $regex"
}

extract_runner_block() {
  local path="$1"
  local start="$2"
  local end="$3"
  awk -v start="$start" -v end="$end" '
    index($0, start) {
      found = 1
    }
    found {
      print
    }
    found && index($0, end) {
      exit 0
    }
    END {
      if (!found) {
        exit 42
      }
    }
  ' "$path"
}

extract_python_dict_block() {
  local path="$1"
  local anchor="$2"
  awk -v anchor="$anchor" '
    index($0, anchor) {
      found = 1
    }
    found {
      print
      line = $0
      opens = gsub(/\{/, "{", line)
      line = $0
      closes = gsub(/\}/, "}", line)
      depth += opens - closes
      if (opens > 0) {
        seen_open = 1
      }
      if (seen_open && depth == 0) {
        exit 0
      }
    }
    END {
      if (!found) {
        exit 42
      }
      if (!seen_open || depth != 0) {
        exit 43
      }
    }
  ' "$path"
}

for path in \
  "$main" \
  "$runner" \
  "$makefile" \
  "$plan" \
  "$scheduler_guard" \
  "$mixed_guard"; do
  require_file "$path"
done

echo "=== x86_64 Microkernel Mixed-IO Fairness Frontier Contract Guard ==="

require_literal "$plan" "x86_64-microkernel-mixed-io-fairness-frontier" \
  "active plan must name the mixed-IO fairness frontier"
require_literal "$plan" "TLS is deferred" \
  "active plan must keep TLS deferred"
require_literal "$scheduler_guard" "scheduler_fairness_load_timer_bound_ok=true" \
  "scheduler fairness guard must remain available as source context"
require_literal "$mixed_guard" "mixed_appliance_fairness_http_ok=true" \
  "mixed appliance fairness guard must remain available as source context"

if rg -q --fixed-strings -- "x86_64-microkernel-mixed-io-fairness-frontier" "$makefile"; then
  require_literal "$makefile" "x86_64-microkernel-mixed-io-fairness-frontier-contract:" \
    "Makefile mixed-IO frontier contract target must run this guard when target exists"
  require_literal "$makefile" "./tools/check_x86_64_microkernel_mixed_io_fairness_frontier_contract.sh" \
    "Makefile must invoke the mixed-IO frontier contract guard"
  require_literal "$makefile" "x86_64-microkernel-mixed-io-fairness-frontier:" \
    "Makefile must expose the mixed-IO frontier proof target"
  require_literal "$makefile" "DP_MICROKERNEL_MIXED_IO_FAIRNESS_FRONTIER_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --mixed-io-fairness-frontier-proof" \
    "Makefile must run the focused mixed-IO proof mode"
fi

if rg -q --fixed-strings -- "--mixed-io-fairness-frontier-proof" "$runner"; then
  require_literal "$runner" "DP_MICROKERNEL_MIXED_IO_FAIRNESS_FRONTIER_PROOF" \
    "runner proof mode must have an env switch"
  require_literal "$runner" 'mixed_io_fairness_summary="$log_dir/x86_64-microkernel-fat32-$run_id.mixed-io-fairness-frontier.summary"' \
    "runner summary must stay under the configured log directory and current run id"
fi

require_literal "$runner" "mixed_io_fairness_summary=" \
  "runner must define an explicit mixed_io_fairness summary artifact"
require_literal "$runner" "mixed_io_fairness_checks = {" \
  "runner verifier must expose mixed_io_fairness checks"

runner_checks="$(extract_python_dict_block "$runner" "mixed_io_fairness_checks = {")" || \
  fail "could not extract mixed_io_fairness_checks dictionary"
runner_summary_block="$(extract_runner_block "$runner" 'elif [[ "$mode" == "--mixed-appliance-fairness-proof" || "$mode" == "--mixed-io-fairness-frontier-proof" ]]; then' 'echo "x86_64 microkernel mixed appliance fairness proof passed."')" || \
  fail "could not extract mixed-IO fairness summary block"

for literal in \
  "mixed_io_fairness_summary_status=pass" \
  "mixed_io_fairness_scheduler_source=dataplane-core-reactor" \
  "mixed_io_fairness_timer_ok=true" \
  "mixed_io_fairness_timer_bound_ok=true" \
  "mixed_io_fairness_cli_ok=true" \
  "mixed_io_fairness_http_ok=true" \
  "mixed_io_fairness_fat32_ok=true" \
  "mixed_io_fairness_network_ok=true" \
  "mixed_io_fairness_fault_progress_ok=true" \
  "mixed_io_fairness_fault_contained=true" \
  "mixed_io_fairness_marker_only_rejected=true" \
  "mixed_io_fairness_no_benchmark_retune_ok=true" \
  'mixed_io_fairness_serial_log=$serial_log' \
  'mixed_io_fairness_qemu_log=$qemu_log' \
  'mixed_io_fairness_client_log=$client_log' \
  'mixed_io_fairness_network_host_log=$host_exchange_log' \
  'mixed_io_fairness_network_pcap=$net_pcap'; do
  require_body_literal "$runner_summary_block" "$literal" \
    "mixed-IO summary block must record required evidence: $literal"
done

for literal in \
  "mixed_io_fairness_timer_ok" \
  "mixed_io_fairness_cli_ok" \
  "mixed_io_fairness_http_ok" \
  "mixed_io_fairness_fat32_ok" \
  "mixed_io_fairness_network_ok" \
  "mixed_io_fairness_fault_progress_ok" \
  "mixed_io_fairness_marker_only_rejected"; do
  require_body_literal "$runner_checks" "$literal" \
    "mixed-IO verifier checks must include $literal"
done

for marker in \
  "DPMK:MIXED-APPLIANCE-TIMER-OK:" \
  "DPMK:MIXED-APPLIANCE-CLI-STATUS-OK:" \
  "DPMK:MIXED-APPLIANCE-HTTP-STATUS-OK:" \
  "DPMK:MIXED-APPLIANCE-FAT32-OK:" \
  "DPMK:MIXED-APPLIANCE-NONTLS-NETWORK-OK:" \
  "DPMK:MIXED-APPLIANCE-CONTAINED-FAULT-OK" \
  "DPMK:MIXED-APPLIANCE-OK"; do
  require_body_literal "$runner_summary_block" "$marker" \
    "mixed-IO summary block must require serial marker $marker"
done

require_body_literal "$runner_summary_block" "duplicate" \
  "mixed-IO summary block must reject duplicate summary keys"
require_body_literal "$runner_summary_block" "run_id" \
  "mixed-IO summary block must reject stale artifact paths"
require_body_literal "$runner_summary_block" "pcap" \
  "mixed-IO summary block must include pcap evidence"
require_body_literal "$runner_summary_block" "log" \
  "mixed-IO summary block must include log evidence"

for scanned in "$main" "$runner" "$makefile" "$plan"; do
  reject_regex "$scanned" 'mixed[-_ ]io.*(HTTPS|https://|cert|certificate|private[[:space:]]+key|curl[[:space:]]+-k|OpenSSL|openssl|rustls|embedded[-_]tls|webpki|ring::|aws-lc-rs|TLS[^_[:alnum:]-])' \
    "mixed-IO frontier must not introduce TLS/HTTPS/certificate drift in $scanned"
  reject_regex "$scanned" 'mixed[-_ ]io.*(generic[[:space:]]+(IPC|ipc|socket)|generic[_-](IPC|ipc|socket)|socket abstraction|SocketApi|service discovery|mDNS|DNS|NTP)' \
    "mixed-IO frontier must not introduce generic sockets or service discovery in $scanned"
  reject_regex "$scanned" 'mixed[-_ ]io.*(restart command|reset command|reset task|debugger|raw[[:space:]]+memory|MMIO|mmio|page-table|pagetable)' \
    "mixed-IO frontier must not drift into restart/debugger/raw memory scope in $scanned"
  reject_regex "$scanned" 'mixed[-_ ]io.*(STRICT_FIVE_CALIBRATION|benchmark[[:space:]]+tuning|retune[[:space:]]+benchmarks|retuning[[:space:]]+benchmarks|calibration)' \
    "mixed-IO frontier must not retune benchmarks in $scanned"
done

reject_body_regex "$runner_checks" 'HTTPS|https://|cert|certificate|private[[:space:]]+key|curl[[:space:]]+-k|OpenSSL|openssl|rustls|embedded[-_]tls|webpki|DP_MICROKERNEL_TLS_PROOF|--tls-proof|generic[[:space:]]+socket|SocketApi|service discovery|restart|reset|debugger' \
  "mixed-IO verifier checks must not add TLS, generic sockets, restart/reset, or debugger drift"
reject_body_regex "$runner_summary_block" 'STRICT_FIVE_CALIBRATION|benchmark[[:space:]]+tuning|retune[[:space:]]+benchmarks|retuning[[:space:]]+benchmarks|calibration' \
  "mixed-IO summary block must not retune benchmarks"

echo "x86_64 microkernel mixed-IO fairness frontier contract guard passed."
