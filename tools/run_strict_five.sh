#!/usr/bin/env bash
# strict-five benchmark runner — fixed parameters from AGENTS.md
set -euo pipefail
cd "$(dirname "$0")/.."

echo "=== native_task_hot_run ==="
TASKS=4096 YIELDS=1 ROUNDS=14000 HOT=1 \
  cargo run --release --manifest-path experiments/rust/hashmap_queue_bench/Cargo.toml \
  --bin native_task_hot_run

echo "=== native_task_flooder ==="
FLOODER_TASKS=25000000 FLOODER_YIELDS=1 FLOODER_BURST=64 HOT_TASKS=0 \
  cargo run --release --manifest-path experiments/rust/hashmap_queue_bench/Cargo.toml \
  --bin native_task_flooder

echo "=== std_future_hot_run ==="
REACTOR_KIND=bump HOT_TASKS=41943040 HOT_YIELDS=1 \
  cargo run --release --manifest-path experiments/rust/hashmap_queue_bench/Cargo.toml \
  --bin std_future_hot_run

echo "=== reactor_two_shard_flooder ==="
REACTOR_KIND=bump SHARDS=2 FLOODER_TASKS=60000000 FLOODER_YIELDS=1 FLOODER_BURST=64 PIN_THREADS=1 \
  cargo run --release --manifest-path experiments/rust/hashmap_queue_bench/Cargo.toml \
  --bin reactor_two_shard_flooder

echo "=== reactor_oneshot_bench ==="
REACTOR_KIND=bump ONESHOT_MODE=direct_two_shard_native ONESHOT_PAIRS=25000000 ONESHOT_BUDGET=4096 PIN_THREADS=1 \
  cargo run --release --manifest-path experiments/rust/hashmap_queue_bench/Cargo.toml \
  --bin reactor_oneshot_bench

echo "=== ALL STRICT-FIVE DONE ==="
