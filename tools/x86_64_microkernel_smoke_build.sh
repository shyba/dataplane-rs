#!/usr/bin/env bash
set -euo pipefail

cd "$(dirname "$0")/.."

crate="crates/dataplane-x86_64-microkernel-smoke"
target="x86_64-unknown-none"
elf="target/$target/release/dataplane-x86_64-microkernel-smoke"
kernel_bin="target/$target/release/dataplane-x86_64-microkernel-smoke.bin"
boot_bin="target/$target/release/x86_64-microkernel-boot.bin"
stage2_bin="target/$target/release/x86_64-microkernel-stage2.bin"
stage2_pad="target/$target/release/x86_64-microkernel-stage2.pad"
image="target/$target/release/dataplane-x86_64-microkernel-smoke.img"

echo "=== x86_64 Microkernel Smoke Build ==="

if [[ ! -d "$crate" ]]; then
  echo "FAIL: microkernel smoke crate not found: $crate"
  exit 1
fi

features="bare-metal-bin"
if [[ -n "${X86_64_MICROKERNEL_FEATURES:-}" ]]; then
  features="$features,${X86_64_MICROKERNEL_FEATURES}"
fi
features_args=(--features "$features")

RUSTFLAGS="-C relocation-model=static -C link-arg=-no-pie -C link-arg=-Tlayout.ld" \
  cargo build -p dataplane-x86_64-microkernel-smoke --target "$target" --release "${features_args[@]}"

objcopy -O binary "$elf" "$kernel_bin"
kernel_size="$(stat -c '%s' "$kernel_bin")"
kernel_sectors=$(((kernel_size + 511) / 512))

as --32 "$crate/boot/boot.S" -o "$boot_bin.o"
ld -m elf_i386 -Ttext 0x7c00 --oformat binary "$boot_bin.o" -o "$boot_bin"

as --32 --defsym KERNEL_SECTORS="$kernel_sectors" "$crate/boot/stage2.S" -o "$stage2_bin.o"
ld -m elf_i386 -e stage2_start -Ttext 0x8000 --oformat binary "$stage2_bin.o" -o "$stage2_bin"

boot_size="$(stat -c '%s' "$boot_bin")"
stage2_size="$(stat -c '%s' "$stage2_bin")"

if [[ "$boot_size" -ne 512 ]]; then
  echo "FAIL: boot sector must be exactly 512 bytes, got $boot_size"
  exit 1
fi

if [[ "$stage2_size" -gt 8192 ]]; then
  echo "FAIL: stage2 must fit in 16 sectors, got $stage2_size bytes"
  exit 1
fi

if [[ "$kernel_size" -gt $((512 * 512)) ]]; then
  echo "FAIL: kernel binary must fit in 512 sectors, got $kernel_size bytes"
  exit 1
fi

cp "$stage2_bin" "$stage2_pad"
truncate -s 8192 "$stage2_pad"

truncate -s 1474560 "$image"
dd if="$boot_bin" of="$image" conv=notrunc status=none
dd if="$stage2_pad" of="$image" bs=512 seek=1 conv=notrunc status=none
dd if="$kernel_bin" of="$image" bs=512 seek=17 conv=notrunc status=none

echo "x86_64 microkernel smoke image: $image"
echo "kernel bytes: $kernel_size"
echo "x86_64 microkernel smoke build passed."
