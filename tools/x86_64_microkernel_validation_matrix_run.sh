#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

source "tools/x86_64_microkernel_validation_matrix_lib.sh"

validation_matrix_anchor='fat32-smoke|curl-proof|fairness|dhcp|fat32-write|http-backpressure|fs-service-boundary|fs-policy-and-directory-slice|capability-fault-policy|operator-appliance-surface|operator-appliance-consolidation|fault-and-recovery-preconditions|service-timeout-stale-reply-policy|service-route-table|service-ipc-audit|service-boundary-audit|status-route-unification|mixed-appliance-fairness|storage-integrity-frontier|http-policy-matrix|bounded-stream-session-refresh|scheduler-fairness-load|memory-isolation-map|operator-recovery-runbook|stream-session-state|status-snapshot-consistency|status-snapshot-contract-v2|filesystem-read-matrix|nontls-network-negative-matrix|network-control-plane-slice|bounded-tcp-negative|nontls-network-service)'

log_root="$(dp_default_log_root DP_VALIDATION_MATRIX_LOG_ROOT)"
run_id="$(dp_new_run_id)"
summary="$(dp_artifact_path "$log_root" "x86_64-microkernel-validation-matrix" "$run_id" "summary")"
mkdir -p "$log_root"

usage() {
  cat <<'USAGE'
usage: tools/x86_64_microkernel_validation_matrix_run.sh [--quick|--full|--scenario NAME...]

Scenarios:
  fat32-smoke
  curl-proof
  fairness
  dhcp
  fat32-write
  http-backpressure
  fs-service-boundary
  capability-fault-policy
  operator-appliance-surface
  storage-integrity-frontier
  scheduler-fairness-load
  memory-isolation-map
  operator-recovery-runbook
  stream-session-state
  status-snapshot-consistency
  status-snapshot-contract-v2
  filesystem-read-matrix
  nontls-network-negative-matrix
  storage-service-counters
  validation-failure-injection
  runner-fail-closed-expansion
  http-policy-matrix
  bounded-stream-session-refresh
  bounded-tcp-negative
  nontls-network-service
  network-control-plane-slice
  operator-appliance-consolidation
  fault-and-recovery-preconditions
  service-timeout-stale-reply-policy
  service-route-table
  service-ipc-audit
  service-boundary-audit
  status-route-unification
  mixed-appliance-fairness
  appliance-load-fairness-pack
  x86_64-virtio-smoke
  raspi3b-mmu-smoke
  qemu-cortexm0-smoke
  rp2040-check
USAGE
}

