#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

src_dir="crates/dataplane-raspi3b-mmu-smoke/src"
main="$src_dir/main.rs"
runner="tools/raspi3b_mmu_smoke_run.sh"
manifest="crates/dataplane-raspi3b-mmu-smoke/Cargo.toml"
elf="target/aarch64-unknown-none/release/dataplane-raspi3b-mmu-smoke"

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

require_regex() {
  local file="$1"
  local pattern="$2"
  local note="$3"
  rg -q -- "$pattern" "$file" || fail "$note"
}

echo "=== Raspi3B MMU Smoke Contract Guard ==="

[[ -f "$main" ]] || fail "missing Raspi3B MMU smoke main"
[[ -f "$runner" ]] || fail "missing Raspi3B MMU smoke runner"
[[ -f "$manifest" ]] || fail "missing Raspi3B MMU smoke manifest"

require_literal Cargo.toml '"crates/dataplane-raspi3b-mmu-smoke",' \
  "workspace must include dataplane-raspi3b-mmu-smoke"
require_literal Makefile 'aarch64-unknown-none' \
  "Makefile must build the smoke for aarch64-unknown-none"
require_literal Makefile 'check_raspi3b_mmu_smoke_contract.sh' \
  "raspi3b-mmu-smoke target must run this contract guard"

require_literal "$runner" 'qemu-system-aarch64' \
  "runner must use qemu-system-aarch64"
require_literal "$runner" '-M raspi3b' \
  "runner must use QEMU raspi3b machine"
require_literal "$runner" '-semihosting-config enable=on,target=native' \
  "runner must use native semihosting pass/fail status"
require_literal "$runner" '-serial "unix:$uart_sock,server=on,wait=on"' \
  "runner must expose a bidirectional Raspi3B VM UART socket before guest execution"
require_literal "$runner" "printf 'DPHOST?\\n'" \
  "runner must send the host UART challenge"
require_literal "$runner" 'read -r -N 8 -t 5 uart_response' \
  "runner must read the fixed VM UART response without closing the socket early"
require_literal "$runner" 'grep -q "DPVM:OK" "$uart_log"' \
  "runner must verify the VM UART challenge/response marker"
require_literal "$runner" '-netdev user,id=raspi_net' \
  "runner must expose a VM-side user network backend"
require_literal "$runner" 'filter-dump,id=raspi_dump,netdev=raspi_net,file=$net_pcap' \
  "runner must capture QEMU VM net backend packets to pcap"
require_literal "$runner" '-device usb-net,netdev=raspi_net' \
  "runner must attach QEMU usb-net to the Raspi3B USB host"
require_literal "$runner" 'od -An -tx1 -v "$net_pcap"' \
  "runner must inspect captured VM net backend packet bytes"
require_literal "$runner" 'ffffffffffff02000000000188b5' \
  "runner must verify the deterministic Ethernet probe frame reaches the VM net backend"

require_literal "$main" '#![no_std]' "smoke must be no_std"
require_literal "$main" '#![no_main]' "smoke must own bare-metal entry"
require_literal "$src_dir" 'FixedLocalExecCounts' \
  "smoke must exercise dataplane scheduler counts, not a standalone loop only"
require_literal "$src_dir" 'ETHERNET_REGION' \
  "smoke must dedicate a protected region to communication tasks"
require_literal "$src_dir" 'UsbEthernetTask' \
  "smoke must include the scheduler-owned USB Ethernet task"
require_literal "$src_dir" 'ethernet.init(bytes)' \
  "Ethernet task must run protected progress through the shared fixed task"
require_literal "$src_dir" 'ethernet.transmit_frame(bytes, crate::ethernet::rndis_probe_frame())' \
  "Ethernet task must request TX through the shared fixed task"
require_literal "$src_dir" 'ethernet.receive_frame(bytes)' \
  "Ethernet task must observe RX/completion through the shared fixed task"
require_literal "$src_dir" 'VmUartTask' \
  "smoke must include a scheduler-owned VM UART communication task"
require_literal "$src_dir" 'vm_uart.poll(bytes)' \
  "VM UART task must run through protected region access"
require_literal "$src_dir" 'UART0_BASE: usize = 0x3f20_1000' \
  "VM UART task must use the Raspi3B PL011 MMIO base"
require_literal "$src_dir" 'DPVM:OK' \
  "VM UART task must emit the host-verified communication marker"
require_literal "$src_dir" 'DPHOST?' \
  "VM UART task must wait for a host challenge before replying"
require_literal "$src_dir" 'UART_RECORD_OFFSET: usize = 384' \
  "VM UART state record must not overlap USB setup/descriptor/bulk buffers"
require_literal "$src_dir" 'vm_uart.communicated()' \
  "smoke must fail unless the VM UART task finishes bidirectional communication"
require_literal "$src_dir" 'if !ethernet_tx_ready' \
  "smoke must fail unless the USB-net descriptor/configuration and TX frontier completes"
require_literal "$src_dir" 'USB_BASE: usize = 0x3f98_0000' \
  "Ethernet task must probe the Raspi3B DWC2 USB host MMIO base"
require_literal "$src_dir" 'GSNPSID' \
  "Ethernet task must read the DWC2 core id"
require_literal "$src_dir" 'HPRT' \
  "Ethernet task must inspect the DWC2 host port"
require_literal "$src_dir" 'HPRT_CONN' \
  "Ethernet task must require a connected USB network device"
require_literal "$src_dir" 'GET_DEVICE_DESCRIPTOR' \
  "Ethernet task must issue a USB GET_DESCRIPTOR control request"
