#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

main="crates/dataplane-x86_64-virtio-smoke/src/main.rs"
src_dir="crates/dataplane-x86_64-virtio-smoke/src"
boot="crates/dataplane-x86_64-virtio-smoke/boot/boot.S"
stage2="crates/dataplane-x86_64-virtio-smoke/boot/stage2.S"
build_script="tools/x86_64_virtio_smoke_build.sh"
runner="tools/x86_64_virtio_smoke_run.sh"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

require_literal() {
  local file="$1"
  local needle="$2"
  local note="$3"
  rg -q --fixed-strings -- "$needle" "$file" || fail "$note"
}

echo "=== x86_64 Virtio Network MMU Smoke Contract Guard ==="

[[ -f "$main" ]] || fail "missing x86_64 virtio smoke main"
[[ -d "$src_dir" ]] || fail "missing x86_64 virtio smoke source directory"
[[ -f "$boot" ]] || fail "missing x86_64 boot sector"
[[ -f "$stage2" ]] || fail "missing x86_64 stage2 loader"
[[ -f "$build_script" ]] || fail "missing x86_64 virtio build script"
[[ -f "$runner" ]] || fail "missing x86_64 virtio runner"

require_literal Cargo.toml '"crates/dataplane-x86_64-virtio-smoke",' \
  "workspace must include dataplane-x86_64-virtio-smoke"
require_literal Makefile 'x86_64-virtio-smoke' \
  "Makefile must expose x86_64 virtio smoke targets"
require_literal "$build_script" 'x86_64-unknown-none' \
  "build script must target x86_64-unknown-none"
require_literal "$build_script" 'objcopy -O binary' \
  "build script must produce a flat x86 boot payload"
require_literal "$build_script" 'kernel_sectors=$(((kernel_size + 511) / 512))' \
  "build script must derive the exact kernel sector count"
require_literal "$build_script" '--defsym KERNEL_SECTORS="$kernel_sectors"' \
  "build script must pass the exact kernel sector count into stage2"
require_literal "$build_script" '-e stage2_start' \
  "build script must link stage2 with an explicit entry point"
require_literal "$runner" 'qemu-system-x86_64' \
  "runner must use qemu-system-x86_64"
require_literal "$runner" '-device virtio-net-pci-transitional,netdev=xnet,mac=52:54:00:12:34:56' \
  "runner must attach a QEMU virtio network device"
require_literal "$runner" '-netdev socket,id=xnet,udp=127.0.0.1:"$host_port",localaddr=127.0.0.1:"$qemu_port"' \
  "runner must use a socket backend so the host can exchange raw Ethernet frames with the VM"
require_literal "$runner" 'filter-dump,id=xnet_dump,netdev=xnet,file=$net_pcap' \
  "runner must capture VM-visible network frames"
require_literal "$runner" 'ffffffffffff02000000000388b7' \
  "runner must verify the deterministic x86 Ethernet probe frame"
require_literal "$runner" 'DPHOST-CHALLENGE-0001' \
  "runner must inject a deterministic host challenge frame"
require_literal "$runner" 'DPX86-RESPONSE-OK-0001' \
  "runner must verify a deterministic guest response frame"
require_literal "$runner" 'seen_response=true' \
  "runner must verify the host received the guest response"
require_literal "$runner" '02000000000452540012345688b9' \
  "runner must verify the response Ethernet frame in QEMU pcap"
require_literal "$runner" 'DPX86:NET-RX' \
  "runner must verify inbound VM communication"
require_literal "$runner" 'DPX86:DRIVER-FAULT-CONTAINED' \
  "runner must verify driver fault containment"

require_literal "$boot" 'mov $(0x0200 | STAGE2_SECTORS), %ax' \
  "boot sector must load the whole stage2 packet in one BIOS read"
require_literal "$stage2" 'mov %dl, %bl' \
  "stage2 CHS conversion must preserve the sector before reusing CX"
require_literal "$stage2" 'mov %bl, %cl' \
  "stage2 CHS conversion must restore the sector into CL before int13"

require_literal "$main" '#![no_std]' "smoke must stay no_std"
require_literal "$main" '#![no_main]' "smoke must own its bare-metal entry"
require_literal "$main" '.section .text._start,"ax"' \
  "smoke must pin the boot entry in .text._start so feature code cannot precede the stage2 entry address"