scenarios=()
if [[ $# -eq 0 || "${1:-}" == "--quick" ]]; then
  scenarios=(fat32-smoke fs-service-boundary fs-policy-and-directory-slice capability-fault-policy operator-appliance-surface storage-integrity-frontier qemu-cortexm0-smoke rp2040-check)
  shift $(( $# > 0 ? 1 : 0 ))
elif [[ "${1:-}" == "--full" ]]; then
  scenarios=(
    fat32-smoke
    curl-proof
    fairness
    dhcp
    fat32-write
    http-backpressure
    fs-service-boundary
    fs-policy-and-directory-slice
    capability-fault-policy
    operator-appliance-surface
    storage-integrity-frontier
    scheduler-fairness-load
    memory-isolation-map
    operator-recovery-runbook
    stream-session-state
    status-snapshot-consistency
    status-snapshot-contract-v2
    filesystem-read-matrix
    nontls-network-negative-matrix
    storage-service-counters
    validation-failure-injection
    runner-fail-closed-expansion
    http-policy-matrix
    bounded-stream-session-refresh
    bounded-tcp-negative
    nontls-network-service
    network-control-plane-slice
    operator-appliance-consolidation
    fault-and-recovery-preconditions
    service-timeout-stale-reply-policy
    service-route-table
    service-ipc-audit
    service-boundary-audit
    status-route-unification
    mixed-appliance-fairness
    x86_64-virtio-smoke
    raspi3b-mmu-smoke
    qemu-cortexm0-smoke
    rp2040-check
  )
  shift
elif [[ "${1:-}" == "--scenario" ]]; then
  shift
  if [[ $# -eq 0 ]]; then
    echo "FAIL: --scenario requires at least one scenario name" >&2
    usage >&2
    exit 2
  fi
  scenarios=("$@")
  set --
else
  usage >&2
  exit 2
fi

if [[ $# -ne 0 ]]; then
  usage >&2
  exit 2
fi

scenario_command() {
  case "$1" in
    fat32-smoke) echo 'make x86_64-microkernel-fat32-smoke' ;;
    curl-proof) echo 'make x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract && ./tools/x86_64_microkernel_fat32_run.sh --curl-proof' ;;
    fairness) echo 'make x86_64-microkernel-fat32-fairness' ;;
    dhcp) echo 'make x86_64-microkernel-fat32-dhcp' ;;
    fat32-write) echo 'make x86_64-microkernel-fat32-write' ;;
    http-backpressure) echo 'make x86_64-microkernel-http-backpressure' ;;
    fs-service-boundary) echo 'make x86_64-microkernel-fs-service-boundary' ;;
    fs-policy-and-directory-slice) echo 'make x86_64-microkernel-fs-policy-and-directory-slice' ;;
    capability-fault-policy) echo 'make x86_64-microkernel-capability-fault-policy' ;;
    operator-appliance-surface) echo 'make x86_64-microkernel-operator-appliance-surface' ;;
    storage-integrity-frontier) echo 'make x86_64-microkernel-storage-integrity-frontier' ;;
    scheduler-fairness-load) echo 'make x86_64-microkernel-scheduler-fairness-load' ;;
    memory-isolation-map) echo 'make x86_64-microkernel-memory-isolation-map' ;;
    operator-recovery-runbook) echo 'make x86_64-microkernel-operator-recovery-runbook' ;;
    stream-session-state) echo 'make x86_64-microkernel-stream-session-state' ;;
    status-snapshot-consistency) echo 'make x86_64-microkernel-status-snapshot-consistency' ;;
    status-snapshot-contract-v2) echo 'make x86_64-microkernel-status-snapshot-contract-v2' ;;
    filesystem-read-matrix) echo 'make x86_64-microkernel-filesystem-read-matrix' ;;
    nontls-network-negative-matrix) echo 'make x86_64-microkernel-nontls-network-negative-matrix' ;;
    network-control-plane-slice) echo 'make x86_64-microkernel-network-control-plane-slice' ;;
    operator-appliance-consolidation) echo 'make x86_64-microkernel-operator-appliance-consolidation' ;;
    fault-and-recovery-preconditions) echo 'make x86_64-microkernel-fault-and-recovery-preconditions' ;;
    service-timeout-stale-reply-policy) echo 'make x86_64-microkernel-service-timeout-stale-reply-policy' ;;
    service-route-table) echo 'make x86_64-microkernel-service-route-table' ;;
    service-ipc-audit) echo 'make x86_64-microkernel-service-ipc-audit' ;;
    service-boundary-audit) echo 'make x86_64-microkernel-service-boundary-audit' ;;
    status-route-unification) echo 'make x86_64-microkernel-status-route-unification' ;;
    mixed-appliance-fairness) echo 'make x86_64-microkernel-mixed-appliance-fairness' ;;
    appliance-load-fairness-pack) echo 'make x86_64-microkernel-appliance-load-fairness-pack' ;;
    storage-service-counters) echo 'make x86_64-microkernel-storage-service-counters' ;;
    validation-failure-injection) echo 'make x86_64-microkernel-validation-failure-injection' ;;
    runner-fail-closed-expansion) echo 'make x86_64-microkernel-runner-fail-closed-expansion' ;;
    http-policy-matrix) echo 'make x86_64-microkernel-http-policy-matrix' ;;
    bounded-stream-session-refresh) echo 'make x86_64-microkernel-bounded-stream-session-refresh' ;;
    bounded-tcp-negative) echo 'make x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-bounded-tcp-stream-contract && ./tools/x86_64_microkernel_fat32_run.sh --bounded-tcp-negative-proof' ;;
    nontls-network-service) echo 'make x86_64-microkernel-nontls-network-service-matrix' ;;
    x86_64-virtio-smoke) echo 'make x86_64-virtio-smoke' ;;
    raspi3b-mmu-smoke) echo 'make raspi3b-mmu-smoke' ;;
    qemu-cortexm0-smoke) echo 'make qemu-cortexm0-smoke' ;;
    rp2040-check) echo 'make rp2040-check' ;;
    *) return 1 ;;
  esac
}

