#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

target="x86_64-unknown-none"
image="target/$target/release/dataplane-x86_64-virtio-smoke.img"
serial_log="$(mktemp "${TMPDIR:-/tmp}/dataplane-x86-virtio-udp-serial.XXXXXX")"
qemu_log="$(mktemp "${TMPDIR:-/tmp}/dataplane-x86-virtio-udp-qemu.XXXXXX")"
net_pcap="$(mktemp "${TMPDIR:-/tmp}/dataplane-x86-virtio-udp-net.XXXXXX.pcap")"
host_exchange_log="$(mktemp "${TMPDIR:-/tmp}/dataplane-x86-virtio-udp-host.XXXXXX")"
qemu_pid=""
host_pid=""

cleanup() {
  if [[ -n "$qemu_pid" ]]; then
    kill "$qemu_pid" >/dev/null 2>&1 || true
    wait "$qemu_pid" >/dev/null 2>&1 || true
  fi
  if [[ -n "$host_pid" ]]; then
    kill "$host_pid" >/dev/null 2>&1 || true
    wait "$host_pid" >/dev/null 2>&1 || true
  fi
  rm -f "$serial_log" "$qemu_log" "$net_pcap" "$host_exchange_log"
}
trap cleanup EXIT

echo "=== x86_64 Virtio UDP Bench ==="

if [[ ! -f "$image" ]]; then
  echo "FAIL: UDP bench image not found: $image"
  echo "Run: make x86_64-virtio-udp-bench-build"
  exit 1
fi

host_port=$((41000 + RANDOM % 1000))
qemu_port=$((host_port + 1000))
packet_count=128

DP_HOST_PORT="$host_port" \
DP_QEMU_PORT="$qemu_port" \
DP_EXCHANGE_LOG="$host_exchange_log" \
DP_PACKET_COUNT="$packet_count" \
python3 - <<'PY' &
import os
import socket
import time

host_port = int(os.environ["DP_HOST_PORT"])
qemu_port = int(os.environ["DP_QEMU_PORT"])
log_path = os.environ["DP_EXCHANGE_LOG"]
packet_count = int(os.environ["DP_PACKET_COUNT"])

vm_mac = bytes.fromhex("525400123456")
host_mac = bytes.fromhex("020000000004")
host_ip = bytes([10, 0, 0, 1])
vm_ip = bytes([10, 0, 0, 2])
request_magic = b"DPUDPQ00"
response_magic = b"DPUDPR00"
host_port_udp = 40000
vm_port_udp = 40001

def be16(value):
    return value.to_bytes(2, "big")

def be32(value):
    return value.to_bytes(4, "big")

def ipv4_checksum(header):
    total = 0
    for index in range(0, 20, 2):
        total += int.from_bytes(header[index:index + 2], "big")
    while total >> 16:
        total = (total & 0xffff) + (total >> 16)
    return (~total) & 0xffff

def build_ipv4_udp(src_ip, dst_ip, src_port, dst_port, magic, seq, check):
    payload = magic + be32(seq) + be32(check)
    total_len = 20 + 8 + len(payload)
    udp_len = 8 + len(payload)
    ip = bytearray(20)
    ip[0] = 0x45
    ip[2:4] = be16(total_len)
    ip[4:6] = be16(seq & 0xffff)
    ip[6:8] = be16(0x4000)
    ip[8] = 64
    ip[9] = 17
    ip[12:16] = src_ip
    ip[16:20] = dst_ip
    ip[10:12] = be16(ipv4_checksum(ip))
    udp = be16(src_port) + be16(dst_port) + be16(udp_len) + b"\x00\x00"
    return bytes(ip) + udp + payload

def request_frame(seq):
    packet = build_ipv4_udp(
        host_ip,
        vm_ip,
        host_port_udp,
        vm_port_udp,
        request_magic,
        seq,
        seq ^ 0xA5A5A5A5,
    )
    return (vm_mac + host_mac + b"\x08\x00" + packet).ljust(60, b"\x00")

def parse_response(data):
    if data.startswith(b"\x00\x00"):
        data = data[2:]
    if len(data) < 14 + 20 + 8 + 16:
        return None
    if data[0:6] != host_mac or data[6:12] != vm_mac or data[12:14] != b"\x08\x00":
        return None
    ip = data[14:34]
    if ip[0] != 0x45 or ip[9] != 17 or ip[12:16] != vm_ip or ip[16:20] != host_ip:
        return None
    if ipv4_checksum(ip) != 0:
        return None
    udp = data[34:42]
    if udp[0:2] != be16(vm_port_udp) or udp[2:4] != be16(host_port_udp):
        return None
    payload = data[42:58]
    if payload[0:8] != response_magic:
        return None
    seq = int.from_bytes(payload[8:12], "big")
    check = int.from_bytes(payload[12:16], "big")
    if check != (seq ^ 0x5A5A5A5A):
        return None
    return seq

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.bind(("127.0.0.1", host_port))
sock.setblocking(False)

responses = 0
received_hex = []
start = time.monotonic()
deadline = start + 20.0

