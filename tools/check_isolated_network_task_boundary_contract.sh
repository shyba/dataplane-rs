#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

component="${1:-all}"

x86_virtio_src="crates/dataplane-x86_64-virtio-smoke/src"
x86_virtio_main="$x86_virtio_src/main.rs"
microkernel_main="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
microkernel_kernel="crates/dataplane-x86_64-microkernel-smoke/src/kernel.rs"
microkernel_network_task="crates/dataplane-x86_64-microkernel-smoke/src/network_task.rs"
microkernel_network_task_src="crates/dataplane-x86_64-microkernel-smoke/src/network_task"
microkernel_http="crates/dataplane-x86_64-microkernel-smoke/src/http.rs"
microkernel_virtio_net="crates/dataplane-x86_64-microkernel-smoke/src/virtio_net.rs"
raspi3b_src="crates/dataplane-raspi3b-mmu-smoke/src"
raspi3b_main="$raspi3b_src/main.rs"
shared_core="crates/dataplane-microkernel-core/src/lib.rs"

fail() {
  echo "FAIL: $*" >&2
  exit 1
}

case "$component" in
  all|x86_64-virtio|x86_64-microkernel|raspi3b) ;;
  *) fail "unknown isolated network task boundary component: $component" ;;
esac

[[ -d "$x86_virtio_src" ]] || fail "missing x86_64 virtio smoke source directory"
[[ -f "$x86_virtio_main" ]] || fail "missing x86_64 virtio smoke source"
[[ -f "$microkernel_main" ]] || fail "missing x86_64 microkernel smoke source"
[[ -f "$microkernel_kernel" ]] || fail "missing x86_64 microkernel kernel source"
[[ -f "$microkernel_network_task" ]] || fail "missing x86_64 microkernel network task source"
[[ -d "$microkernel_network_task_src" ]] || fail "missing x86_64 microkernel network task module directory"
[[ -f "$microkernel_http" ]] || fail "missing x86_64 microkernel http source"
[[ -f "$microkernel_virtio_net" ]] || fail "missing x86_64 microkernel virtio net source"
[[ -f "$raspi3b_main" ]] || fail "missing Raspi3B MMU smoke source"
[[ -f "$shared_core" ]] || fail "missing shared microkernel core source"

python3 - "$component" "$x86_virtio_src" "$microkernel_main" "$microkernel_kernel" "$microkernel_network_task_src" "$microkernel_http" "$microkernel_virtio_net" "$raspi3b_src" "$shared_core" <<'PY'
import sys
from pathlib import Path


component = sys.argv[1]
paths = {
    "x86_64-virtio": Path(sys.argv[2]),
    "x86_64-microkernel": Path(sys.argv[3]),
    "x86_64-microkernel-kernel": Path(sys.argv[4]),
    "x86_64-microkernel-network-task": Path(sys.argv[5]),
    "x86_64-microkernel-http": Path(sys.argv[6]),
    "x86_64-microkernel-virtio-net": Path(sys.argv[7]),
    "raspi3b": Path(sys.argv[8]),
    "shared-core": Path(sys.argv[9]),
}
def read_source(path: Path) -> str:
    if path.is_dir():
        return "\n".join(child.read_text() for child in sorted(path.rglob("*.rs")))
    return path.read_text()


sources = {name: read_source(path) for name, path in paths.items()}


def fail(message: str) -> None:
    print(f"FAIL: {message}", file=sys.stderr)
    sys.exit(1)


def scan_matching_brace(source: str, start: int) -> int:
    depth = 0
    in_line_comment = False
    in_block_comment = False
    in_string = False
    escape = False
    i = start
    while i < len(source):
        ch = source[i]
        nxt = source[i + 1] if i + 1 < len(source) else ""

        if in_line_comment:
            if ch == "\n":
                in_line_comment = False
            i += 1
            continue
        if in_block_comment:
            if ch == "*" and nxt == "/":
                in_block_comment = False
                i += 2
            else:
                i += 1
            continue
        if in_string:
            if escape:
                escape = False
            elif ch == "\\":
                escape = True
            elif ch == '"':
                in_string = False
            i += 1
            continue
        if ch == "/" and nxt == "/":
            in_line_comment = True
            i += 2
            continue
        if ch == "/" and nxt == "*":
            in_block_comment = True
            i += 2
            continue
        if ch == '"':
            in_string = True
            i += 1
            continue
        if ch == "{":
            depth += 1
        elif ch == "}":
            depth -= 1
            if depth == 0:
                return i + 1
            if depth < 0:
                fail("brace scanner underflowed while extracting Rust item")
        i += 1
    fail("could not find matching closing brace while extracting Rust item")
    return -1


