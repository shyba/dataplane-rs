#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

plan="aidocs/055_microkernel_tls_deferred_robust_appliance_plan_2026-05-31.md"
guest="crates/dataplane-x86_64-microkernel-smoke/src/main.rs"
http="crates/dataplane-x86_64-microkernel-smoke/src/http.rs"
runner="tools/x86_64_microkernel_fat32_run.sh"
makefile="Makefile"

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
  local status
  set +e
  rg -Fq -- "$literal" "$path"
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    return 0
  fi
  if [[ "$status" -eq 1 ]]; then
    echo "$message" >&2
    echo "missing literal in $path: $literal" >&2
    exit 1
  fi
  echo "rg failed while scanning $path for required literal: $literal" >&2
  exit 1
}

reject_regex() {
  local path="$1"
  local regex="$2"
  local message="$3"
  local tmp status
  tmp="$(mktemp)"
  set +e
  rg -n -- "$regex" "$path" >"$tmp" 2>&1
  status=$?
  set -e
  if [[ "$status" -eq 0 ]]; then
    echo "$message" >&2
    cat "$tmp" >&2
    rm -f "$tmp"
    exit 1
  fi
  if [[ "$status" -ne 1 ]]; then
    echo "rg failed while scanning $path for forbidden regex: $regex" >&2
    cat "$tmp" >&2
    rm -f "$tmp"
    exit 1
  fi
  rm -f "$tmp"
}

reject_regex_except_tls_absence_checks() {
  local path="$1"
  local regex="$2"
  local message="$3"
  local tmp filtered status
  tmp="$(mktemp)"
  filtered="$(mktemp)"
  set +e
  rg -n -- "$regex" "$path" >"$tmp" 2>&1
  status=$?
  set -e
  if [[ "$status" -eq 1 ]]; then
    rm -f "$tmp" "$filtered"
    return 0
  fi
  if [[ "$status" -ne 0 ]]; then
    echo "rg failed while scanning $path for forbidden regex: $regex" >&2
    cat "$tmp" >&2
    rm -f "$tmp" "$filtered"
    exit 1
  fi
  grep -v \
    -e 'cli_operator_surface_polish_tls_deferred_ok' \
    -e 'b"HTTPS" not in captured' \
    -e 'b"DP_MICROKERNEL_TLS_PROOF" not in captured' \
    -e 'b"--tls-proof" not in captured' \
    "$tmp" >"$filtered" || true
  if [[ -s "$filtered" ]]; then
    echo "$message" >&2
    cat "$filtered" >&2
    rm -f "$tmp" "$filtered"
    exit 1
  fi
  rm -f "$tmp" "$filtered"
}

echo "=== x86_64 Microkernel HTTP Static Appliance Polish Contract Guard ==="

for path in "$plan" "$guest" "$http" "$runner" "$makefile"; do
  require_file "$path"
done

require_literal "$plan" "### 1. HTTP Static Appliance Polish" \
  "focused plan must name the HTTP static appliance polish packet"
require_literal "$plan" "x86_64-microkernel-http-static-appliance-polish" \
  "focused plan must identify the HTTP static appliance polish packet name"
require_literal "$plan" "make x86_64-microkernel-http-static-appliance-polish\`: pass;" \
  "focused plan must record the HTTP static appliance polish evidence line"
require_literal "$plan" "TLS is deferred." \
  "focused plan must keep TLS deferred"

require_literal "$http" 'target_bytes == b"/" || target_bytes == b"/INDEX.HTM"' \
  "guest parser must route / as the index alias"
require_literal "$http" 'target_bytes == b"/STATUS.HTM" || target_bytes == b"/STATUS.TXT"' \
  "guest parser must route /STATUS.TXT as the bounded status alias"
require_literal "$http" 'HttpMethod::Head if parsed.target == HttpTarget::Chain' \
  "guest must support HEAD for multi-sector /CHAIN.HTM"
require_literal "$http" 'HttpMethod::Head if parsed.target == HttpTarget::Large' \
  "guest must support HEAD for /LARGE.HTM"
require_literal "$http" 'fn content_type(kind: HttpResponseKind)' \
  "guest must keep content-type selection explicit and bounded"
require_literal "$http" 'b"GET / HTTP/1.0' \
  "guest policy probe must cover root alias"
require_literal "$http" 'b"HEAD /CHAIN.HTM HTTP/1.0' \
  "guest policy probe must cover multi-sector HEAD"
require_literal "$http" 'b"GET /STATUS.TXT HTTP/1.0' \
  "guest policy probe must cover status text alias"

require_literal "$runner" 'DP_MICROKERNEL_HTTP_STATIC_APPLIANCE_POLISH_PROOF' \
  "runner must expose a focused HTTP static polish proof mode"
require_literal "$runner" '--http-static-appliance-polish-proof' \
  "runner must accept the HTTP static polish mode"
require_literal "$runner" 'curl_root_url="http://127.0.0.1:$curl_port/"' \
  "runner must request the root alias"
require_literal "$runner" 'curl_head_chain_url="http://127.0.0.1:$curl_port/CHAIN.HTM"' \
  "runner must request body-free multi-sector HEAD"
require_literal "$runner" 'curl_status_txt_url="http://127.0.0.1:$curl_port/STATUS.TXT"' \
  "runner must request status text alias"
require_literal "$runner" 'curl_head_chain_body_bytes=0' \
  "runner summary must prove HEAD /CHAIN.HTM is body-free"
require_literal "$runner" 'http_static_appliance_polish_summary_status=pass' \
  "runner must write a focused HTTP static polish summary"
require_literal "$runner" 'http_static_appliance_polish_tls_deferred=true' \
  "runner summary must keep TLS explicitly deferred"

require_literal "$makefile" "x86_64-microkernel-http-static-appliance-polish-contract:" \
  "Makefile must expose the HTTP static polish contract target"
require_literal "$makefile" "x86_64-microkernel-http-static-appliance-polish:" \
  "Makefile must expose the HTTP static polish proof target"

reject_regex_except_tls_absence_checks "$runner" 'HTTPS|OpenSSL|certificate|private[[:space:]]+key|curl[[:space:]]+-k|DP_MICROKERNEL_TLS_PROOF|--tls-proof' \
  "HTTP static polish runner must not add TLS/HTTPS/certificate tooling"
reject_regex "$runner" 'keep-alive|chunked|CGI|scripting|generic[[:space:]]+socket|socket[[:space:]]+api|dynamic[[:space:]]+template|template[[:space:]]+language' \
  "HTTP static polish runner must not broaden into a web framework"
reject_regex "$runner" 'benchmark[[:space:]]+tuning|retun(e|ing)[[:space:]]+bench|bench[^[:space:]]*[[:space:]]+retun(e|ing)|calibration|strict-five|hardware-readiness' \
  "HTTP static polish runner must not mix benchmark or hardware claims"

echo "x86_64 microkernel HTTP static appliance polish contract guard passed."
