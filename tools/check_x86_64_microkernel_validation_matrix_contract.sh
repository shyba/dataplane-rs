#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

note="aidocs/050_microkernel_robust_design_frontier_2026-05-30.md"
runner="tools/x86_64_microkernel_validation_matrix_run.sh"
helper="tools/x86_64_microkernel_validation_matrix_lib.sh"
fat32_runner="tools/x86_64_microkernel_fat32_run.sh"
makefile="Makefile"

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

echo "=== x86_64 Microkernel Validation Matrix Contract Guard ==="

require_file "$note"
require_file "$runner"
require_file "$helper"
require_file "$fat32_runner"
require_file "$makefile"

require_literal "$note" "## Phase 6: Validation Matrix Expansion" \
  "validation matrix note must define Phase 6"
require_literal "$note" "Add a single matrix runner that can execute named scenarios and write a small" \
  "validation matrix note must require a single matrix runner"
require_literal "$note" 'summary file under `/home/user/mnt/dataplane/logs`' \
  "validation matrix note must require summaries under /home/user/mnt/dataplane/logs"
require_literal "$note" "Keep QEMU x86_64, Cortex-M0 QEMU, RP2040 compile/no-alloc, and" \
  "validation matrix note must require evidence class separation"
require_literal "$note" "Raspi3B MMU" \
  "validation matrix note must require evidence class separation"
require_literal "$note" "evidence separated" \
  "validation matrix note must require evidence class separation"
require_literal "$note" "fail closed on parser errors and missing captures" \
  "validation matrix note must require fail-closed parsing"

require_literal "$helper" 'dp_default_log_root()' \
  "matrix helper must own default log-root resolution"
require_literal "$helper" 'local value="${!env_name:-/home/user/mnt/dataplane/logs}"' \
  "matrix helper must keep /home/user/mnt/dataplane/logs as default log root"
require_literal "$helper" 'dp_new_run_id()' \
  "matrix helper must own run-id generation"
require_literal "$helper" 'dp_artifact_path()' \
  "matrix helper must own artifact path construction"
require_literal "$runner" 'log_root="$(dp_default_log_root DP_VALIDATION_MATRIX_LOG_ROOT)"' \
  "matrix runner must keep summaries and logs under /home/user/mnt/dataplane/logs"
require_literal "$runner" 'run_id="$(dp_new_run_id)"' \
  "matrix runner must use shared run-id generation"
require_literal "$runner" 'summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-validation-matrix" "$run_id" "summary")"' \
  "matrix runner must write a concise summary under the log root"
require_literal "$runner" 'scenario_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-validation-matrix" "$run_id" "$scenario.log")"' \
  "matrix runner must keep per-scenario logs under the log root"
require_literal "$runner" 'source "tools/x86_64_microkernel_validation_matrix_lib.sh"' \
  "matrix runner must source fail-closed matrix helper functions"
require_literal "$helper" 'if [[ "$path" != /home/user/mnt/dataplane/* ]]; then' \
  "matrix runner must reject artifacts outside the large-file boundary"
require_literal "$helper" 'if [[ ! -s "$path" ]]; then' \
  "matrix runner must fail closed when a captured artifact is missing or empty"
require_literal "$helper" '"$label":\ /*) line="$candidate" ;;' \
  "matrix helper must parse artifact labels as fixed shell prefixes"
require_literal "$helper" 'path="${line#"$label": }"' \
  "matrix helper must strip exact fixed artifact labels"
require_literal "$runner" 'evidence_class=' \
  "matrix runner must record evidence classes separately"
require_literal "$runner" "storage-service-counters) echo 'x86_64-source-scan' ;;" \
  "matrix runner must label storage-service-counters as source-scan evidence"
require_literal "$runner" 'x86_64-qemu-microkernel' \
  "matrix runner must label x86_64 microkernel QEMU evidence and keep non-storage QEMU scenarios on that class"
require_literal "$runner" 'x86_64-qemu-virtio' \
  "matrix runner must label x86_64 virtio evidence separately"
require_literal "$runner" 'raspi3b-qemu-mmu' \
  "matrix runner must label Raspi3B MMU evidence separately"
require_literal "$runner" 'cortexm0-qemu-noalloc' \
  "matrix runner must label Cortex-M0 QEMU evidence separately"
require_literal "$runner" 'rp2040-compile-noalloc' \
  "matrix runner must label RP2040 compile/no-alloc evidence separately"
require_literal "$runner" 'scenario_command()' \
  "matrix runner must dispatch named scenarios explicitly"
require_literal "$runner" 'extract_artifact()' \
  "matrix runner must use fail-closed artifact extraction"
require_literal "$runner" 'dp_require_marker "$scenario_log" "$marker"' \
  "matrix runner must require positive scenario markers"
require_literal "$runner" "fat32-smoke|curl-proof|fairness|dhcp|fat32-write|http-backpressure|fs-service-boundary|fs-policy-and-directory-slice|capability-fault-policy|operator-appliance-surface|operator-appliance-consolidation|fault-and-recovery-preconditions|service-timeout-stale-reply-policy|service-route-table|service-ipc-audit|service-boundary-audit|status-route-unification|mixed-appliance-fairness|storage-integrity-frontier|http-policy-matrix|bounded-stream-session-refresh|scheduler-fairness-load|memory-isolation-map|operator-recovery-runbook|stream-session-state|status-snapshot-consistency|status-snapshot-contract-v2|filesystem-read-matrix|nontls-network-negative-matrix|network-control-plane-slice|bounded-tcp-negative|nontls-network-service)" \
  "matrix runner must keep the non-storage QEMU scenarios mapped to x86_64-qemu-microkernel"