def body(source: str, needle: str) -> str:
    pos = source.find(needle)
    if pos < 0:
        fail(f"missing Rust item boundary: {needle}")
    brace = source.find("{", pos)
    if brace < 0:
        fail(f"missing opening brace for Rust item: {needle}")
    end = scan_matching_brace(source, brace)
    return source[pos:end]


def require_text(source: str, needle: str, note: str) -> None:
    if needle not in source:
        fail(note)


def require_body(source: str, item: str, needle: str, note: str) -> None:
    if needle not in body(source, item):
        fail(note)


def reject_body(source: str, item: str, needle: str, note: str) -> None:
    item_body = body(source, item)
    if needle in item_body:
        fail(note)


def require_not_text(source: str, needle: str, note: str) -> None:
    if needle in source:
        fail(note)


def reject_many(source: str, item: str, needles: list[tuple[str, str]]) -> None:
    for needle, label in needles:
        reject_body(
            source,
            item,
            needle,
            f"{item} must not contain {label}; network drivers stay at frame/transport boundaries",
        )


protocol_tokens = [
    ("NetworkPacketKind", "network protocol packet classifier"),
    ("NetworkReplyKind", "network protocol reply classifier"),
    ("HttpTask", "HTTP task reference"),
    ("HttpResponse", "HTTP response type"),
    ("http_reply", "HTTP reply helper"),
    ("handle_net_frame", "network protocol dispatcher"),
    ("accepts_ipv4", "IPv4 accept predicate"),
    ("write_ipv4_header", "IPv4 header writer"),
    ("tcp_checksum", "TCP checksum helper"),
    ("TcpSegmentSpec", "TCP segment type"),
    ("ETHERTYPE_ARP", "ARP ethertype"),
    ("ETHERTYPE_IPV4", "IPv4 ethertype"),
    ("ARP", "ARP token"),
    ("IPv4", "IPv4 token"),
    ("Ipv4", "IPv4 token"),
    ("HTTP", "HTTP token"),
    ("Http", "HTTP token"),
]


def check_x86_64_virtio() -> None:
    source = sources["x86_64-virtio"]
    require_text(source, "dataplane_microkernel_core::{", "x86_64 virtio smoke must import shared microkernel network contract")
    require_text(source, "FixedNetworkDriver", "x86_64 virtio smoke must consume shared FixedNetworkDriver")
    require_text(source, "FixedNetworkTask", "x86_64 virtio smoke must consume shared FixedNetworkTask")
    require_text(source, "EthernetFrameSpec", "x86_64 virtio smoke must consume shared EthernetFrameSpec")
    require_text(source, "ReceivedFrame", "x86_64 virtio smoke must consume shared ReceivedFrame")
    require_text(source, "NetworkDriverTask::new(", "x86_64 virtio task must construct a network driver task")
    require_text(source, "FixedNetworkTask::new(virtio::VirtioLegacyPciNet::new())", "x86_64 virtio task must be constructed with the shared fixed task")
    require_body(source, "impl<D> NetworkDriverTask<D>", "self.fixed.transmit_frame", "NetworkDriverTask must delegate TX through the shared frame boundary")
    require_body(source, "impl<D> NetworkDriverTask<D>", "self.fixed.receive_frame", "NetworkDriverTask must delegate RX through the shared frame boundary")
    require_body(source, "impl<'a> FixedNetworkDriver<NetworkTaskMemory<'a>, FailReason> for VirtioLegacyPciNet", "fn transmit_frame(", "Virtio driver must implement shared frame-level TX")
    require_body(source, "impl<'a> FixedNetworkDriver<NetworkTaskMemory<'a>, FailReason> for VirtioLegacyPciNet", "fn receive_frame(", "Virtio driver must implement shared frame-level RX")
    require_body(source, "impl<'a> FixedNetworkDriver<NetworkTaskMemory<'a>, FailReason> for VirtioLegacyPciNet", "make_tx_frame", "Virtio driver TX must consume frame intent, not protocol intent")
    require_body(source, "impl<'a> FixedNetworkDriver<NetworkTaskMemory<'a>, FailReason> for VirtioLegacyPciNet", "fn arm_receive(", "Virtio driver RX must provision raw frame buffers")
    require_body(source, "impl<'a> FixedNetworkDriver<NetworkTaskMemory<'a>, FailReason> for VirtioLegacyPciNet", "submit_writable", "Virtio driver RX must publish a writable raw frame buffer")
    reject_many(source, "impl<'a> FixedNetworkDriver<NetworkTaskMemory<'a>, FailReason> for VirtioLegacyPciNet", protocol_tokens)
    reject_many(source, "impl<D> NetworkDriverTask<D>", protocol_tokens)