scenario_class() {
  # x86_64-qemu-microkernel remains the anchor label for QEMU-backed x86_64 microkernel scenarios.
  # fat32-smoke|curl-proof|fairness|dhcp|fat32-write|http-backpressure|fs-service-boundary|fs-policy-and-directory-slice|capability-fault-policy|operator-appliance-surface|operator-appliance-consolidation|fault-and-recovery-preconditions|service-timeout-stale-reply-policy|service-route-table|service-ipc-audit|service-boundary-audit|status-route-unification|mixed-appliance-fairness|appliance-load-fairness-pack|storage-integrity-frontier|http-policy-matrix|bounded-stream-session-refresh|scheduler-fairness-load|memory-isolation-map|operator-recovery-runbook|stream-session-state|status-snapshot-consistency|status-snapshot-contract-v2|filesystem-read-matrix|nontls-network-negative-matrix|network-control-plane-slice|bounded-tcp-negative|nontls-network-service
  case "$1" in
    storage-service-counters) echo 'x86_64-source-scan' ;;
    fat32-smoke|curl-proof|fairness|dhcp|fat32-write|http-backpressure|fs-service-boundary|fs-policy-and-directory-slice|capability-fault-policy|operator-appliance-surface|operator-appliance-consolidation|fault-and-recovery-preconditions|service-timeout-stale-reply-policy|service-route-table|service-ipc-audit|service-boundary-audit|status-route-unification|mixed-appliance-fairness|appliance-load-fairness-pack|storage-integrity-frontier|http-policy-matrix|bounded-stream-session-refresh|scheduler-fairness-load|memory-isolation-map|operator-recovery-runbook|stream-session-state|status-snapshot-consistency|status-snapshot-contract-v2|filesystem-read-matrix|nontls-network-negative-matrix|network-control-plane-slice|bounded-tcp-negative|nontls-network-service)
      echo 'x86_64-qemu-microkernel'
      ;;
    validation-failure-injection) echo 'validation-matrix-negative-proof' ;;
    runner-fail-closed-expansion) echo 'validation-matrix-negative-proof' ;;
    x86_64-virtio-smoke) echo 'x86_64-qemu-virtio' ;;
    raspi3b-mmu-smoke) echo 'raspi3b-qemu-mmu' ;;
    qemu-cortexm0-smoke) echo 'cortexm0-qemu-noalloc' ;;
    rp2040-check) echo 'rp2040-compile-noalloc' ;;
    *) return 1 ;;
  esac
}

