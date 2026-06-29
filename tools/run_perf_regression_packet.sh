#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

if [ "$#" -ne 0 ]; then
  echo "ERROR: run_perf_regression_packet.sh takes no arguments" >&2
  exit 2
fi

if [ -n "${STRICT_FIVE_CALIBRATION:-}" ]; then
  echo "ERROR: perf-regression-packet must run the fixed strict-five gate; unset STRICT_FIVE_CALIBRATION" >&2
  exit 1
fi

echo "[perf-packet] commit: $(git rev-parse HEAD)"
echo "[perf-packet] date_utc: $(date -u +%Y-%m-%dT%H:%M:%SZ)"
echo "[perf-packet] kernel: $(uname -srmo)"

if command -v lscpu >/dev/null 2>&1; then
  echo "[perf-packet] cpu_summary:"
  lscpu | sed 's/^/[perf-packet]   /'
else
  echo "[perf-packet] cpu_summary: lscpu unavailable"
fi

echo "[perf-packet] strict_five_cmd: ./tools/run_strict_five.sh"
echo "[perf-packet] strict_five_metadata:"
printf '%s\n' \
  "[perf-packet]   TASKS=4096 YIELDS=1 ROUNDS=14000 HOT=1 -> native_task_hot_run" \
  "[perf-packet]   FLOODER_TASKS=25000000 FLOODER_YIELDS=1 FLOODER_BURST=64 HOT_TASKS=0 -> native_task_flooder" \
  "[perf-packet]   REACTOR_KIND=bump HOT_TASKS=41943040 HOT_YIELDS=1 -> std_future_hot_run" \
  "[perf-packet]   REACTOR_KIND=bump SHARDS=2 FLOODER_TASKS=60000000 FLOODER_YIELDS=1 FLOODER_BURST=64 PIN_THREADS=1 -> reactor_two_shard_flooder" \
  "[perf-packet]   REACTOR_KIND=bump ONESHOT_MODE=direct_two_shard_native ONESHOT_PAIRS=25000000 ONESHOT_BUDGET=4096 PIN_THREADS=1 -> reactor_oneshot_bench"

./tools/run_strict_five.sh
