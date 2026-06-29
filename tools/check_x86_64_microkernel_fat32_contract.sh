#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
kernel="crates/dataplane-x86_64-microkernel-smoke/src/kernel.rs"
cli="crates/dataplane-x86_64-microkernel-smoke/src/cli.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
builder="tools/microkernel_make_fat32_image.py"
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

require_regex() {
  local file="$1"
  local pattern="$2"
  local note="$3"
  grep -Eq -- "$pattern" "$file" || fail "$note"
}

reject_literal() {
  local file="$1"
  local literal="$2"
  local note="$3"
  if grep -Fq -- "$literal" "$file"; then
    fail "$note"
  fi
}

for file in "$main" "$kernel" "$runner" "$builder" "$makefile"; do
  require_file "$file"
done

echo "=== x86_64 Microkernel FAT32 Contract Guard ==="

for literal in \
  'target="x86_64-unknown-none"' \
  'mnt_root="/home/user/mnt/dataplane"' \
  'fs_img="$mnt_root/microkernel-fat32.img"' \
  'log_root="$(dp_default_log_root DP_MICROKERNEL_FAT32_LOG_ROOT)"' \
  'tools/microkernel_make_fat32_image.py "$fs_img"' \
  'COMMAND_CHECKS = [' \
  'for command, expected in COMMAND_CHECKS:' \
  'send_command(out, command, expected)' \
  'DP_EXPECTED_BOOT_MARKERS=' \
  'DP_EXPECTED_FINAL_MARKERS=' \
  'DP_EXPECTED_NETWORK_MARKERS=' \
  'require_host_evidence_for_marker "DPMK:NET-ARP-REPLY" "seen_arp_reply=true"' \
  'require_pcap_evidence_for_marker "DPMK:NET-UDP-ECHO" "4450554450523030"' \
  'require_host_evidence_for_marker "DPMK:HTTP-GET-OK" "http_get_index_ok=true"' \
  'DPMK:NET-TIMER-MAXGAP:'; do
  require_literal "$runner" "$literal" "runner must preserve current FAT32 smoke evidence: $literal"
done

require_regex "$runner" 'def send_command\(out, command, expected, timeout_seconds=[^)]*\):' \
  "runner must preserve current FAT32 smoke evidence: timeout-bearing send_command signature"

for command in \
  '"help"' \
  '"tasks"' \
  '"fs ls /"' \
  '"fs cat /HELLO.TXT"' \
  '"fs cat /INDEX.HTM"' \
  '"fs stat /HELLO.TXT"' \
  '"fs stat /INDEX.HTM"' \
  '"fs stat /MISSING.TXT"' \
  '"fs stat /THISNAMEISTOOLONG.TXT"' \
  '"fs write /OUT.TXT append"' \
  '"task timer"' \
  '"task fs"' \
  '"task block"' \
  '"task tcpip"' \
  '"queues"'; do
  require_literal "$runner" "$command" "runner must send current CLI command $command"
  require_literal "$cli" "$command" "guest CLI parser must recognize current command $command"
done

for output in \
  'DPCLI:HELP help tasks fs ls / fs cat /HELLO.TXT fs cat /INDEX.HTM' \
  'DPCLI:HELP-FS fs stat /HELLO.TXT fs stat /INDEX.HTM fs stat /MISSING.TXT fs stat /THISNAMEISTOOLONG.TXT fs write /OUT.TXT append' \
  'DPCLI:HELP-OPS task timer task fs task block task tcpip queues' \
  'DPCLI:TASKS timer=ready cli=ready fs=ready block=ready' \
  'DPCLI:TASKS-NET net=candidate tcpip=candidate' \
  'DPCLI:LS / HELLO.TXT 40 INDEX.HTM 110' \
  'DPCLI:STAT /HELLO.TXT cluster=3 size=40 readonly=1' \
  'DPCLI:STAT /INDEX.HTM cluster=4 size=110 readonly=1' \
  'DPCLI:FS-ERR /MISSING.TXT unsupported-path' \
  'DPCLI:FS-ERR /THISNAMEISTOOLONG.TXT long-filename' \
  'DPCLI:FS-ERR /OUT.TXT unsupported-write-shape' \
  'DPCLI:TASK timer id=1 endpoint=1 status=ready' \
  'DPCLI:TASK fs id=3 endpoint=3 status=ready' \
  'DPCLI:TASK block id=4 endpoint=4 status=ready' \
  'DPCLI:TASK tcpip id=6 endpoint=6 status=ready' \
  'DPCLI:QUEUES timer=0 cli=0 fs=0 block=0 net=0 tcpip=0 http=0 dhcp=0' \
  'DPMK:CLI-OPERATOR-OK' \
  'DPMK:FS-HARDENING-OK'; do
  require_literal "$runner" "$output" "runner must verify current deterministic CLI output: $output"