scenario_marker() {
  case "$1" in
    fat32-smoke) echo 'x86_64 microkernel FAT32 smoke passed.' ;;
    curl-proof) echo 'x86_64 microkernel FAT32 curl proof passed.' ;;
    fairness) echo 'x86_64 microkernel FAT32 fairness proof passed.' ;;
    dhcp) echo 'x86_64 microkernel FAT32 DHCP proof passed.' ;;
    fat32-write) echo 'x86_64 microkernel FAT32 write proof passed.' ;;
    http-backpressure) echo 'x86_64 microkernel HTTP backpressure proof passed.' ;;
    fs-service-boundary) echo 'x86_64 microkernel FS service boundary proof passed.' ;;
    fs-policy-and-directory-slice) echo 'x86_64 microkernel fs policy and directory slice proof passed.' ;;
    capability-fault-policy) echo 'x86_64 microkernel FAT32 smoke passed.' ;;
    operator-appliance-surface) echo 'x86_64 microkernel operator appliance source-scan proof passed.' ;;
    storage-integrity-frontier) echo 'x86_64 microkernel storage integrity proof passed.' ;;
    scheduler-fairness-load) echo 'x86_64 microkernel scheduler fairness load proof passed.' ;;
    memory-isolation-map) echo 'x86_64 microkernel memory isolation map proof passed.' ;;
    operator-recovery-runbook) echo 'x86_64 microkernel operator recovery runbook proof passed.' ;;
    stream-session-state) echo 'x86_64 microkernel stream session state proof passed.' ;;
    status-snapshot-consistency) echo 'x86_64 microkernel status snapshot consistency proof passed.' ;;
    status-snapshot-contract-v2) echo 'x86_64 microkernel status snapshot contract v2 proof passed.' ;;
    filesystem-read-matrix) echo 'x86_64 microkernel filesystem read matrix proof passed.' ;;
    nontls-network-negative-matrix) echo 'x86_64 microkernel non-TLS network negative matrix passed.' ;;
    network-control-plane-slice) echo 'x86_64 microkernel network control-plane slice proof passed.' ;;
    operator-appliance-consolidation) echo 'x86_64 microkernel operator appliance consolidation proof passed.' ;;
    fault-and-recovery-preconditions) echo 'x86_64 microkernel fault and recovery preconditions proof passed.' ;;
    service-timeout-stale-reply-policy) echo 'x86_64 microkernel service timeout stale reply policy proof passed.' ;;
    service-route-table) echo 'x86_64 microkernel service route table proof passed.' ;;
    service-ipc-audit) echo 'x86_64 microkernel service IPC ledger proof passed.' ;;
    service-boundary-audit) echo 'x86_64 microkernel service boundary audit proof passed.' ;;
    status-route-unification) echo 'x86_64 microkernel status route compatibility proof passed.' ;;
    mixed-appliance-fairness) echo 'x86_64 microkernel mixed appliance fairness proof passed.' ;;
    appliance-load-fairness-pack) echo 'x86_64 microkernel appliance load fairness pack passed.' ;;
    storage-service-counters) echo 'x86_64 microkernel storage service counters source-scan proof passed.' ;;
    validation-failure-injection) echo 'x86_64 microkernel validation failure injection proof passed.' ;;
    runner-fail-closed-expansion) echo 'x86_64 microkernel runner fail-closed expansion proof passed.' ;;
    http-policy-matrix) echo 'x86_64 microkernel HTTP policy matrix proof passed.' ;;
    bounded-stream-session-refresh) echo 'x86_64 microkernel bounded stream session refresh proof passed.' ;;
    bounded-tcp-negative) echo 'x86_64 microkernel bounded TCP negative proof passed.' ;;
    nontls-network-service) echo 'x86_64 microkernel non-TLS network service matrix passed.' ;;
    x86_64-virtio-smoke) echo 'x86_64 virtio network MMU smoke passed.' ;;
    raspi3b-mmu-smoke) echo 'Raspi3B MMU protected shard smoke passed.' ;;
    qemu-cortexm0-smoke) echo 'QEMU Cortex-M0 smoke passed.' ;;
    rp2040-check) echo 'RP2040 no-alloc smoke contract guard passed.' ;;
    *) return 1 ;;
  esac
}