require_literal "$runner" 'if [[ "$metadata_status" -ne 0 || -z "$marker" ]]; then' \
  "matrix runner must reject missing marker metadata"
require_literal "$runner" 'if [[ "$metadata_status" -ne 0 || -z "$evidence_class" ]]; then' \
  "matrix runner must reject missing evidence class metadata"

require_literal "$fat32_runner" '--curl-proof' \
  "runner must dispatch the curl proof scenario"
require_literal "$fat32_runner" '--fairness-proof' \
  "runner must dispatch the fairness proof scenario"
require_literal "$fat32_runner" '--dhcp-proof' \
  "runner must dispatch the DHCP proof scenario"
require_literal "$fat32_runner" '--write-proof' \
  "runner must dispatch the write proof scenario"
require_literal "$fat32_runner" '--bounded-tcp-negative-proof' \
  "runner must dispatch the TCP negative scenario"
require_literal "$fat32_runner" '--http-backpressure-proof' \
  "runner must dispatch the HTTP backpressure scenario"
require_literal "$fat32_runner" '--fs-service-boundary-proof' \
  "runner must dispatch the filesystem boundary scenario"
require_literal "$runner" "capability-fault-policy) echo 'make x86_64-microkernel-capability-fault-policy' ;;" \
  "matrix runner must dispatch the capability and fault policy scenario"
require_literal "$runner" "nontls-network-service) echo 'make x86_64-microkernel-nontls-network-service-matrix' ;;" \
  "matrix runner must dispatch the non-TLS network service scenario"
require_literal "$runner" "operator-appliance-surface) echo 'make x86_64-microkernel-operator-appliance-surface' ;;" \
  "matrix runner must dispatch the operator appliance scenario"
require_literal "$runner" "operator-appliance-consolidation) echo 'make x86_64-microkernel-operator-appliance-consolidation' ;;" \
  "matrix runner must dispatch the operator appliance consolidation scenario"
require_literal "$runner" "storage-integrity-frontier) echo 'make x86_64-microkernel-storage-integrity-frontier' ;;" \
  "matrix runner must dispatch the storage integrity scenario"
require_literal "$runner" "scheduler-fairness-load) echo 'make x86_64-microkernel-scheduler-fairness-load' ;;" \
  "matrix runner must dispatch the scheduler fairness load scenario"
require_literal "$runner" "memory-isolation-map) echo 'make x86_64-microkernel-memory-isolation-map' ;;" \
  "matrix runner must dispatch the memory isolation map scenario"
require_literal "$runner" "operator-recovery-runbook) echo 'make x86_64-microkernel-operator-recovery-runbook' ;;" \
  "matrix runner must dispatch the operator recovery runbook scenario"
require_literal "$runner" "stream-session-state) echo 'make x86_64-microkernel-stream-session-state' ;;" \
  "matrix runner must dispatch the stream session state scenario"
require_literal "$runner" "status-snapshot-consistency) echo 'make x86_64-microkernel-status-snapshot-consistency' ;;" \
  "matrix runner must dispatch the status snapshot consistency scenario"
require_literal "$runner" "status-snapshot-contract-v2) echo 'make x86_64-microkernel-status-snapshot-contract-v2' ;;" \
  "matrix runner must dispatch the status snapshot contract v2 scenario"
require_literal "$runner" "filesystem-read-matrix) echo 'make x86_64-microkernel-filesystem-read-matrix' ;;" \
  "matrix runner must dispatch the filesystem read matrix scenario"
require_literal "$runner" "fs-policy-and-directory-slice) echo 'make x86_64-microkernel-fs-policy-and-directory-slice' ;;" \
  "matrix runner must dispatch the filesystem policy and directory slice scenario"
require_literal "$runner" "nontls-network-negative-matrix) echo 'make x86_64-microkernel-nontls-network-negative-matrix' ;;" \
  "matrix runner must dispatch the non-TLS network negative matrix scenario"
require_literal "$runner" "storage-service-counters) echo 'make x86_64-microkernel-storage-service-counters' ;;" \
  "matrix runner must dispatch the storage service counters scenario"
require_literal "$runner" "service-timeout-stale-reply-policy) echo 'make x86_64-microkernel-service-timeout-stale-reply-policy' ;;" \
  "matrix runner must dispatch the service timeout/stale reply policy scenario"
require_literal "$runner" "service-route-table) echo 'make x86_64-microkernel-service-route-table' ;;" \
  "matrix runner must dispatch the service route table scenario"
require_literal "$runner" "service-ipc-audit) echo 'make x86_64-microkernel-service-ipc-audit' ;;" \
  "matrix runner must dispatch the service IPC audit scenario"
require_literal "$runner" "service-boundary-audit) echo 'make x86_64-microkernel-service-boundary-audit' ;;" \
  "matrix runner must dispatch the service boundary audit scenario"
require_literal "$runner" "status-route-unification) echo 'make x86_64-microkernel-status-route-unification' ;;" \
  "matrix runner must dispatch the status route unification scenario"