def check_x86_64_microkernel() -> None:
    main_source = sources["x86_64-microkernel"]
    kernel_source = sources["x86_64-microkernel-kernel"]
    network_task_source = sources["x86_64-microkernel-network-task"]
    http_source = sources["x86_64-microkernel-http"]
    net_source = sources["x86_64-microkernel-virtio-net"]

    require_not_text(main_source, "struct NetDriverTask", "microkernel must keep the net driver task out of main.rs")
    require_not_text(main_source, "impl NetDriverTask", "microkernel must keep the net driver task implementation out of main.rs")
    require_text(network_task_source, "struct TcpIpTask", "microkernel must keep protocol parsing outside the net driver")
    require_body(net_source, "impl NetDriverTask", "self.driver.transmit_frame", "NetDriverTask must delegate TX through frame boundary")
    require_body(net_source, "impl NetDriverTask", "self.driver.poll_receive_frame", "NetDriverTask must poll raw frames through driver boundary")
    require_body(net_source, "impl NetDriverTask", "self.driver.arm_receive", "NetDriverTask must re-arm raw frame receive buffers")
    require_body(net_source, "impl NetDriverTask", "protected_net_region", "NetDriverTask must use protected net memory access")
    require_body(net_source, "impl VirtioLegacyPciNet", "fn transmit_frame(", "microkernel virtio driver must expose frame-level TX")
    require_body(net_source, "impl VirtioLegacyPciNet", "fn poll_receive_frame(", "microkernel virtio driver must expose frame-level RX polling")
    require_body(net_source, "impl VirtioLegacyPciNet", "fn arm_receive(", "microkernel virtio driver must expose raw RX buffer arming")
    require_body(net_source, "impl VirtioLegacyPciNet", "prepare_net_tx_frame", "microkernel virtio TX must consume EthernetFrameSpec")
    require_body(net_source, "impl VirtioLegacyPciNet", "prepare_net_rx_buffer", "microkernel virtio RX must provision raw frame buffers")
    reject_many(net_source, "impl NetDriverTask", protocol_tokens)
    reject_many(net_source, "impl VirtioLegacyPciNet", protocol_tokens)
    require_text(network_task_source, "handle_net_frame", "TcpIpTask must own network protocol dispatch")
    require_text(network_task_source, "NetworkPacketKind", "TcpIpTask must own network protocol classification")
    require_body(http_source, "impl HttpTask", "build_response", "HttpTask must own HTTP response handling")