scenario_artifact_labels() {
  case "$1" in
    fat32-smoke) echo 'serial log|qemu log|network pcap' ;;
    curl-proof) echo 'curl pcap|serial log|qemu log' ;;
    fairness) echo 'fairness log|fairness pcap|serial log|qemu log' ;;
    dhcp) echo 'dhcp log|dhcp pcap|serial log|qemu log' ;;
    fat32-write) echo 'write log|write readback|write image after sha|serial log|qemu log' ;;
    http-backpressure) echo 'HTTP backpressure log|HTTP backpressure pcap|serial log|qemu log' ;;
    fs-service-boundary) echo 'FS service boundary log|FS service boundary pcap|serial log|qemu log' ;;
    fs-policy-and-directory-slice) echo 'fs policy and directory slice summary|client log|serial log|qemu log|network pcap' ;;
    capability-fault-policy) echo 'serial log|qemu log|client log|network pcap' ;;
    operator-appliance-surface) echo 'summary|source scan' ;;
    storage-integrity-frontier) echo 'client log|serial log|qemu log|network pcap' ;;
    scheduler-fairness-load) echo 'scheduler fairness summary|fairness log|fairness client log|fairness curl log|fairness pcap|serial log|qemu log' ;;
    memory-isolation-map) echo 'memory isolation summary|client log|serial log|qemu log|network pcap' ;;
    operator-recovery-runbook) echo 'operator recovery summary|client log|serial log|qemu log|network pcap' ;;
    stream-session-state) echo 'stream session state summary|client log|serial log|qemu log|network pcap' ;;
    status-snapshot-consistency) echo 'status snapshot consistency summary|client log|serial log|qemu log|network pcap' ;;
    status-snapshot-contract-v2) echo 'status snapshot v2 summary|client log|serial log|qemu log|network pcap' ;;
    filesystem-read-matrix) echo 'filesystem read matrix summary|client log|serial log|qemu log|network pcap' ;;
    nontls-network-negative-matrix) echo 'non-TLS network negative summary|network host log|network pcap|serial log|qemu log' ;;
    network-control-plane-slice) echo 'network control-plane slice summary|network host log|network pcap|serial log|qemu log' ;;
    operator-appliance-consolidation) echo 'operator appliance consolidation summary|client log|serial log|qemu log|network pcap' ;;
    fault-and-recovery-preconditions) echo 'fault and recovery preconditions summary|client log|serial log|qemu log|network pcap' ;;
    service-timeout-stale-reply-policy) echo 'service timeout stale reply policy summary|client log|serial log|qemu log|network pcap' ;;
    service-route-table) echo 'service route table summary|client log|serial log|qemu log|network pcap' ;;
    service-ipc-audit) echo 'service IPC audit summary|client log|serial log|qemu log|network pcap' ;;
    service-boundary-audit) echo 'service boundary audit summary|client log|serial log|qemu log|network pcap' ;;
    status-route-unification) echo 'status route unification summary|client log|serial log|qemu log|network pcap' ;;
    mixed-appliance-fairness) echo 'mixed appliance fairness summary|client log|network host log|mixed appliance fairness pcap|serial log|qemu log' ;;
    appliance-load-fairness-pack) echo 'appliance load fairness pack summary|http-heavy summary|cli-heavy summary|fat32-heavy summary|network-heavy summary|timer-heavy summary|contained-fault summary' ;;
    storage-service-counters) echo 'storage service counters summary|source scan' ;;
    validation-failure-injection) echo 'validation failure injection summary|failure injection log' ;;
    runner-fail-closed-expansion) echo 'runner fail-closed expansion summary|failure injection log' ;;
    http-policy-matrix) echo 'HTTP policy matrix summary|curl log|curl pcap|operator client log|operator network pcap' ;;
    bounded-stream-session-refresh) echo 'bounded stream session refresh summary|stream session state summary|stream session client log|stream session serial log|stream session pcap|HTTP policy matrix summary|HTTP policy curl pcap' ;;
    bounded-tcp-negative) echo 'bounded TCP log|bounded TCP pcap|serial log|qemu log' ;;
    nontls-network-service) echo 'network host log|network pcap|serial log|qemu log' ;;
    x86_64-virtio-smoke) echo '' ;;
    raspi3b-mmu-smoke) echo '' ;;
    qemu-cortexm0-smoke) echo '' ;;
    rp2040-check) echo '' ;;
    *) return 1 ;;
  esac
}

extract_artifact() {
  dp_extract_artifact "$@"
}