for scenario in \
  'fat32-smoke' \
  'curl-proof' \
  'fairness' \
  'dhcp' \
  'fat32-write' \
  'http-backpressure' \
  'fs-service-boundary' \
  'fs-policy-and-directory-slice' \
  'capability-fault-policy' \
  'operator-appliance-surface' \
  'operator-appliance-consolidation' \
  'service-timeout-stale-reply-policy' \
  'storage-integrity-frontier' \
  'storage-service-counters' \
  'scheduler-fairness-load' \
  'mixed-appliance-fairness' \
  'memory-isolation-map' \
  'operator-recovery-runbook' \
  'stream-session-state' \
  'status-snapshot-consistency' \
  'filesystem-read-matrix' \
  'nontls-network-negative-matrix' \
  'network-control-plane-slice' \
  'validation-failure-injection' \
  'bounded-stream-session-refresh' \
  'bounded-tcp-negative' \
  'nontls-network-service' \
  'x86_64-virtio-smoke' \
  'raspi3b-mmu-smoke' \
  'qemu-cortexm0-smoke' \
  'rp2040-check'
do
  require_literal "$runner" "$scenario" \
    "matrix runner must know required scenario: $scenario"
done

require_literal "$makefile" 'x86_64-microkernel-fat32-smoke: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract' \
  "Makefile must keep the x86_64 microkernel smoke target wired to the existing contract"
require_literal "$makefile" 'x86_64-microkernel-fat32-fairness: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract' \
  "Makefile must keep the fairness target wired to the existing contract"
require_literal "$makefile" 'x86_64-microkernel-fat32-dhcp: guard-scripts-executable x86_64-microkernel-fat32-smoke x86_64-microkernel-fat32-contract' \
  "Makefile must keep the DHCP target wired to the existing contract"
require_literal "$makefile" 'x86_64-microkernel-fat32-write: guard-scripts-executable x86_64-microkernel-fat32-smoke x86_64-microkernel-fat32-write-build x86_64-microkernel-fat32-contract' \
  "Makefile must keep the write target wired to the existing contract"
require_literal "$makefile" 'x86_64-microkernel-fat32-http-backpressure: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract' \
  "Makefile must keep the HTTP backpressure target wired to the existing contract"
require_literal "$makefile" 'x86_64-microkernel-fs-service-boundary: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract' \
  "Makefile must keep the filesystem boundary target wired to the existing contract"
require_literal "$makefile" 'x86_64-microkernel-capability-fault-policy: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract' \
  "Makefile must keep the capability fault policy target wired to the existing contract"
require_literal "$makefile" 'x86_64-microkernel-nontls-network-service-matrix: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-nontls-network-service-contract' \
  "Makefile must expose the non-TLS network service matrix target"
require_literal "$makefile" 'x86_64-microkernel-nontls-network-negative-matrix-contract:' \
  "Makefile must expose the non-TLS network negative matrix contract target"
require_literal "$makefile" 'x86_64-microkernel-nontls-network-negative-matrix: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-nontls-network-service-contract x86_64-microkernel-nontls-network-negative-matrix-contract' \
  "Makefile must expose the non-TLS network negative matrix proof target"
require_literal "$makefile" 'x86_64-microkernel-operator-appliance-surface-contract:' \
  "Makefile must expose the operator appliance contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_operator_appliance_surface_contract.sh' \
  "Makefile operator appliance contract target must run the packet-specific guard"
require_literal "$makefile" 'x86_64-microkernel-operator-appliance-surface: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-operator-appliance-surface-contract' \
  "Makefile must expose the operator appliance proof target"
require_literal "$makefile" 'x86_64-microkernel-operator-appliance-consolidation-contract:' \
  "Makefile must expose the operator appliance consolidation contract target"
require_literal "$makefile" 'x86_64-microkernel-operator-appliance-consolidation: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-operator-appliance-consolidation-contract' \
  "Makefile must expose the operator appliance consolidation proof target"
require_literal "$makefile" 'x86_64-microkernel-storage-integrity-frontier-contract:' \
  "Makefile must expose the storage integrity contract target"
require_literal "$makefile" 'x86_64-microkernel-storage-integrity-frontier: guard-scripts-executable x86_64-microkernel-fat32-smoke x86_64-microkernel-fat32-write x86_64-microkernel-storage-integrity-frontier-contract' \
  "Makefile must expose the storage integrity proof target"
require_literal "$makefile" 'x86_64-microkernel-storage-service-counters-contract:' \
  "Makefile must expose the storage service counters contract target"
require_literal "$makefile" 'x86_64-microkernel-storage-service-counters: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-storage-integrity-frontier-contract x86_64-microkernel-storage-service-counters-contract' \
  "Makefile must expose the storage service counters proof target"
require_literal "$makefile" 'x86_64-microkernel-scheduler-fairness-load-contract:' \
  "Makefile must expose the scheduler fairness load contract target"
require_literal "$makefile" 'x86_64-microkernel-scheduler-fairness-load: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-nontls-network-service-contract x86_64-microkernel-service-ipc-audit-contract x86_64-microkernel-scheduler-fairness-load-contract' \
  "Makefile must expose the scheduler fairness load proof target"
require_literal "$makefile" 'x86_64-microkernel-service-route-table-contract:' \
  "Makefile must expose the service route table contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_service_route_table_contract.sh' \
  "Makefile service route table target must run the service route table guard"