require_literal "$src_dir" 'USB_CLASS_HUB' \
  "Ethernet task must identify the QEMU root USB hub before enumerating usb-net"
require_literal "$src_dir" 'HUB_ADDRESS: u32 = 1' \
  "Ethernet task must assign an explicit address to the QEMU USB hub"
require_literal "$src_dir" 'USB_NET_ADDRESS: u32 = 2' \
  "Ethernet task must assign an explicit downstream address to QEMU usb-net"
require_literal "$src_dir" 'HUB_PORT_POWER' \
  "Ethernet task must power the downstream QEMU hub port"
require_literal "$src_dir" 'HUB_PORT_RESET' \
  "Ethernet task must reset the downstream QEMU hub port"
require_literal "$src_dir" 'RNDIS_CONFIGURATION_VALUE: u16 = 2' \
  "Ethernet task must select QEMU usb-net RNDIS configuration before probing bulk packet movement"
require_literal "$src_dir" 'rndis_configured' \
  "Ethernet task must record accepted RNDIS configuration before the bulk attempt"
require_literal "$src_dir" 'USB_CDC_SEND_ENCAPSULATED_COMMAND' \
  "Ethernet task must send RNDIS control commands through CDC encapsulated commands"
require_literal "$src_dir" 'USB_CDC_GET_ENCAPSULATED_RESPONSE' \
  "Ethernet task must read RNDIS control completions through CDC encapsulated responses"
require_literal "$src_dir" 'RNDIS_INITIALIZE_MSG' \
  "Ethernet task must send RNDIS initialize before data movement"
require_literal "$src_dir" 'RNDIS_INITIALIZE_CMPLT' \
  "Ethernet task must verify RNDIS initialize completion"
require_literal "$src_dir" 'RNDIS_SET_MSG' \
  "Ethernet task must send an RNDIS packet-filter set command"
require_literal "$src_dir" 'RNDIS_SET_CMPLT' \
  "Ethernet task must verify RNDIS packet-filter set completion"
require_literal "$src_dir" 'OID_GEN_CURRENT_PACKET_FILTER' \
  "Ethernet task must enable the RNDIS packet filter before bulk data"
require_literal "$src_dir" 'prepare_rndis_probe_frame' \
  "Ethernet task must prepare an RNDIS-wrapped Ethernet probe frame"
require_literal "$src_dir" 'RNDIS_HEADER_LEN' \
  "Ethernet task must keep the RNDIS packet header length explicit"
require_literal "$src_dir" 'ETHERNET_FRAME_LEN: usize = 14' \
  "Ethernet task must send the fixed Ethernet probe payload length"
require_literal "$src_dir" 'bulk_attempted' \
  "Ethernet task must record that the QEMU usb-net bulk path was attempted"
require_literal "$src_dir" 'frame_sent' \
  "Ethernet task must record completed RNDIS bulk packet movement"
require_literal "$src_dir" '&& self.frame_sent' \
  "USB readiness must require completed RNDIS bulk packet movement"
require_literal "$src_dir" 'HCCHAR0' \
  "Ethernet task must enable a DWC2 host channel for the control transfer"
require_literal "$src_dir" 'HCTSIZ0' \
  "Ethernet task must program DWC2 transfer size/PID for descriptor exchange"
require_literal "$src_dir" 'HCDMA0' \
  "Ethernet task must bind descriptor transfer DMA to its protected region"
require_literal "$src_dir" 'descriptor_valid' \
  "Ethernet task must record descriptor receipt, not only port presence"
require_literal "$src_dir" 'communicated()' \
  "smoke must expose communication completion checks"
require_literal "$src_dir" 'struct ProtectedShard' \
  "smoke must expose the safe protected shard wrapper"
require_literal "$src_dir" 'fn with_access' \
  "protected shard access must stay centralized"
require_literal "$src_dir" 'allow_shard(self.id)' \
  "protected shard access must allow the shard before mutable access"
require_literal "$src_dir" 'deny_shard(self.id)' \
  "protected shard access must deny the shard after mutable access"
require_literal "$src_dir" 'trigger_forbidden_access' \
  "smoke must deliberately prove the denied shard faults"
require_literal "$src_dir" 'FAULT_SEEN' \
  "fault handler must record successful MMU fault observation"
require_literal "$src_dir" 'mrs {}, esr_el2' \
  "fault handler must inspect EL2 syndrome"
require_literal "$src_dir" 'mrs {}, elr_el2' \
  "fault handler must inspect EL2 return address"
require_regex "$src_dir" 'ec == 0x24 \|\| ec == 0x25' \
  "fault handler must accept instruction/data abort ECs only"
require_literal "$src_dir" 'msr elr_el2' \
  "fault handler must advance ELR after the expected abort"
require_literal "$src_dir" 'let tcr = 25u64' \
  "EL2 translation must keep T0SZ=25 for the three-table low-address map"
require_literal "$src_dir" 'hlt #0xf000' \
  "smoke must terminate through semihosting"
require_literal "$src_dir" 'mov x0, #0x20' \
  "success path must use SYS_EXIT_EXTENDED for a zero process status"

if [[ -f "$elf" ]]; then
  vectors_hex="$(readelf -sW "$elf" | awk '$8 == "vectors" {print "0x" $2; exit}')"
  [[ -n "$vectors_hex" ]] || fail "built ELF must expose vectors symbol"
  if (( vectors_hex % 2048 != 0 )); then
    fail "vectors must be 2 KiB aligned for AArch64 exception dispatch, got $vectors_hex"
  fi
fi

./tools/check_isolated_network_task_boundary_contract.sh raspi3b

echo "Raspi3B MMU smoke contract guard passed."
