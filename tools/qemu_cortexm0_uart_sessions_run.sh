#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

target="thumbv6m-none-eabi"
elf="target/$target/release/dataplane-qemu-cortexm0-smoke"
tmpdir="$(mktemp -d /tmp/dataplane-qemu-uart.XXXXXX)"
sock="$tmpdir/uart.sock"
trace="$tmpdir/gpio.trace"
qemu_pid=""

cleanup() {
  if [[ -n "$qemu_pid" ]] && kill -0 "$qemu_pid" >/dev/null 2>&1; then
    kill "$qemu_pid" >/dev/null 2>&1 || true
    wait "$qemu_pid" >/dev/null 2>&1 || true
  fi
  rm -rf "$tmpdir"
}
trap cleanup EXIT

echo "=== QEMU Cortex-M0 UART Session Fairness Smoke ==="

if [[ ! -f "$elf" ]]; then
  echo "FAIL: release ELF not found: $elf"
  echo "Run: make qemu-cortexm0-smoke-build"
  exit 1
fi

timeout 25s qemu-system-arm \
  -M microbit \
  -cpu cortex-m0 \
  -nographic \
  -monitor none \
  -chardev socket,id=uart0,path="$sock",server=on,wait=off \
  -serial chardev:uart0 \
  -semihosting-config enable=on,target=native \
  -trace enable=nrf51_gpio_write,file="$trace" \
  -kernel "$elf" &
qemu_pid=$!

python3 - "$sock" "$trace" <<'PY'
import os
import socket
import sys
import time

sock_path = sys.argv[1]
trace_path = sys.argv[2]

FRAME_IN_MAGIC = 0xA5
FRAME_ACK_MAGIC = 0x5A
TARGETS = [64, 24, 24]
MAX_ACTIVE_ACK_GAP = 4

deadline = time.monotonic() + 10.0
while not os.path.exists(sock_path):
    if time.monotonic() > deadline:
        raise SystemExit("FAIL: QEMU UART socket did not appear")
    time.sleep(0.01)

client = socket.socket(socket.AF_UNIX, socket.SOCK_STREAM)
client.settimeout(5.0)
while True:
    try:
        client.connect(sock_path)
        break
    except (FileNotFoundError, ConnectionRefusedError):
        if time.monotonic() > deadline:
            raise SystemExit("FAIL: could not connect to QEMU UART socket")
        time.sleep(0.01)

frames = []
for round_ in range(TARGETS[0]):
    frames.append((0, 1))
    if round_ < TARGETS[1]:
        frames.append((1, 1))
    if round_ < TARGETS[2]:
        frames.append((2, 1))

payload = bytearray()
for session, units in frames:
    payload.extend((FRAME_IN_MAGIC, session, units, FRAME_IN_MAGIC ^ session ^ units))
client.sendall(payload)

acks = [0, 0, 0]
ack_stream = []
active_gap = [0, 0, 0]
max_active_gap = [0, 0, 0]
buf = bytearray()

read_deadline = time.monotonic() + 10.0
while acks != TARGETS:
    if time.monotonic() > read_deadline:
        raise SystemExit(f"FAIL: timed out waiting for ACKs; got {acks}, expected {TARGETS}")
    chunk = client.recv(64)
    if not chunk:
        raise SystemExit(f"FAIL: UART closed before all ACKs; got {acks}, expected {TARGETS}")
    buf.extend(chunk)

    while len(buf) >= 4:
        if buf[0] != FRAME_ACK_MAGIC:
            del buf[0]
            continue
        magic, session, count, checksum = buf[:4]
        del buf[:4]
        if checksum != (magic ^ session ^ count):
            raise SystemExit("FAIL: bad ACK checksum")
        if session >= len(TARGETS):
            raise SystemExit(f"FAIL: bad ACK session {session}")
        if count != acks[session] + 1:
            raise SystemExit(
                f"FAIL: non-monotonic ACK for session {session}: got {count}, expected {acks[session] + 1}"
            )

        for idx, target in enumerate(TARGETS):
            if acks[idx] < target:
                active_gap[idx] += 1
                max_active_gap[idx] = max(max_active_gap[idx], active_gap[idx])

        acks[session] += 1
        active_gap[session] = 0
        ack_stream.append(session)

for idx, gap in enumerate(max_active_gap):
    if gap > MAX_ACTIVE_ACK_GAP:
        raise SystemExit(
            f"FAIL: session {idx} starved for {gap} active ACK slots; limit {MAX_ACTIVE_ACK_GAP}"
        )

trace_deadline = time.monotonic() + 2.0
gpio_writes = 0
while time.monotonic() < trace_deadline:
    if os.path.exists(trace_path):
        with open(trace_path, "r", encoding="utf-8", errors="replace") as trace_file:
            gpio_writes = sum(1 for line in trace_file if "nrf51_gpio_write" in line)
        if gpio_writes >= 6:
            break
    time.sleep(0.05)

if gpio_writes < 6:
    raise SystemExit(f"FAIL: expected at least 6 GPIO writes while sessions ran, saw {gpio_writes}")

print(f"UART ACKs: {acks}")
print(f"Max active ACK gaps: {max_active_gap}")
print(f"GPIO writes observed: {gpio_writes}")
PY

wait "$qemu_pid"
qemu_pid=""

echo "QEMU Cortex-M0 UART session fairness smoke passed."
