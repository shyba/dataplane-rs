#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$repo_root"

mnt_root="/home/user/mnt/dataplane"
log_dir="$mnt_root/logs"
run_id="$(date -u +%Y%m%dT%H%M%SZ)-$$"
image_a="$log_dir/x86_64-microkernel-fat32-repro-$run_id-a.img"
image_b="$log_dir/x86_64-microkernel-fat32-repro-$run_id-b.img"
summary="$log_dir/x86_64-microkernel-fat32-repro-$run_id.summary"

mkdir -p "$log_dir"
python3 tools/microkernel_make_fat32_image.py "$image_a" >/dev/null
python3 tools/microkernel_make_fat32_image.py "$image_b" >/dev/null

if ! cmp -s "$image_a" "$image_b"; then
  echo "FAIL: deterministic FAT32 images differ" >&2
  exit 1
fi

python3 - "$image_a" "$image_b" "$summary" <<'PY'
import hashlib
import pathlib
import sys

image_a = pathlib.Path(sys.argv[1])
image_b = pathlib.Path(sys.argv[2])
summary = pathlib.Path(sys.argv[3])

BYTES_PER_SECTOR = 512
RESERVED_SECTORS = 32
FAT_COUNT = 2
FAT_SECTORS = 1024
ROOT_CLUSTER = 2

EXPECTED = {
    "HELLO.TXT": b"hello from dataplane microkernel fat32\r\n",
    "INDEX.HTM": (
        b"<!doctype html><html><head><title>dataplane</title></head>"
        b"<body><h1>dataplane microkernel</h1></body></html>\r\n"
    ),
    "LARGE.HTM": (
        b"<!doctype html><html><head><title>dataplane backpressure</title></head><body><pre>"
        + b"dataplane-backpressure-proof-0123456789abcdef\r\n" * 8
        + b"</pre></body></html>\r\n"
    ),
    "CHAIN.HTM": (
        b"<!doctype html><html><head><title>dataplane chain</title></head><body><pre>"
        + b"dataplane-fs-service-boundary-multicluster-proof-0123456789abcdef\r\n" * 11
        + b"</pre></body></html>\r\n"
    ),
}


def sha256(data: bytes) -> str:
    return hashlib.sha256(data).hexdigest()


def le16(data: bytes, offset: int) -> int:
    return int.from_bytes(data[offset : offset + 2], "little")


def le32(data: bytes, offset: int) -> int:
    return int.from_bytes(data[offset : offset + 4], "little")


def sector_offset(sector: int) -> int:
    return sector * BYTES_PER_SECTOR


def data_start_sector() -> int:
    return RESERVED_SECTORS + FAT_COUNT * FAT_SECTORS


def cluster_sector(cluster: int) -> int:
    return data_start_sector() + (cluster - 2)


def cluster_offset(cluster: int) -> int:
    return sector_offset(cluster_sector(cluster))


def fat_next(image: bytes, cluster: int) -> int:
    fat_offset = sector_offset(RESERVED_SECTORS) + cluster * 4
    return le32(image, fat_offset) & 0x0FFF_FFFF


def read_chain(image: bytes, cluster: int, size: int) -> bytes:
    out = bytearray()
    current = cluster
    while len(out) < size:
        out.extend(image[cluster_offset(current) : cluster_offset(current) + BYTES_PER_SECTOR])
        nxt = fat_next(image, current)
        if nxt >= 0x0FFF_FFF8:
            break
        current = nxt
    return bytes(out[:size])


def short_name(name: str) -> bytes:
    stem, ext = name.split(".")
    return stem.encode("ascii").ljust(8, b" ") + ext.encode("ascii").ljust(3, b" ")


image = image_a.read_bytes()
other = image_b.read_bytes()
if image != other:
    raise SystemExit("image mismatch after shell cmp")
if len(image) != 67_108_864:
    raise SystemExit(f"unexpected image size {len(image)}")
if image[510:512] != b"\x55\xaa":
    raise SystemExit("missing FAT32 boot signature")

root = image[cluster_offset(ROOT_CLUSTER) : cluster_offset(ROOT_CLUSTER) + BYTES_PER_SECTOR]
entries = {}
for offset in range(0, BYTES_PER_SECTOR, 32):
    entry = root[offset : offset + 32]
    if entry[0] == 0:
        continue
    name = entry[0:11]
    cluster = (le16(entry, 20) << 16) | le16(entry, 26)
    size = le32(entry, 28)
    entries[name] = (cluster, size)

with summary.open("w", encoding="ascii") as out:
    out.write("fat32_artifact_reproducibility_summary_status=pass\n")
    out.write(f"fat32_artifact_reproducibility_image_a={image_a}\n")
    out.write(f"fat32_artifact_reproducibility_image_b={image_b}\n")
    out.write(f"fat32_artifact_reproducibility_image_bytes={len(image)}\n")
    out.write(f"fat32_artifact_reproducibility_image_sha256={sha256(image)}\n")
    out.write("fat32_artifact_reproducibility_images_identical=true\n")
    for route, expected in EXPECTED.items():
        key = short_name(route)
        if key not in entries:
            raise SystemExit(f"missing FAT32 route {route}")
        cluster, size = entries[key]
        data = read_chain(image, cluster, size)
        if data != expected:
            raise SystemExit(f"unexpected bytes for {route}")
        slug = route.lower().replace(".", "_")
        out.write(f"fat32_artifact_reproducibility_{slug}_bytes={len(data)}\n")
        out.write(f"fat32_artifact_reproducibility_{slug}_sha256={sha256(data)}\n")
    out.write("fat32_artifact_reproducibility_repair_mode=false\n")
    out.write("fat32_artifact_reproducibility_default_writable=false\n")
PY

grep -q "fat32_artifact_reproducibility_summary_status=pass" "$summary"
grep -q "fat32_artifact_reproducibility_images_identical=true" "$summary"
grep -q "fat32_artifact_reproducibility_repair_mode=false" "$summary"
grep -q "fat32_artifact_reproducibility_default_writable=false" "$summary"

echo "x86_64 microkernel FAT32 artifact reproducibility passed."
echo "summary: $summary"
