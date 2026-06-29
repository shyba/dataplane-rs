#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

src="crates/dataplane-qemu-cortexm0-smoke/src/main.rs"

echo "=== QEMU Cortex-M0 Smoke Contract Guard ==="

if [[ ! -f "$src" ]]; then
  echo "FAIL: missing QEMU smoke source: $src"
  exit 1
fi

require() {
  local pattern="$1"
  local label="$2"

  if rg -q "$pattern" "$src"; then
    echo "  $label: ok"
  else
    echo "FAIL: $label not found in $src"
    exit 1
  fi
}

echo "Checking interrupt-driven execution evidence..."
require '#\[exception\]' "Cortex-M exception handler"
require 'fn SysTick\(\)' "SysTick handler"
require 'set_clock_source\(SystClkSource::Core\)' "SysTick core clock source"
require 'enable_interrupt\(\)' "SysTick interrupt enable"
require 'wait_for_next_tick' "bounded wait for interrupt tick"

echo "Checking dataplane no-alloc scheduler execution from ticks..."
require 'FixedLocalExecCounts' "fixed-capacity no-alloc queue"
require 'drive_counts_step' "dataplane step driver"
require 'while counts\.has_work\(\)' "tick-driven work loop"
require 'run_systick_flooder_like_workload' "SysTick flooder-like workload"
require 'TOTAL_TASKS: usize = 128' "fixed embedded flooder task count"
require 'BURST: usize = 8' "fixed embedded flooder burst"
require 'spawned < TOTAL_TASKS \|\| counts\.has_work\(\)' "spawn-and-drain flooder loop"

echo "Checking UART multiplexing plus blink evidence..."
require 'uart-sessions' "feature-gated UART session scenario"
require 'SESSION_TARGETS: \[usize; SESSIONS\] = \[64, 24, 24\]' "fixed three-session workload"
require 'run_three_uart_sessions_while_blinking' "three-session UART plus blink runner"
require 'FRAME_IN_MAGIC' "input frame protocol"
require 'FRAME_ACK_MAGIC' "ACK frame protocol"
require 'nrf51::set_led' "GPIO LED-equivalent toggle"

echo "QEMU Cortex-M0 smoke contract guard passed."