record_summary_header() {
  {
    echo "validation_matrix_run_id=$run_id"
    echo "validation_matrix_log_root=$log_root"
    echo "validation_matrix_summary=$summary"
    echo "validation_matrix_scenarios=${scenarios[*]}"
    echo "validation_matrix_note=host-visible artifacts are primary for network/filesystem scenarios"
    echo
  } >"$summary"
}

run_scenario() {
  local scenario="$1"
  local command marker evidence_class labels scenario_log status metadata_status
  metadata_status=0
  command="$(scenario_command "$scenario")" || metadata_status=$?
  if [[ "$metadata_status" -ne 0 || -z "$command" ]]; then
    echo "FAIL: unknown scenario: $scenario" >&2
    return 1
  fi
  metadata_status=0
  marker="$(scenario_marker "$scenario")" || metadata_status=$?
  if [[ "$metadata_status" -ne 0 || -z "$marker" ]]; then
    echo "FAIL: scenario $scenario has no positive marker" >&2
    return 1
  fi
  metadata_status=0
  evidence_class="$(scenario_class "$scenario")" || metadata_status=$?
  if [[ "$metadata_status" -ne 0 || -z "$evidence_class" ]]; then
    echo "FAIL: scenario $scenario has no evidence class" >&2
    return 1
  fi
  metadata_status=0
  labels="$(scenario_artifact_labels "$scenario")" || metadata_status=$?
  if [[ "$metadata_status" -ne 0 ]]; then
    echo "FAIL: scenario $scenario has no artifact label metadata" >&2
    return 1
  fi
  scenario_log="$(dp_artifact_path "$log_root" "x86_64-microkernel-validation-matrix" "$run_id" "$scenario.log")"

  echo "matrix: running $scenario [$evidence_class]"
  status=0
  bash -lc "$command" >"$scenario_log" 2>&1 || status=$?

  {
    echo "scenario=$scenario"
    echo "evidence_class=$evidence_class"
    echo "command=$command"
    echo "command_log=$scenario_log"
    echo "exit_status=$status"
  } >>"$summary"

  if [[ "$status" -ne 0 ]]; then
    echo "result=fail" >>"$summary"
    echo >>"$summary"
    echo "FAIL: scenario $scenario exited with $status; log: $scenario_log" >&2
    return 1
  fi

  if ! dp_require_marker "$scenario_log" "$marker"; then
    echo "result=fail" >>"$summary"
    echo >>"$summary"
    echo "FAIL: scenario $scenario missing positive marker: $marker" >&2
    echo "log: $scenario_log" >&2
    return 1
  fi
  echo "positive_marker=$marker" >>"$summary"

  local artifact_count=0
  if [[ -n "$labels" ]]; then
    local old_ifs="$IFS"
    IFS='|'
    for label in $labels; do
      local artifact
      if ! artifact="$(extract_artifact "$scenario_log" "$label")"; then
        IFS="$old_ifs"
        echo "result=fail" >>"$summary"
        echo >>"$summary"
        echo "FAIL: scenario $scenario missing artifact label '$label' under /home/user/mnt/dataplane" >&2
        echo "log: $scenario_log" >&2
        return 1
      fi
      echo "artifact.$label=$artifact" >>"$summary"
      artifact_count=$((artifact_count + 1))
    done
    IFS="$old_ifs"
  fi
  echo "artifact_count=$artifact_count" >>"$summary"
  echo "result=pass" >>"$summary"
  echo >>"$summary"
}

record_summary_header

failures=0
for scenario in "${scenarios[@]}"; do
  if ! run_scenario "$scenario"; then
    failures=$((failures + 1))
  fi
done

{
  echo "validation_matrix_failures=$failures"
  if [[ "$failures" -eq 0 ]]; then
    echo "validation_matrix_result=pass"
  else
    echo "validation_matrix_result=fail"
  fi
} >>"$summary"

if [[ "$failures" -ne 0 ]]; then
  echo "x86_64 microkernel validation matrix failed."
  echo "summary: $summary"
  exit 1
fi

echo "x86_64 microkernel validation matrix passed."
echo "summary: $summary"