require_literal "$makefile" 'x86_64-microkernel-service-route-table: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-service-mailbox-envelope-contract x86_64-microkernel-service-route-table-contract' \
  "Makefile must expose the current service route table proof target with prerequisite guards"
require_literal "$makefile" 'x86_64-microkernel-mixed-appliance-fairness-contract:' \
  "Makefile must expose the mixed appliance fairness contract target"
require_literal "$makefile" 'x86_64-microkernel-mixed-appliance-fairness: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-nontls-network-service-contract x86_64-microkernel-service-ipc-audit-contract x86_64-microkernel-scheduler-fairness-load-contract x86_64-microkernel-filesystem-read-matrix-contract x86_64-microkernel-network-control-plane-slice-contract x86_64-microkernel-fault-and-recovery-preconditions-contract x86_64-microkernel-mixed-appliance-fairness-contract' \
  "Makefile must expose the mixed appliance fairness proof target"
require_literal "$makefile" 'x86_64-microkernel-memory-isolation-map-contract:' \
  "Makefile must expose the memory isolation map contract target"
require_literal "$makefile" 'x86_64-microkernel-memory-isolation-map: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-nontls-network-service-contract x86_64-microkernel-service-ipc-audit-contract x86_64-microkernel-scheduler-fairness-load-contract x86_64-microkernel-memory-isolation-map-contract' \
  "Makefile must expose the memory isolation map proof target"
require_literal "$makefile" 'x86_64-microkernel-operator-recovery-runbook-contract:' \
  "Makefile must expose the operator recovery runbook contract target"
require_literal "$makefile" 'x86_64-microkernel-operator-recovery-runbook: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-service-ipc-audit-contract x86_64-microkernel-memory-isolation-map-contract x86_64-microkernel-operator-recovery-runbook-contract' \
  "Makefile must expose the operator recovery runbook proof target"
require_literal "$makefile" 'x86_64-microkernel-stream-session-state-contract:' \
  "Makefile must expose the stream session state contract target"
require_literal "$makefile" 'x86_64-microkernel-stream-session-state: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-bounded-tcp-stream-contract x86_64-microkernel-stream-session-state-contract' \
  "Makefile must expose the stream session state proof target"
require_literal "$makefile" 'x86_64-microkernel-status-snapshot-consistency-contract:' \
  "Makefile must expose the status snapshot consistency contract target"
require_literal "$makefile" 'x86_64-microkernel-status-snapshot-consistency: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-status-snapshot-consistency-contract' \
  "Makefile must expose the status snapshot consistency proof target"
require_literal "$makefile" 'x86_64-microkernel-status-snapshot-contract-v2-contract:' \
  "Makefile must expose the status snapshot contract v2 contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_status_snapshot_contract_v2.sh' \
  "Makefile status snapshot contract v2 target must run the focused guard"
require_literal "$makefile" 'x86_64-microkernel-status-snapshot-contract-v2: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-status-snapshot-consistency-contract x86_64-microkernel-status-snapshot-contract-v2-contract' \
  "Makefile must expose the status snapshot contract v2 proof target"
require_literal "$makefile" 'x86_64-microkernel-bounded-tcp-stream-contract:' \
  "Makefile must keep the bounded TCP contract target available"
require_literal "$makefile" 'x86_64-microkernel-validation-matrix-contract:' \
  "Makefile must expose a validation matrix contract target"
require_literal "$makefile" 'x86_64-microkernel-validation-matrix:' \
  "Makefile must expose a validation matrix target"
require_literal "$makefile" './tools/x86_64_microkernel_validation_matrix_run.sh --quick' \
  "Makefile validation matrix target must run the quick matrix"
require_literal "$makefile" 'x86_64-microkernel-validation-failure-injection-contract:' \
  "Makefile must expose the validation failure injection contract target"
require_literal "$makefile" 'x86_64-microkernel-validation-failure-injection: guard-scripts-executable x86_64-microkernel-validation-matrix-contract x86_64-microkernel-validation-failure-injection-contract' \
  "Makefile must expose the validation failure injection proof target"
require_literal "$makefile" 'x86_64-microkernel-bounded-stream-session-refresh-contract:' \
  "Makefile must expose the bounded stream/session refresh contract target"
require_literal "$makefile" 'x86_64-microkernel-bounded-stream-session-refresh: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-bounded-tcp-stream-contract x86_64-microkernel-stream-session-state-contract x86_64-microkernel-http-policy-matrix-contract x86_64-microkernel-bounded-stream-session-refresh-contract' \
  "Makefile must expose the bounded stream/session refresh proof target"
require_literal "$makefile" 'x86_64-microkernel-fs-policy-and-directory-slice-contract:' \
  "Makefile must expose the filesystem policy and directory slice contract target"
require_literal "$makefile" 'x86_64-microkernel-fs-policy-and-directory-slice: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-filesystem-read-matrix-contract x86_64-microkernel-fs-policy-and-directory-slice-contract' \
  "Makefile must expose the filesystem policy and directory slice proof target"
require_literal "$runner" "operator-appliance-surface) echo 'make x86_64-microkernel-operator-appliance-surface' ;;" \
  "matrix runner must dispatch operator appliance through Make"
require_literal "$runner" "operator-appliance-surface) echo 'x86_64 microkernel operator appliance source-scan proof passed.' ;;" \
  "matrix runner must require the operator appliance positive marker"
require_literal "$runner" "operator-appliance-surface) echo 'summary|source scan' ;;" \
  "matrix runner must require operator appliance artifacts"