require_literal "$main" 'FixedLocalExecCounts' \
  "smoke must exercise dataplane no-alloc scheduler counts"
require_literal "$src_dir" 'ProtectedMailboxEndpoint' \
  "x86_64 QEMU mailbox bus must expose protected mailbox endpoints"
require_literal "$src_dir" 'Mailbox<CAP>' \
  "x86_64 QEMU mailbox bus must preserve dataplane-microkernel-core Mailbox shape"
require_literal "$src_dir" 'MessageBody::InlineBytes' \
  "x86_64 QEMU mailbox bus benchmark must use the shared MessageBody payload shape"
require_literal "$runner" 'DPX86:SHARD-BUS' \
  "runner must surface QEMU mailbox bus conformance markers when present"
require_literal "$runner" 'DPX86:SHARD-FAIRNESS' \
  "runner must surface QEMU mailbox bus fairness markers when present"
require_literal "$src_dir" 'FixedNetworkDriver' \
  "smoke must route driver work through the shared fixed network abstraction"
require_literal "$src_dir" 'transmit_frame' \
  "network abstraction must expose generic frame transmission"
require_literal "$src_dir" 'receive_frame' \
  "network abstraction must expose generic frame reception"
require_literal "$src_dir" 'EthernetFrameSpec' \
  "network abstraction must pass shared Ethernet frame intent separately from virtio transport"
require_literal "$src_dir" 'struct NetworkTaskMemory' \
  "smoke must pass network drivers an explicit protected-memory capability"
require_literal "$src_dir" 'struct NetworkDriverTask' \
  "smoke must run networking as a task object"
require_literal "$src_dir" 'struct VirtioLegacyPciNet' \
  "smoke must keep virtio transport-specific code behind a concrete driver"
require_literal "$src_dir" 'virtio-net-pci-missing' \
  "smoke must fail closed when virtio-net is absent"
require_literal "$src_dir" 'VIRTIO_PCI_QUEUE_PFN' \
  "smoke must program virtio queue physical pages"
require_literal "$src_dir" 'VIRTIO_PCI_QUEUE_NOTIFY' \
  "smoke must notify the virtio device after publishing descriptors"
require_literal "$src_dir" 'fn arm_receive(' \
  "smoke must provision an RX virtqueue buffer, not only a TX queue"
require_literal "$src_dir" 'self.rx.submit_writable(region, RX_BUFFER_OFFSET, 2048)' \
  "smoke must publish the RX buffer as a writable virtqueue descriptor"
require_literal "$src_dir" 'validate_host_challenge' \
  "smoke must validate an inbound host challenge frame"
require_literal "$src_dir" 'host_response_frame' \
  "smoke must prepare a guest response frame after RX"
require_literal "$src_dir" 'HOST_CHALLENGE_PAYLOAD' \
  "smoke must keep the host challenge payload explicit"
require_literal "$src_dir" 'VM_RESPONSE_PAYLOAD' \
  "smoke must keep the guest response payload explicit"
require_literal "$main" 'DPX86:NET-RX' \
  "smoke must emit a host-verified inbound network marker"
require_literal "$src_dir" 'tx_probe_frame' \
  "smoke must prepare a deterministic Ethernet probe frame"
require_literal "$src_dir" 'make_tx_frame' \
  "smoke must share frame preparation across probe and response TX"
require_literal "$main" 'with_network_region' \
  "smoke must only expose driver memory through a protected task-region API"
require_literal "$main" 'mmu::allow_region' \
  "protected task access must permit pages before driver execution"
require_literal "$main" 'mmu::deny_region' \
  "protected task access must deny pages after driver execution"
require_literal "$main" 'trigger_driver_fault' \
  "smoke must intentionally fault a denied driver page"
require_literal "$src_dir" 'DRIVER_FAULT_SEEN' \
  "page fault handler must record contained driver crash evidence"
require_literal "$src_dir" 'IdtEntry::new(handler, 0x18, 0x8e00)' \
  "page fault gate must use the active long-mode code selector and interrupt-gate attribute byte"
require_literal "$main" 'DPX86:OK' \
  "smoke must emit a host-verified success marker"

./tools/check_isolated_network_task_boundary_contract.sh x86_64-virtio

echo "x86_64 virtio network MMU smoke contract guard passed."
