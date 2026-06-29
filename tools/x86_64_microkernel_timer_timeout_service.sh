#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

export DP_MICROKERNEL_TIMER_TIMEOUT_SERVICE_PROOF=1
exec ./tools/x86_64_microkernel_fat32_run.sh --timer-timeout-service-proof "$@"
