#!/usr/bin/env bash

set -euo pipefail

cd "$(dirname "$0")/.."

crate="dataplane-core-reactor-alloc"
manifest="crates/${crate}/Cargo.toml"
target="thumbv6m-none-eabi"

echo "=== RP2040 Alloc Hygiene Guard ==="

if ! awk '
BEGIN { in_dev = 0; ok = 1 }
{
  line = $0
  sub(/[[:space:]]*#.*$/, "", line)
  gsub(/^[[:space:]]+|[[:space:]]+$/, "", line)
  if (line == "") next

  if (line == "[dev-dependencies]") {
    in_dev = 1
    next
  }
  if (line ~ /^\[(dependencies|build-dependencies)\]$/) {
    print "unexpected direct dependency section: " line > "/dev/stderr"
    ok = 0
  }
  if (line ~ /^\[target\..*\.dependencies\]$/ || line ~ /^\[target\..*\.build-dependencies\]$/) {
    print "unexpected target dependency section: " line > "/dev/stderr"
    ok = 0
  }
}
END {
  exit ok ? 0 : 1
}
' "$manifest"; then
  echo "alloc crate manifest contains disallowed dependency sections" >&2
  exit 1
fi

tree_output="$(cargo tree -p "$crate" --target "$target" -e normal,build --prefix none)"
tree_lines="$(printf '%s\n' "$tree_output" | sed '/^[[:space:]]*$/d')"
if [ "$(printf '%s\n' "$tree_lines" | wc -l | tr -d ' ')" -ne 1 ]; then
  echo "unexpected normal/build dependency tree for $crate on $target:" >&2
  printf '%s\n' "$tree_lines" >&2
  exit 1
fi

case "$tree_lines" in
  "$crate v"*)
    ;;
  *)
    echo "unexpected cargo tree root for $crate on $target:" >&2
    printf '%s\n' "$tree_lines" >&2
    exit 1
    ;;
esac

cargo check -p "$crate" --target "$target" --lib
echo "RP2040 alloc hygiene guard passed."