require_literal "$runner" "operator-appliance-consolidation) echo 'make x86_64-microkernel-operator-appliance-consolidation' ;;" \
  "matrix runner must dispatch operator appliance consolidation through Make"
require_literal "$runner" "operator-appliance-consolidation) echo 'x86_64 microkernel operator appliance consolidation proof passed.' ;;" \
  "matrix runner must require the operator appliance consolidation positive marker"
require_literal "$runner" "operator-appliance-consolidation) echo 'operator appliance consolidation summary|client log|serial log|qemu log|network pcap' ;;" \
  "matrix runner must require operator appliance consolidation artifacts"
require_literal "$runner" "service-timeout-stale-reply-policy) echo 'make x86_64-microkernel-service-timeout-stale-reply-policy' ;;" \
  "matrix runner must dispatch service timeout/stale reply policy through Make"
require_literal "$runner" "service-timeout-stale-reply-policy) echo 'x86_64 microkernel service timeout stale reply policy proof passed.' ;;" \
  "matrix runner must require the service timeout/stale reply policy positive marker"
require_literal "$runner" "service-timeout-stale-reply-policy) echo 'service timeout stale reply policy summary|client log|serial log|qemu log|network pcap' ;;" \
  "matrix runner must require service timeout/stale reply policy artifacts"
require_literal "$runner" "storage-integrity-frontier) echo 'make x86_64-microkernel-storage-integrity-frontier' ;;" \
  "matrix runner must dispatch storage integrity through Make"
require_literal "$runner" "storage-integrity-frontier) echo 'x86_64 microkernel storage integrity proof passed.' ;;" \
  "matrix runner must require the storage integrity positive marker"
require_literal "$runner" "storage-integrity-frontier) echo 'client log|serial log|qemu log|network pcap' ;;" \
  "matrix runner must require storage integrity artifacts"
require_literal "$runner" "storage-service-counters) echo 'make x86_64-microkernel-storage-service-counters' ;;" \
  "matrix runner must dispatch storage service counters through Make"
require_literal "$runner" "storage-service-counters) echo 'x86_64 microkernel storage service counters source-scan proof passed.' ;;" \
  "matrix runner must require the storage service counters positive marker"
require_literal "$runner" "storage-service-counters) echo 'storage service counters summary|source scan' ;;" \
  "matrix runner must require honest storage service counters artifacts"
require_literal "$runner" "scheduler-fairness-load) echo 'make x86_64-microkernel-scheduler-fairness-load' ;;" \
  "matrix runner must dispatch scheduler fairness through Make"
require_literal "$runner" "scheduler-fairness-load) echo 'x86_64 microkernel scheduler fairness load proof passed.' ;;" \
  "matrix runner must require the scheduler fairness positive marker"
require_literal "$runner" "scheduler-fairness-load) echo 'scheduler fairness summary|fairness log|fairness client log|fairness curl log|fairness pcap|serial log|qemu log' ;;" \
  "matrix runner must require scheduler fairness artifacts"
require_literal "$runner" "mixed-appliance-fairness) echo 'make x86_64-microkernel-mixed-appliance-fairness' ;;" \
  "matrix runner must dispatch mixed appliance fairness through Make"
require_literal "$runner" "mixed-appliance-fairness) echo 'x86_64 microkernel mixed appliance fairness proof passed.' ;;" \
  "matrix runner must require the mixed appliance fairness positive marker"
require_literal "$runner" "mixed-appliance-fairness) echo 'mixed appliance fairness summary|client log|network host log|mixed appliance fairness pcap|serial log|qemu log' ;;" \
  "matrix runner must require mixed appliance fairness artifacts"
require_literal "$runner" "memory-isolation-map) echo 'make x86_64-microkernel-memory-isolation-map' ;;" \
  "matrix runner must dispatch memory isolation through Make"
require_literal "$runner" "memory-isolation-map) echo 'x86_64 microkernel memory isolation map proof passed.' ;;" \
  "matrix runner must require the memory isolation positive marker"
require_literal "$runner" "memory-isolation-map) echo 'memory isolation summary|client log|serial log|qemu log|network pcap' ;;" \
  "matrix runner must require memory isolation artifacts"
require_literal "$runner" "operator-recovery-runbook) echo 'make x86_64-microkernel-operator-recovery-runbook' ;;" \
  "matrix runner must dispatch operator recovery through Make"
require_literal "$runner" "operator-recovery-runbook) echo 'x86_64 microkernel operator recovery runbook proof passed.' ;;" \
  "matrix runner must require the operator recovery positive marker"
require_literal "$runner" "operator-recovery-runbook) echo 'operator recovery summary|client log|serial log|qemu log|network pcap' ;;" \
  "matrix runner must require operator recovery artifacts"
require_literal "$runner" "stream-session-state) echo 'make x86_64-microkernel-stream-session-state' ;;" \
  "matrix runner must dispatch stream session state through Make"
require_literal "$runner" "stream-session-state) echo 'x86_64 microkernel stream session state proof passed.' ;;" \
  "matrix runner must require the stream session state positive marker"
require_literal "$runner" "stream-session-state) echo 'stream session state summary|client log|serial log|qemu log|network pcap' ;;" \
  "matrix runner must require stream session state artifacts"
