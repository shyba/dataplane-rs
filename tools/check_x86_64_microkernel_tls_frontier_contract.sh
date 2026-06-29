#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

note="aidocs/048_microkernel_tls_frontier_feasibility_2026-05-30.md"
roadmap="aidocs/047_microkernel_http_fat32_cli_roadmap_2026-05-30.md"
runner="tools/x86_64_microkernel_fat32_run.sh"
guest="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
smoke_manifest="crates/dataplane-x86_64-microkernel-smoke/Cargo.toml"
core_manifest="crates/dataplane-microkernel-core/Cargo.toml"

require_file() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    echo "missing required file: $path" >&2
    exit 1
  fi
}

require_literal() {
  local path="$1"
  local literal="$2"
  local message="$3"
  if ! rg -Fq -- "$literal" "$path"; then
    echo "$message" >&2
    echo "missing literal in $path: $literal" >&2
    exit 1
  fi
}

reject_regex() {
  local path="$1"
  local regex="$2"
  local message="$3"
  set +e
  rg -n -- "$regex" "$path" >/tmp/dataplane_tls_frontier_guard.$$ 2>&1
  local status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    echo "$message" >&2
    cat /tmp/dataplane_tls_frontier_guard.$$ >&2
    rm -f /tmp/dataplane_tls_frontier_guard.$$
    exit 1
  fi
  if [[ "$status" -ne 1 ]]; then
    echo "rg failed while scanning $path for $regex" >&2
    cat /tmp/dataplane_tls_frontier_guard.$$ >&2
    rm -f /tmp/dataplane_tls_frontier_guard.$$
    exit 1
  fi
  rm -f /tmp/dataplane_tls_frontier_guard.$$
}

echo "=== x86_64 Microkernel TLS Frontier Contract Guard ==="

require_file "$note"
require_file "$roadmap"
require_file "$runner"
require_file "$guest"
require_file "$smoke_manifest"
require_file "$core_manifest"

require_literal "$note" "Status: Stage J feasibility complete." \
  "TLS frontier note must declare Stage J feasibility complete"
require_literal "$note" "TLS implementation is deferred." \
  "TLS frontier note must defer implementation"
require_literal "$note" "x86_64-microkernel-bounded-tcp-stream" \
  "TLS frontier note must route the next packet to bounded TCP stream work"
require_literal "$note" "no connection table or per-connection stream state" \
  "TLS frontier note must name the missing TCP stream state"
require_literal "$note" "Allocator Policy" \
  "TLS frontier note must name allocator policy"
require_literal "$note" "Entropy Policy" \
  "TLS frontier note must name entropy policy"
require_literal "$note" "Clock And Certificate Policy" \
  "TLS frontier note must name clock and certificate policy"
require_literal "$note" "Buffer Policy" \
  "TLS frontier note must name buffer policy"
require_literal "$note" "rustls" \
  "TLS frontier note must evaluate rustls"
require_literal "$note" "embedded-tls" \
  "TLS frontier note must evaluate embedded-tls"
require_literal "$note" "curl -k https://127.0.0.1:\$port/INDEX.HTM" \
  "TLS frontier note must name future host-visible HTTPS validation"
require_literal "$note" "Do not add \`--tls-proof\`" \
  "TLS frontier note must forbid fake TLS runner mode"
require_literal "$note" "Do not emit \`DPMK:TLS-OK\` yet." \
  "TLS frontier note must forbid fake TLS markers"

require_literal "$roadmap" "Stage J1 replaced the one-shot packet/request HTTP path" \
  "roadmap must record the completed bounded TCP stream prerequisite"
require_literal "$roadmap" "Stage L: CLI operator surface is done." \
  "roadmap must record Stage L after bounded TCP stream completion"
require_literal "$roadmap" "x86_64-microkernel-tls-frontier" \
  "roadmap must name the TLS frontier packet"
require_literal "$roadmap" "x86_64-microkernel-bounded-tcp-stream" \
  "roadmap must route the post-feasibility implementation to bounded TCP stream"

reject_regex "$runner" '(^|[^A-Z0-9_])--tls-proof([^A-Z0-9_]|$)' \
  "runner must not expose --tls-proof before real TLS exists"
reject_regex "$runner" 'DP_MICROKERNEL_TLS_PROOF' \
  "runner must not expose DP_MICROKERNEL_TLS_PROOF before real TLS exists"
reject_regex "$guest" 'DPMK:TLS-OK' \
  "guest must not emit DPMK:TLS-OK before real host TLS proof exists"
reject_regex "$smoke_manifest" '(^|[[:space:]])(rustls|embedded-tls|webpki|ring|aws-lc-rs)[[:space:]]*=' \
  "x86_64 microkernel manifest must not add TLS dependencies during feasibility"
reject_regex "$core_manifest" '(^|[[:space:]])(rustls|embedded-tls|webpki|ring|aws-lc-rs)[[:space:]]*=' \
  "microkernel core manifest must stay TLS-dependency free"

echo "x86_64 microkernel TLS frontier contract guard passed."