done

for cli_output in \
  'DPCLI:HELP help tasks fs ls / fs cat /HELLO.TXT fs cat /INDEX.HTM' \
  'DPCLI:HELP-FS fs stat /HELLO.TXT fs stat /INDEX.HTM fs stat /MISSING.TXT fs stat /THISNAMEISTOOLONG.TXT fs write /OUT.TXT append' \
  'DPCLI:HELP-OPS task timer task fs task block task tcpip queues' \
  'DPCLI:TASK ' \
  'DPCLI:QUEUES timer=' \
  'DPMK:CLI-OPERATOR-OK'; do
  require_literal "$cli" "$cli_output" "guest must emit current deterministic CLI output component: $cli_output"
done

for main_output in \
  'DPCLI:LS / HELLO.TXT ' \
  'INDEX.HTM ' \
  'DPCLI:STAT ' \
  'unsupported-path' \
  'long-filename' \
  'unsupported-write-shape' \
  'DPMK:FS-HARDENING-OK'; do
  require_literal "$kernel" "$main_output" "guest must emit current deterministic CLI output component: $main_output"
done

for marker in \
  'DPMK:BOOT' \
  'DPMK:TIMER' \
  'DPMK:BLK-READY' \
  'DPMK:BLK-SECTOR0-OK' \
  'DPMK:FS-READY' \
  'DPMK:FS-LS-ROOT-OK' \
  'DPMK:FS-HELLO-OK' \
  'DPMK:FS-INDEX-OK' \
  'DPMK:IPC-OK' \
  'DPMK:NET-SPLIT-READY' \
  'DPMK:CLI-READY' \
  'DPMK:CLI-INPUT-READY' \
  'DPMK:CLI-COMMANDS-OK' \
  'DPMK:FS-HARDENING-OK' \
  'DPMK:FAULT-CONTAINED' \
  'DPMK:OK'; do
  require_literal "$runner" "$marker" "runner must verify current marker $marker"
  require_literal "$kernel" "$marker" "guest must emit current marker $marker"
done

for literal in \
  'HELLO_NAME = b"HELLO   TXT"' \
  'INDEX_NAME = b"INDEX   HTM"' \
  'HELLO_CONTENT = b"hello from dataplane microkernel fat32' \
  'INDEX_CONTENT = (' \
  'b"<!doctype html><html><head><title>dataplane</title></head>"' \
  'b"<body><h1>dataplane microkernel</h1></body></html>' \
  'write_file(image, HELLO_CLUSTER, HELLO_CONTENT)' \
  'write_file(image, INDEX_CLUSTER, INDEX_CONTENT)'; do
  require_literal "$builder" "$literal" "FAT32 image builder must preserve current fixture literal: $literal"
done

require_literal "$makefile" 'x86_64-microkernel-fat32-contract:' \
  "Makefile must expose FAT32 contract target"
require_literal "$makefile" './tools/check_x86_64_microkernel_fat32_contract.sh' \
  "Makefile must run FAT32 contract guard"
require_literal "$makefile" 'x86_64-microkernel-fat32-smoke:' \
  "Makefile must expose FAT32 smoke target"
require_literal "$makefile" './tools/x86_64_microkernel_fat32_run.sh' \
  "Makefile must run FAT32 smoke runner"

reject_literal "$runner" 'sock.sendall(b"help\n")' \
  "runner must not treat a lone help write as sufficient CLI proof"
reject_literal "$runner" 'curl -k' \
  "FAT32 smoke runner must not use insecure HTTPS/TLS shortcuts"
reject_literal "$kernel" 'DPCLI:ROUTES routes=' \
  "FAT32 smoke guard must not require route table output"
reject_literal "$kernel" 'DPCLI:STATUS-BEGIN' \
  "FAT32 smoke guard must not require the later status snapshot packet"
reject_literal "$kernel" 'DPCLI:FAULTS timer=' \
  "FAT32 smoke guard must not require the later fault-counters packet"

echo "x86_64 microkernel FAT32 contract OK"