for seq in range(packet_count):
    saw = False
    next_send = 0.0
    while time.monotonic() < deadline:
        now = time.monotonic()
        if now >= next_send:
            frame = request_frame(seq)
            sock.sendto(frame, ("127.0.0.1", qemu_port))
            sock.sendto(b"\x00\x00" + frame, ("127.0.0.1", qemu_port))
            next_send = now + 0.005
        try:
            data, _ = sock.recvfrom(2048)
            if len(received_hex) < 8:
                received_hex.append(data.hex())
            response_seq = parse_response(data)
            if response_seq == seq:
                responses += 1
                saw = True
                break
        except BlockingIOError:
            pass
        time.sleep(0.001)
    if not saw:
        break

elapsed_ms = max(1, int((time.monotonic() - start) * 1000))
pps = int((responses * 1000) / elapsed_ms)

with open(log_path, "w", encoding="ascii") as log:
    log.write(f"udp_packets={packet_count}\n")
    log.write(f"udp_responses={responses}\n")
    log.write(f"elapsed_ms={elapsed_ms}\n")
    log.write(f"pps={pps}\n")
    for item in received_hex:
        log.write("rx=" + item + "\n")

raise SystemExit(0 if responses == packet_count else 2)
PY
host_pid="$!"

timeout 25s qemu-system-x86_64 \
  -M pc \
  -m 128M \
  -nographic \
  -monitor none \
  -serial file:"$serial_log" \
  -drive file="$image",format=raw,if=floppy \
  -boot a \
  -netdev socket,id=xnet,udp=127.0.0.1:"$host_port",localaddr=127.0.0.1:"$qemu_port" \
  -object "filter-dump,id=xnet_dump,netdev=xnet,file=$net_pcap" \
  -device virtio-net-pci-transitional,netdev=xnet,mac=52:54:00:12:34:56 \
  -no-reboot \
  -no-shutdown >"$qemu_log" 2>&1 &
qemu_pid="$!"

deadline=$((SECONDS + 20))
while (( SECONDS < deadline )); do
  if grep -q "DPX86:OK" "$serial_log" 2>/dev/null; then
    break
  fi
  if ! kill -0 "$qemu_pid" >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done

if ! grep -q "DPX86:OK" "$serial_log" 2>/dev/null; then
  echo "FAIL: x86_64 virtio UDP bench did not report success"
  echo "--- serial ---"
  sed -n '1,180p' "$serial_log" 2>/dev/null || true
  echo "--- qemu ---"
  sed -n '1,180p' "$qemu_log" 2>/dev/null || true
  echo "--- host exchange ---"
  sed -n '1,180p' "$host_exchange_log" 2>/dev/null || true
  exit 1
fi

kill "$qemu_pid" >/dev/null 2>&1 || true
wait "$qemu_pid" >/dev/null 2>&1 || true
qemu_pid=""

if ! wait "$host_pid"; then
  host_pid=""
  echo "FAIL: host UDP bench did not receive all guest responses"
  echo "--- host exchange ---"
  sed -n '1,180p' "$host_exchange_log" 2>/dev/null || true
  echo "--- serial ---"
  sed -n '1,180p' "$serial_log" 2>/dev/null || true
  exit 1
fi
host_pid=""

if [[ ! -s "$net_pcap" ]]; then
  echo "FAIL: x86_64 virtio UDP pcap was not written"
  sed -n '1,180p' "$serial_log" 2>/dev/null || true
  exit 1
fi

net_pcap_hex="$(od -An -tx1 -v "$net_pcap" | tr -d ' \n')"
if [[ "$net_pcap_hex" != *"020000000004525400123456080045"* ]]; then
  echo "FAIL: x86_64 virtio UDP pcap did not capture a guest IPv4/UDP response"
  echo "Expected frame prefix: 02 00 00 00 00 04 52 54 00 12 34 56 08 00 45"
  echo "--- serial ---"
  sed -n '1,180p' "$serial_log" 2>/dev/null || true
  echo "--- host exchange ---"
  sed -n '1,180p' "$host_exchange_log" 2>/dev/null || true
  exit 1
fi

if ! grep -q "udp_responses=$packet_count" "$host_exchange_log"; then
  echo "FAIL: UDP bench did not complete every packet"
  sed -n '1,180p' "$host_exchange_log" 2>/dev/null || true
  exit 1
fi

if ! grep -q "DPX86:UDP-BENCH" "$serial_log"; then
  echo "FAIL: x86_64 virtio UDP bench marker missing"
  sed -n '1,180p' "$serial_log" 2>/dev/null || true
  exit 1
fi

if ! grep -q "DPX86:NET-RX" "$serial_log"; then
  echo "FAIL: x86_64 virtio UDP bench did not report inbound VM communication"
  sed -n '1,180p' "$serial_log" 2>/dev/null || true
  exit 1
fi

if ! grep -q "DPX86:DRIVER-FAULT-CONTAINED" "$serial_log"; then
  echo "FAIL: x86_64 virtio UDP bench driver fault containment marker missing"
  sed -n '1,180p' "$serial_log" 2>/dev/null || true
  exit 1
fi

sed -n '1,12p' "$host_exchange_log"
echo "x86_64 virtio UDP bench passed."
