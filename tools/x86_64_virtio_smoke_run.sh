#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

target="x86_64-unknown-none"
image="target/$target/release/dataplane-x86_64-virtio-smoke.img"
serial_log="$(mktemp "${TMPDIR:-/tmp}/dataplane-x86-virtio-serial.XXXXXX")"
qemu_log="$(mktemp "${TMPDIR:-/tmp}/dataplane-x86-virtio-qemu.XXXXXX")"
net_pcap="$(mktemp "${TMPDIR:-/tmp}/dataplane-x86-virtio-net.XXXXXX.pcap")"
host_exchange_log="$(mktemp "${TMPDIR:-/tmp}/dataplane-x86-virtio-host.XXXXXX")"
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

echo "=== x86_64 Virtio Network MMU Smoke ==="

if [[ ! -f "$image" ]]; then
  echo "FAIL: smoke image not found: $image"
  echo "Run: make x86_64-virtio-smoke-build"
  exit 1
fi

host_port=$((39000 + RANDOM % 1000))
qemu_port=$((host_port + 1000))

DP_HOST_PORT="$host_port" \
DP_QEMU_PORT="$qemu_port" \
DP_EXCHANGE_LOG="$host_exchange_log" \
python3 - <<'PY' &
import os
import socket
import time

host_port = int(os.environ["DP_HOST_PORT"])
qemu_port = int(os.environ["DP_QEMU_PORT"])
log_path = os.environ["DP_EXCHANGE_LOG"]

vm_mac = bytes.fromhex("525400123456")
host_mac = bytes.fromhex("020000000004")
challenge_payload = b"DPHOST-CHALLENGE-0001"
response_payload = b"DPX86-RESPONSE-OK-0001"
challenge = (vm_mac + host_mac + bytes.fromhex("88b8") + challenge_payload).ljust(60, b"\x00")
response = host_mac + vm_mac + bytes.fromhex("88b9") + response_payload

sock = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
sock.bind(("127.0.0.1", host_port))
sock.setblocking(False)

deadline = time.monotonic() + 15.0
next_send = 0.0
received = []
seen_response = False

def is_response(data):
    return data.startswith(response) or data.startswith(b"\x00\x00" + response)

while time.monotonic() < deadline:
    now = time.monotonic()
    if now >= next_send:
        sock.sendto(challenge, ("127.0.0.1", qemu_port))
        sock.sendto(b"\x00\x00" + challenge, ("127.0.0.1", qemu_port))
        next_send = now + 0.05
    try:
        data, _ = sock.recvfrom(2048)
        received.append(data.hex())
        if is_response(data):
            seen_response = True
            break
    except BlockingIOError:
        pass
    time.sleep(0.01)

with open(log_path, "w", encoding="ascii") as log:
    log.write("challenge=" + challenge.hex() + "\n")
    for item in received:
        log.write("rx=" + item + "\n")
    log.write("seen_response=" + ("true" if seen_response else "false") + "\n")

raise SystemExit(0 if seen_response else 2)
PY
host_pid="$!"

timeout 20s qemu-system-x86_64 \
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

deadline=$((SECONDS + 15))
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
  echo "FAIL: x86_64 virtio smoke did not report success"
  echo "--- serial ---"
  sed -n '1,160p' "$serial_log" 2>/dev/null || true
  echo "--- qemu ---"
  sed -n '1,160p' "$qemu_log" 2>/dev/null || true
  exit 1
fi

kill "$qemu_pid" >/dev/null 2>&1 || true
wait "$qemu_pid" >/dev/null 2>&1 || true
qemu_pid=""

if ! wait "$host_pid"; then
  host_pid=""
  echo "FAIL: host did not receive the x86_64 virtio response frame"
  echo "--- host exchange ---"
  sed -n '1,160p' "$host_exchange_log" 2>/dev/null || true
  echo "--- serial ---"
  sed -n '1,160p' "$serial_log" 2>/dev/null || true
  exit 1
fi
host_pid=""

if [[ ! -s "$net_pcap" ]]; then
  echo "FAIL: x86_64 virtio net backend pcap was not written"
  sed -n '1,160p' "$serial_log" 2>/dev/null || true
  exit 1
fi

net_pcap_hex="$(od -An -tx1 -v "$net_pcap" | tr -d ' \n')"
if [[ "$net_pcap_hex" != *"ffffffffffff02000000000388b7"* ]]; then
  echo "FAIL: x86_64 virtio net backend did not capture the deterministic Ethernet probe frame"
  echo "Expected frame prefix: ff ff ff ff ff ff 02 00 00 00 00 03 88 b7"
  echo "--- serial ---"
  sed -n '1,160p' "$serial_log" 2>/dev/null || true
  exit 1
fi

if [[ "$net_pcap_hex" != *"02000000000452540012345688b9"* ]]; then
  echo "FAIL: x86_64 virtio net backend did not capture the response Ethernet frame"
  echo "Expected frame prefix: 02 00 00 00 00 04 52 54 00 12 34 56 88 b9"
  echo "--- serial ---"
  sed -n '1,160p' "$serial_log" 2>/dev/null || true
  echo "--- host exchange ---"
  sed -n '1,160p' "$host_exchange_log" 2>/dev/null || true
  exit 1
fi

if ! grep -q "seen_response=true" "$host_exchange_log"; then
  echo "FAIL: host exchange did not prove request/response communication"
  sed -n '1,160p' "$host_exchange_log" 2>/dev/null || true
  exit 1
fi

if ! grep -q "DPX86:NET-RX" "$serial_log"; then
  echo "FAIL: x86_64 virtio smoke did not report inbound VM communication"
  sed -n '1,160p' "$serial_log" 2>/dev/null || true
  exit 1
fi

if ! grep -q "DPX86:DRIVER-FAULT-CONTAINED" "$serial_log"; then
  echo "FAIL: x86_64 driver fault containment marker missing"
  sed -n '1,160p' "$serial_log" 2>/dev/null || true
    exit 1
fi

if grep -q "DPX86:SHARD-BENCH" "$serial_log"; then
  grep "DPX86:SHARD-BENCH" "$serial_log"
fi
if grep -q "DPX86:SHARD-BUS" "$serial_log"; then
  grep "DPX86:SHARD-BUS" "$serial_log"
fi
if grep -q "DPX86:SHARD-FAIRNESS" "$serial_log"; then
  grep "DPX86:SHARD-FAIRNESS" "$serial_log"
fi

echo "x86_64 virtio network MMU smoke passed."