def check_raspi3b() -> None:
    source = sources["raspi3b"]
    require_text(source, "dataplane_microkernel_core::FixedNetworkTask", "Raspi3B root must consume shared FixedNetworkTask")
    require_text(source, "use dataplane_microkernel_core::{", "Raspi3B Ethernet task must import shared network contract")
    require_text(source, "FixedNetworkDriver", "Raspi3B Ethernet task must consume shared FixedNetworkDriver")
    require_text(source, "EthernetFrameSpec", "Raspi3B Ethernet task must consume shared EthernetFrameSpec")
    require_text(source, "ReceivedFrame", "Raspi3B Ethernet task must consume shared ReceivedFrame")
    require_text(source, "FixedNetworkTask::new(UsbEthernetTask::new())", "Raspi3B USB-net task must be wrapped in the shared fixed task")
    require_body(source, 'pub(crate) fn run()', "ethernet.init(bytes)", "Raspi3B scheduler must drive USB-net progress through the shared FixedNetworkTask init method")
    require_body(source, 'pub(crate) fn run()', "ethernet.transmit_frame(bytes, crate::ethernet::rndis_probe_frame())", "Raspi3B scheduler must request TX through the shared FixedNetworkTask TX method")
    require_body(source, 'pub(crate) fn run()', "ethernet.receive_frame(bytes)", "Raspi3B scheduler must observe RX/completion through the shared FixedNetworkTask RX method")
    require_not_text(source, "driver_mut().poll", "Raspi3B must not bypass FixedNetworkTask by polling the wrapped driver directly")
    require_text(source, "struct UsbEthernetTask", "Raspi3B smoke must keep a distinct USB Ethernet task")
    require_body(source, "impl FixedNetworkDriver<[u8; SHARD_REGION_SIZE], ()> for UsbEthernetTask", "fn transmit_frame(", "UsbEthernetTask must implement shared frame-level TX")
    require_body(source, "impl FixedNetworkDriver<[u8; SHARD_REGION_SIZE], ()> for UsbEthernetTask", "fn receive_frame(", "UsbEthernetTask must expose shared frame-level RX semantics")
    require_body(source, "impl FixedNetworkDriver<[u8; SHARD_REGION_SIZE], ()> for UsbEthernetTask", "prepare_rndis_probe_frame", "USB-net TX must map shared Ethernet frame intent into local RNDIS transport")
    require_body(source, "impl FixedNetworkDriver<[u8; SHARD_REGION_SIZE], ()> for UsbEthernetTask", "start_bulk_out_transfer", "UsbEthernetTask must transmit through USB bulk transport boundary")
    require_body(source, "impl FixedNetworkDriver<[u8; SHARD_REGION_SIZE], ()> for UsbEthernetTask", "self.rx_frame_len", "USB-net RX must report the validated Ethernet frame length")
    require_body(source, "impl UsbEthernetTask", "StartBulkIn", "UsbEthernetTask must arm a bounded USB bulk-IN receive after TX completion")
    require_body(source, "impl UsbEthernetTask", "WaitBulkIn", "UsbEthernetTask must wait for USB bulk-IN completion before reporting RX")
    require_body(source, "impl UsbEthernetTask", "capture_rx_frame", "UsbEthernetTask must capture RX evidence from the bulk-IN buffer")
    require_body(source, "impl UsbEthernetTask", "self.rx_frame_len = data_len", "UsbEthernetTask must store the validated RX Ethernet frame length")
    require_body(source, "impl UsbEthernetTask", "BULK_RX_FRAME_OFFSET", "UsbEthernetTask must keep RX buffer evidence distinct from TX buffer evidence")
    require_body(source, "pub(super) fn poll_start_bulk_in", "start_bulk_in_transfer", "UsbEthernetTask must receive through the USB bulk transport boundary")
    require_body(source, "fn receive_frame(", "self.rx_frame_len", "receive_frame must return the validated RX Ethernet frame length")
    reject_body(source, "fn receive_frame(", "self.rx_transport_len as usize", "receive_frame must not report the bounded USB transport length as descriptor length")
    reject_body(source, "fn receive_frame(", "BULK_FRAME_OFFSET", "receive_frame must not reuse the TX RNDIS buffer as RX evidence")
    reject_body(source, "fn receive_frame(", "RNDIS_PACKET_LEN as u32", "receive_frame must not report TX packet length as RX evidence")
    reject_body(source, "fn receive_frame(", "capture_frame", "receive_frame must not treat TX completion capture as RX")
    reject_body(source, "impl UsbEthernetTask", "self.transmit_frame", "UsbEthernetTask must not invoke shared TX through a target-local self call")
    require_body(source, "impl UsbEthernetTask", "capture_rndis_response", "UsbEthernetTask must keep RNDIS control response handling bounded")
    require_text(source, "const ETHERNET_FRAME_LEN", "Raspi3B Ethernet frame length must remain explicit")
    require_text(source, "const RNDIS_PACKET_LEN", "Raspi3B RNDIS packet length must remain explicit")
    require_text(source, "const BULK_RX_FRAME_OFFSET", "Raspi3B RX buffer offset must remain explicit and target-local")
    require_text(source, "fn start_bulk_in_transfer", "Raspi3B USB bulk-IN receive helper must remain target-local")
    reject_many(source, "impl UsbEthernetTask", protocol_tokens)


def check_shared_core() -> None:
    source = sources["shared-core"]
    require_text(source, "pub struct EthernetFrameSpec<'a>", "shared core must define EthernetFrameSpec")
    require_text(source, "pub struct ReceivedFrame", "shared core must define ReceivedFrame")
    require_text(source, "pub trait FixedNetworkDriver<Memory, Error>", "shared core must define the fixed network driver trait")
    require_body(source, "pub trait FixedNetworkDriver<Memory, Error>", "fn transmit_frame(", "shared driver trait must expose frame-level TX")
    require_body(source, "pub trait FixedNetworkDriver<Memory, Error>", "fn receive_frame(", "shared driver trait must expose frame-level RX")
    require_text(source, "pub struct FixedNetworkTask<D>", "shared core must define the fixed network task wrapper")
    require_body(source, "impl<D> FixedNetworkTask<D>", "self.driver.transmit_frame", "shared fixed task must delegate frame-level TX")
    require_body(source, "impl<D> FixedNetworkTask<D>", "self.driver.receive_frame", "shared fixed task must delegate frame-level RX")


checks = {
    "x86_64-virtio": check_x86_64_virtio,
    "x86_64-microkernel": check_x86_64_microkernel,
    "raspi3b": check_raspi3b,
}

if component == "all":
    check_shared_core()
    for check in checks.values():
        check()
else:
    if component in ("x86_64-virtio", "raspi3b"):
        check_shared_core()
    checks[component]()
PY

echo "isolated network task boundary contract passed: $component"