require_literal "$runner" "bounded-stream-session-refresh) echo 'make x86_64-microkernel-bounded-stream-session-refresh' ;;" \
  "matrix runner must dispatch bounded stream/session refresh through Make"
require_literal "$runner" "bounded-stream-session-refresh) echo 'x86_64 microkernel bounded stream session refresh proof passed.' ;;" \
  "matrix runner must require the bounded stream/session refresh positive marker"
require_literal "$runner" "bounded-stream-session-refresh) echo 'bounded stream session refresh summary|stream session state summary|stream session client log|stream session serial log|stream session pcap|HTTP policy matrix summary|HTTP policy curl pcap' ;;" \
  "matrix runner must require bounded stream/session refresh artifacts"
require_literal "$runner" "status-snapshot-consistency) echo 'make x86_64-microkernel-status-snapshot-consistency' ;;" \
  "matrix runner must dispatch status snapshot consistency through Make"
require_literal "$runner" "status-snapshot-consistency) echo 'x86_64 microkernel status snapshot consistency proof passed.' ;;" \
  "matrix runner must require the status snapshot consistency positive marker"
require_literal "$runner" "status-snapshot-consistency) echo 'status snapshot consistency summary|client log|serial log|qemu log|network pcap' ;;" \
  "matrix runner must require status snapshot consistency artifacts"
require_literal "$runner" "status-snapshot-contract-v2) echo 'make x86_64-microkernel-status-snapshot-contract-v2' ;;" \
  "matrix runner must dispatch status snapshot contract v2 through Make"
require_literal "$runner" "status-snapshot-contract-v2) echo 'x86_64 microkernel status snapshot contract v2 proof passed.' ;;" \
  "matrix runner must require the status snapshot contract v2 positive marker"
require_literal "$runner" "status-snapshot-contract-v2) echo 'status snapshot v2 summary|client log|serial log|qemu log|network pcap' ;;" \
  "matrix runner must require status snapshot contract v2 artifacts"
require_literal "$runner" "filesystem-read-matrix) echo 'make x86_64-microkernel-filesystem-read-matrix' ;;" \
  "matrix runner must dispatch filesystem read matrix through Make"
require_literal "$runner" "filesystem-read-matrix) echo 'x86_64 microkernel filesystem read matrix proof passed.' ;;" \
  "matrix runner must require the filesystem read matrix positive marker"
require_literal "$runner" "filesystem-read-matrix) echo 'filesystem read matrix summary|client log|serial log|qemu log|network pcap' ;;" \
  "matrix runner must require filesystem read matrix artifacts"
require_literal "$runner" "fs-policy-and-directory-slice) echo 'make x86_64-microkernel-fs-policy-and-directory-slice' ;;" \
  "matrix runner must dispatch filesystem policy and directory slice through Make"
require_literal "$runner" "fs-policy-and-directory-slice) echo 'x86_64 microkernel fs policy and directory slice proof passed.' ;;" \
  "matrix runner must require the filesystem policy and directory slice positive marker"
require_literal "$runner" "fs-policy-and-directory-slice) echo 'fs policy and directory slice summary|client log|serial log|qemu log|network pcap' ;;" \
  "matrix runner must require filesystem policy and directory slice artifacts"
require_literal "$runner" "nontls-network-negative-matrix) echo 'make x86_64-microkernel-nontls-network-negative-matrix' ;;" \
  "matrix runner must dispatch non-TLS network negative matrix through Make"
require_literal "$runner" "nontls-network-negative-matrix) echo 'x86_64 microkernel non-TLS network negative matrix passed.' ;;" \
  "matrix runner must require the non-TLS network negative matrix positive marker"
require_literal "$runner" "nontls-network-negative-matrix) echo 'non-TLS network negative summary|network host log|network pcap|serial log|qemu log' ;;" \
  "matrix runner must require non-TLS network negative matrix artifacts"
require_literal "$runner" "network-control-plane-slice) echo 'make x86_64-microkernel-network-control-plane-slice' ;;" \
  "matrix runner must dispatch network control-plane slice through Make"
require_literal "$runner" "network-control-plane-slice) echo 'x86_64 microkernel network control-plane slice proof passed.' ;;" \
  "matrix runner must require the network control-plane slice positive marker"
require_literal "$runner" "network-control-plane-slice) echo 'network control-plane slice summary|network host log|network pcap|serial log|qemu log' ;;" \
  "matrix runner must require network control-plane slice artifacts"
require_literal "$runner" "service-route-table) echo 'make x86_64-microkernel-service-route-table' ;;" \
  "matrix runner must dispatch service route table through Make"
require_literal "$runner" "service-route-table) echo 'x86_64 microkernel service route table proof passed.' ;;" \
  "matrix runner must require the service route table positive marker"
require_literal "$runner" "service-route-table) echo 'service route table summary|client log|serial log|qemu log|network pcap' ;;" \
  "matrix runner must require service route table artifacts"
require_literal "$runner" "validation-failure-injection) echo 'make x86_64-microkernel-validation-failure-injection' ;;" \
  "matrix runner must dispatch validation failure injection through Make"
require_literal "$runner" "validation-failure-injection) echo 'validation-matrix-negative-proof' ;;" \
  "matrix runner must classify validation failure injection separately"
require_literal "$runner" "validation-failure-injection) echo 'x86_64 microkernel validation failure injection proof passed.' ;;" \
  "matrix runner must require the validation failure injection positive marker"
