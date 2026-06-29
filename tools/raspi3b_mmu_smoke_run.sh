#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

elf="target/aarch64-unknown-none/release/dataplane-raspi3b-mmu-smoke"
uart_log="$(mktemp "${TMPDIR:-/tmp}/dataplane-raspi3b-uart.XXXXXX")"
uart_dir="$(mktemp -d "${TMPDIR:-/tmp}/dataplane-raspi3b-uart.XXXXXX")"
uart_sock="$uart_dir/serial.sock"
qemu_log="$(mktemp "${TMPDIR:-/tmp}/dataplane-raspi3b-qemu.XXXXXX")"
net_pcap="$(mktemp "${TMPDIR:-/tmp}/dataplane-raspi3b-net.XXXXXX.pcap")"
qemu_pid=""
uart_pid=""

cleanup() {
  if [[ -n "$uart_pid" ]]; then
    kill "$uart_pid" 2>/dev/null || true
  fi
  if [[ -n "$qemu_pid" ]]; then
    kill "$qemu_pid" 2>/dev/null || true
  fi
  rm -f "$uart_log" "$qemu_log" "$net_pcap"
  rm -rf "$uart_dir"
}
trap cleanup EXIT

echo "=== Raspi3B MMU Protected Shard Smoke ==="

if [[ ! -f "$elf" ]]; then
  echo "FAIL: release ELF not found: $elf"
  echo "Run: make raspi3b-mmu-build"
  exit 1
fi

timeout 10s qemu-system-aarch64 \
  -M raspi3b \
  -cpu cortex-a53 \
  -nographic \
  -monitor none \
  -serial "unix:$uart_sock,server=on,wait=on" \
  -netdev user,id=raspi_net \
  -object "filter-dump,id=raspi_dump,netdev=raspi_net,file=$net_pcap" \
  -device usb-net,netdev=raspi_net \
  -semihosting-config enable=on,target=native \
  -kernel "$elf" >"$qemu_log" 2>&1 &
qemu_pid="$!"

for _ in $(seq 1 100); do
  [[ -S "$uart_sock" ]] && break
  sleep 0.05
done

if [[ ! -S "$uart_sock" ]]; then
  echo "FAIL: Raspi3B VM UART socket was not created"
  sed -n '1,80p' "$qemu_log"
  exit 1
fi

if command -v socat >/dev/null 2>&1; then
  coproc UART { socat - "UNIX-CONNECT:$uart_sock" 2>>"$qemu_log"; }
elif command -v nc >/dev/null 2>&1; then
  coproc UART { nc -U "$uart_sock" 2>>"$qemu_log"; }
else
  echo "FAIL: neither socat nor nc is available for the Raspi3B VM UART socket"
  exit 1
fi
uart_pid="$UART_PID"

printf 'DPHOST?\n' >&"${UART[1]}"
uart_response=""
if ! IFS= read -r -N 8 -t 5 uart_response <&"${UART[0]}"; then
  echo "FAIL: Raspi3B VM UART response was not readable"
  sed -n '1,80p' "$qemu_log"
  exit 1
fi
printf '%s' "$uart_response" >"$uart_log"

if ! wait "$qemu_pid"; then
  echo "FAIL: Raspi3B MMU smoke QEMU run failed"
  sed -n '1,120p' "$qemu_log"
  exit 1
fi
qemu_pid=""

kill "$uart_pid" 2>/dev/null || true
wait "$uart_pid" 2>/dev/null || true
uart_pid=""

if ! grep -q "DPVM:OK" "$uart_log"; then
  echo "FAIL: Raspi3B VM UART challenge/response marker not observed"
  echo "--- UART log ---"
  sed -n '1,40p' "$uart_log"
  exit 1
fi

if [[ ! -s "$net_pcap" ]]; then
  echo "FAIL: Raspi3B VM net backend pcap was not written"
  sed -n '1,120p' "$qemu_log"
  exit 1
fi

net_pcap_hex="$(od -An -tx1 -v "$net_pcap" | tr -d ' \n')"
if [[ "$net_pcap_hex" != *"ffffffffffff02000000000188b5"* ]]; then
  echo "FAIL: Raspi3B VM net backend did not capture the deterministic Ethernet probe frame"
  echo "Expected frame prefix: ff ff ff ff ff ff 02 00 00 00 00 01 88 b5"
  exit 1
fi

echo "Raspi3B MMU protected shard smoke passed."
