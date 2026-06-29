#!/usr/bin/env bash

# Non-failing RP2040 hardware status reporter.
# Reports probe-rs/cargo-embed/elf2uf2/picotool/openocd availability and board detection.
# Does NOT claim hardware validation is complete.

set +euo pipefail

echo "=== RP2040 Hardware Status ==="

flash_tools=(
  probe-rs
  cargo-embed
  elf2uf2
  elf2uf2-rs
  picotool
  openocd
)

missing_tools=()
for tool in "${flash_tools[@]}"; do
  if command -v "$tool" >/dev/null 2>&1; then
    echo "$tool: available"
  else
    missing_tools+=("$tool")
  fi
done

if ((${#missing_tools[@]} > 0)); then
  echo "Flash/debug tools missing: ${missing_tools[*]}"
else
  echo "Flash/debug tools: all present"
fi

# Board detection - check for Pico mass storage, serial, and debug probe
board_detected=false

# Check for Pico mass storage device (bootrom drive)
if [ -d /media/*/RPI-RP2 ] || [ -d /Volumes/RPI-RP2 ] || ls /dev/disk/by-label/RPI-RP2* >/dev/null 2>&1; then
  echo "Board: Pico mass storage detected"
  board_detected=true
fi

# Check for RP2040 serial devices
if [ -e /dev/ttyACM0 ] || [ -e /dev/ttyACM1 ] || ls /dev/serial/by-id/*Raspberry* >/dev/null 2>&1 2>/dev/null; then
  echo "Board: RP2040 serial device detected"
  board_detected=true
fi

# Check for common debug probes
if lsusb | grep -iE "c251|2e8a|obicro" >/dev/null 2>&1 || command -v picotool >/dev/null 2>&1; then
  # picotool can sometimes detect connected Pico
  if picotool info >/dev/null 2>&1; then
    echo "Board: Picotool reports device connected"
    board_detected=true
  fi
fi

if [ "$board_detected" = false ]; then
  echo "Board: No RP2040 board detected"
fi

echo ""
echo "Hardware status complete (not a validation - tooling/board may still be unavailable)."