require_literal "$runner" "validation-failure-injection) echo 'validation failure injection summary|failure injection log' ;;" \
  "matrix runner must require validation failure injection artifacts"
require_literal "$runner" "memory-isolation-map) echo 'make x86_64-microkernel-memory-isolation-map' ;;" \
  "matrix runner must dispatch the memory isolation map scenario"
require_literal "$runner" "memory-isolation-map) echo 'x86_64 microkernel memory isolation map proof passed.' ;;" \
  "matrix runner must require the memory isolation map positive marker"
require_literal "$runner" "memory-isolation-map) echo 'memory isolation summary|client log|serial log|qemu log|network pcap' ;;" \
  "matrix runner must require memory isolation map artifacts"
require_literal "$runner" "operator-recovery-runbook) echo 'make x86_64-microkernel-operator-recovery-runbook' ;;" \
  "matrix runner must dispatch the operator recovery runbook scenario"
require_literal "$runner" "operator-recovery-runbook) echo 'x86_64 microkernel operator recovery runbook proof passed.' ;;" \
  "matrix runner must require the operator recovery runbook positive marker"
require_literal "$runner" "operator-recovery-runbook) echo 'operator recovery summary|client log|serial log|qemu log|network pcap' ;;" \
  "matrix runner must require operator recovery runbook artifacts"
require_literal "$runner" "stream-session-state) echo 'make x86_64-microkernel-stream-session-state' ;;" \
  "matrix runner must dispatch the stream session state scenario"
require_literal "$runner" "stream-session-state) echo 'x86_64 microkernel stream session state proof passed.' ;;" \
  "matrix runner must require the stream session state positive marker"
require_literal "$runner" "stream-session-state) echo 'stream session state summary|client log|serial log|qemu log|network pcap' ;;" \
  "matrix runner must require stream session state artifacts"
require_literal "$runner" "status-snapshot-consistency) echo 'make x86_64-microkernel-status-snapshot-consistency' ;;" \
  "matrix runner must dispatch the status snapshot consistency scenario"
require_literal "$runner" "status-snapshot-consistency) echo 'x86_64 microkernel status snapshot consistency proof passed.' ;;" \
  "matrix runner must require the status snapshot consistency positive marker"
require_literal "$runner" "status-snapshot-consistency) echo 'status snapshot consistency summary|client log|serial log|qemu log|network pcap' ;;" \
  "matrix runner must require status snapshot consistency artifacts"
require_literal "$runner" "status-snapshot-contract-v2) echo 'make x86_64-microkernel-status-snapshot-contract-v2' ;;" \
  "matrix runner must dispatch the status snapshot contract v2 scenario"
require_literal "$runner" "status-snapshot-contract-v2) echo 'x86_64 microkernel status snapshot contract v2 proof passed.' ;;" \
  "matrix runner must require the status snapshot contract v2 positive marker"
require_literal "$runner" "status-snapshot-contract-v2) echo 'status snapshot v2 summary|client log|serial log|qemu log|network pcap' ;;" \
  "matrix runner must require status snapshot contract v2 artifacts"
require_literal "$runner" "filesystem-read-matrix) echo 'make x86_64-microkernel-filesystem-read-matrix' ;;" \
  "matrix runner must dispatch the filesystem read matrix scenario"
require_literal "$runner" "filesystem-read-matrix) echo 'x86_64 microkernel filesystem read matrix proof passed.' ;;" \
  "matrix runner must require the filesystem read matrix positive marker"
require_literal "$runner" "filesystem-read-matrix) echo 'filesystem read matrix summary|client log|serial log|qemu log|network pcap' ;;" \
  "matrix runner must require filesystem read matrix artifacts"
require_literal "$runner" 'fault-and-recovery-preconditions' \
  "matrix runner must dispatch the fault and recovery preconditions scenario"
require_literal "$runner" "fault-and-recovery-preconditions) echo 'make x86_64-microkernel-fault-and-recovery-preconditions' ;;" \
  "matrix runner must wire the fault and recovery preconditions command"
require_literal "$runner" "fault-and-recovery-preconditions) echo 'x86_64 microkernel fault and recovery preconditions proof passed.' ;;" \
  "matrix runner must require the fault and recovery preconditions positive marker"
require_literal "$runner" "fault-and-recovery-preconditions) echo 'fault and recovery preconditions summary|client log|serial log|qemu log|network pcap' ;;" \
  "matrix runner must require fault and recovery preconditions artifacts"
require_literal "$runner" "fault-and-recovery-preconditions) echo 'make x86_64-microkernel-fault-and-recovery-preconditions' ;;" \
  "matrix runner must dispatch the fault and recovery preconditions scenario"
require_literal "$runner" "fault-and-recovery-preconditions) echo 'x86_64 microkernel fault and recovery preconditions proof passed.' ;;" \
  "matrix runner must require the fault and recovery preconditions positive marker"
require_literal "$runner" "fault-and-recovery-preconditions) echo 'fault and recovery preconditions summary|client log|serial log|qemu log|network pcap' ;;" \
  "matrix runner must require fault and recovery preconditions artifacts"
require_literal "$makefile" 'x86_64-microkernel-service-timeout-stale-reply-policy-contract:' \
  "Makefile must expose the service timeout/stale reply policy contract target"
require_literal "$makefile" 'x86_64-microkernel-service-timeout-stale-reply-policy: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-service-timeout-stale-reply-policy-contract' \
  "Makefile must expose the service timeout/stale reply policy proof target"
require_literal "$runner" "service-timeout-stale-reply-policy) echo 'make x86_64-microkernel-service-timeout-stale-reply-policy' ;;" \
  "matrix runner must dispatch the service timeout/stale reply policy scenario"
require_literal "$runner" "service-timeout-stale-reply-policy) echo 'x86_64 microkernel service timeout stale reply policy proof passed.' ;;" \
  "matrix runner must require the service timeout/stale reply policy positive marker"
require_literal "$runner" "service-timeout-stale-reply-policy) echo 'service timeout stale reply policy summary|client log|serial log|qemu log|network pcap' ;;" \
  "matrix runner must require service timeout/stale reply policy artifacts"
require_literal "$runner" "service-route-table) echo 'make x86_64-microkernel-service-route-table' ;;" \
  "matrix runner must dispatch the service route table scenario"
require_literal "$runner" "service-route-table) echo 'x86_64 microkernel service route table proof passed.' ;;" \
  "matrix runner must require the service route table positive marker"
require_literal "$runner" "service-route-table) echo 'service route table summary|client log|serial log|qemu log|network pcap' ;;" \
  "matrix runner must require service route table artifacts"
require_literal "$runner" 'service-route-table) echo '\''make x86_64-microkernel-service-route-table'\''' \
  "matrix runner must dispatch the service route table scenario"
require_literal "$runner" 'service-route-table) echo '\''x86_64 microkernel service route table proof passed.'\''' \
  "matrix runner must emit the service route table proof marker"
require_literal "$runner" 'service-route-table) echo '\''service route table summary|client log|serial log|qemu log|network pcap'\''' \
  "matrix runner must keep the service route table artifact pattern"
require_literal "$runner" 'service-ipc-audit) echo '\''make x86_64-microkernel-service-ipc-audit'\''' \
  "matrix runner must dispatch the service IPC audit scenario"
require_literal "$runner" 'service-ipc-audit) echo '\''x86_64 microkernel service IPC ledger proof passed.'\''' \
  "matrix runner must emit the service IPC audit proof marker"
require_literal "$runner" 'service-ipc-audit) echo '\''service IPC audit summary|client log|serial log|qemu log|network pcap'\''' \
  "matrix runner must keep the service IPC audit artifact pattern"
require_literal "$runner" 'service-boundary-audit) echo '\''make x86_64-microkernel-service-boundary-audit'\''' \
  "matrix runner must dispatch the service boundary audit scenario"
require_literal "$runner" 'service-boundary-audit) echo '\''x86_64 microkernel service boundary audit proof passed.'\''' \
  "matrix runner must emit the service boundary audit proof marker"
require_literal "$runner" 'service-boundary-audit) echo '\''service boundary audit summary|client log|serial log|qemu log|network pcap'\''' \
  "matrix runner must keep the service boundary audit artifact pattern"
require_literal "$runner" 'status-route-unification) echo '\''make x86_64-microkernel-status-route-unification'\''' \
  "matrix runner must dispatch the status route unification scenario"
require_literal "$runner" 'status-route-unification) echo '\''x86_64 microkernel status route compatibility proof passed.'\''' \
  "matrix runner must emit the status route compatibility proof marker"
require_literal "$runner" 'status-route-unification) echo '\''status route unification summary|client log|serial log|qemu log|network pcap'\''' \
  "matrix runner must keep the status route unification artifact pattern"
require_literal "$runner" 'service-route-table|service-ipc-audit|service-boundary-audit|status-route-unification' \
  "matrix runner must include the service-route validation family in scenario classification"
require_literal "$runner" 'service-route-table) echo '\''service route table summary|client log|serial log|qemu log|network pcap'\''' \
  "matrix runner must keep the service route table artifact pattern"

reject_regex "$runner" '(^|[^A-Z0-9_])--tls-proof([^A-Z0-9_]|$)' \
  "runner must not expose a fake TLS proof mode"
reject_regex "$runner" 'DP_MICROKERNEL_TLS_PROOF' \
  "runner must not accept a fake TLS environment override"
reject_regex "$runner" 'curl[[:space:]]+-k' \
  "runner must not weaken validation with curl -k"
reject_regex "$runner" 'HTTPS|OpenSSL|certificate|private[[:space:]]+key' \
  "runner must not add TLS/HTTPS/certificate tooling"
reject_regex "$runner" 'restart[[:space:]]+fs|restart[[:space:]]*$' \
  "runner must not add restart commands in the validation matrix slice"
reject_regex "$runner" 'generic[[:space:]]+socket[[:space:]]+api|socket[[:space:]]+api' \
  "runner must not introduce a generic socket API"
reject_regex "$runner" 'grep[[:space:]]+-Fq[[:space:]]+"\$marker"[[:space:]]+"\$scenario_log"' \
  "runner must not bypass the fail-closed marker helper"
reject_regex "$runner" 'benchmark[[:space:]]+tuning|retun(e|ing)|calibration' \
  "runner must not add benchmark tuning to the validation matrix"
reject_regex "$fat32_runner" 'debugger[[:space:]]+command|raw[[:space:]]+memory[[:space:]]+command|MMIO[[:space:]]+command|page-table[[:space:]]+command' \
  "runner must not add debugger, raw memory, MMIO, or page-table operator commands"

echo "x86_64 microkernel validation matrix contract guard passed."
