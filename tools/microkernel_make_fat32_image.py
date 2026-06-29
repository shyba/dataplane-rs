#!/usr/bin/env python3
"""Build a deterministic FAT32 superfloppy image for microkernel QEMU tests."""

from __future__ import annotations

import argparse
import pathlib
import struct

BYTES_PER_SECTOR = 512
SECTORS_PER_CLUSTER = 1
RESERVED_SECTORS = 32
FAT_COUNT = 2
FAT_SECTORS = 1024
TOTAL_SECTORS = 131_072
ROOT_CLUSTER = 2
HELLO_CLUSTER = 3
INDEX_CLUSTER = 4
LARGE_CLUSTER = 6
CHAIN_CLUSTER_FIRST = 7
CHAIN_CLUSTER_SECOND = 8
JOURNAL_CLUSTER = 9
VOLUME_LABEL = b"DATAPLANE  "
HELLO_NAME = b"HELLO   TXT"
INDEX_NAME = b"INDEX   HTM"
LARGE_NAME = b"LARGE   HTM"
CHAIN_NAME = b"CHAIN   HTM"
JOURNAL_NAME = b"JOURNAL BIN"
HELLO_CONTENT = b"hello from dataplane microkernel fat32\r\n"
INDEX_CONTENT = (
    b"<!doctype html><html><head><title>dataplane</title></head>"
    b"<body><h1>dataplane microkernel</h1></body></html>\r\n"
)
LARGE_CONTENT = (
    b"<!doctype html><html><head><title>dataplane backpressure</title></head><body><pre>"
    + b"dataplane-backpressure-proof-0123456789abcdef\r\n" * 8
    + b"</pre></body></html>\r\n"
)
CHAIN_CONTENT = (
    b"<!doctype html><html><head><title>dataplane chain</title></head><body><pre>"
    + b"dataplane-fs-service-boundary-multicluster-proof-0123456789abcdef\r\n" * 11
    + b"</pre></body></html>\r\n"
)


def le16(value: int) -> bytes:
    return struct.pack("<H", value)


def le32(value: int) -> bytes:
    return struct.pack("<I", value)


def sector_offset(sector: int) -> int:
    return sector * BYTES_PER_SECTOR


def data_start_sector() -> int:
    return RESERVED_SECTORS + FAT_COUNT * FAT_SECTORS


def cluster_sector(cluster: int) -> int:
    return data_start_sector() + (cluster - 2) * SECTORS_PER_CLUSTER


def short_entry(name: bytes, cluster: int, size: int) -> bytes:
    if len(name) != 11:
        raise ValueError("short FAT32 names must be exactly 11 bytes")
    entry = bytearray(32)
    entry[0:11] = name
    entry[11] = 0x20
    entry[20:22] = le16((cluster >> 16) & 0xFFFF)
    entry[26:28] = le16(cluster & 0xFFFF)
    entry[28:32] = le32(size)
    return bytes(entry)


def write_boot_sector(image: bytearray) -> None:
    boot = bytearray(BYTES_PER_SECTOR)
    boot[0:3] = b"\xeb\x58\x90"
    boot[3:11] = b"DATAPLN "
    boot[11:13] = le16(BYTES_PER_SECTOR)
    boot[13] = SECTORS_PER_CLUSTER
    boot[14:16] = le16(RESERVED_SECTORS)
    boot[16] = FAT_COUNT
    boot[17:19] = le16(0)
    boot[19:21] = le16(0)
    boot[21] = 0xF8
    boot[22:24] = le16(0)
    boot[24:26] = le16(63)
    boot[26:28] = le16(255)
    boot[28:32] = le32(0)
    boot[32:36] = le32(TOTAL_SECTORS)
    boot[36:40] = le32(FAT_SECTORS)
    boot[40:42] = le16(0)
    boot[42:44] = le16(0)
    boot[44:48] = le32(ROOT_CLUSTER)
    boot[48:50] = le16(1)
    boot[50:52] = le16(6)
    boot[64] = 0x80
    boot[66] = 0x29
    boot[67:71] = le32(0xDADA_0470)
    boot[71:82] = VOLUME_LABEL
    boot[82:90] = b"FAT32   "
    boot[510:512] = b"\x55\xaa"
    image[0:BYTES_PER_SECTOR] = boot
    image[sector_offset(6) : sector_offset(7)] = boot


def write_fsinfo(image: bytearray) -> None:
    fsinfo = bytearray(BYTES_PER_SECTOR)
    fsinfo[0:4] = le32(0x4161_5252)
    fsinfo[484:488] = le32(0x6141_7272)
    fsinfo[488:492] = le32(TOTAL_SECTORS - data_start_sector() - 3)
    fsinfo[492:496] = le32(5)
    fsinfo[508:512] = le32(0xAA55_0000)
    image[sector_offset(1) : sector_offset(2)] = fsinfo
    image[sector_offset(7) : sector_offset(8)] = fsinfo


def write_fats(image: bytearray) -> None:
    fat = bytearray(FAT_SECTORS * BYTES_PER_SECTOR)
    entries = {
        0: 0x0FFF_FFF8,
        1: 0x0FFF_FFFF,
        ROOT_CLUSTER: 0x0FFF_FFFF,
        HELLO_CLUSTER: 0x0FFF_FFFF,
        INDEX_CLUSTER: 0x0FFF_FFFF,
        LARGE_CLUSTER: 0x0FFF_FFFF,
        CHAIN_CLUSTER_FIRST: CHAIN_CLUSTER_SECOND,
        CHAIN_CLUSTER_SECOND: 0x0FFF_FFFF,
        JOURNAL_CLUSTER: 0x0FFF_FFFF,
    }
    for index, value in entries.items():
        fat[index * 4 : index * 4 + 4] = le32(value)
    first = sector_offset(RESERVED_SECTORS)
    second = sector_offset(RESERVED_SECTORS + FAT_SECTORS)
    image[first : first + len(fat)] = fat
    image[second : second + len(fat)] = fat


def write_root_directory(image: bytearray) -> None:
    root = bytearray(SECTORS_PER_CLUSTER * BYTES_PER_SECTOR)
    root[0:32] = short_entry(HELLO_NAME, HELLO_CLUSTER, len(HELLO_CONTENT))
    root[32:64] = short_entry(INDEX_NAME, INDEX_CLUSTER, len(INDEX_CONTENT))
    root[64:96] = short_entry(LARGE_NAME, LARGE_CLUSTER, len(LARGE_CONTENT))
    root[96:128] = short_entry(CHAIN_NAME, CHAIN_CLUSTER_FIRST, len(CHAIN_CONTENT))
    root[128:160] = short_entry(JOURNAL_NAME, JOURNAL_CLUSTER, BYTES_PER_SECTOR)
    start = sector_offset(cluster_sector(ROOT_CLUSTER))
    image[start : start + len(root)] = root


def write_file(image: bytearray, cluster: int, content: bytes) -> None:
    cluster_bytes = SECTORS_PER_CLUSTER * BYTES_PER_SECTOR
    if len(content) > cluster_bytes:
        raise ValueError("test file content must fit in one cluster")
    start = sector_offset(cluster_sector(cluster))
    image[start : start + len(content)] = content


def write_file_chain(image: bytearray, clusters: tuple[int, ...], content: bytes) -> None:
    cluster_bytes = SECTORS_PER_CLUSTER * BYTES_PER_SECTOR
    if len(content) <= cluster_bytes:
        raise ValueError("chain test file must exceed one cluster")
    if len(content) > cluster_bytes * len(clusters):
        raise ValueError("chain test file does not fit declared clusters")
    offset = 0
    for cluster in clusters:
        chunk = content[offset : offset + cluster_bytes]
        start = sector_offset(cluster_sector(cluster))
        image[start : start + len(chunk)] = chunk
        offset += len(chunk)


def build_image() -> bytearray:
    image = bytearray(TOTAL_SECTORS * BYTES_PER_SECTOR)
    write_boot_sector(image)
    write_fsinfo(image)
    write_fats(image)
    write_root_directory(image)
    write_file(image, HELLO_CLUSTER, HELLO_CONTENT)
    write_file(image, INDEX_CLUSTER, INDEX_CONTENT)
    write_file(image, LARGE_CLUSTER, LARGE_CONTENT)
    write_file_chain(image, (CHAIN_CLUSTER_FIRST, CHAIN_CLUSTER_SECOND), CHAIN_CONTENT)
    write_file(image, JOURNAL_CLUSTER, bytes(BYTES_PER_SECTOR))
    return image


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser()
    parser.add_argument(
        "output",
        nargs="?",
        default="/home/user/mnt/dataplane/microkernel-fat32.img",
        help="output FAT32 image path",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    output = pathlib.Path(args.output)
    output.parent.mkdir(parents=True, exist_ok=True)
    image = build_image()
    output.write_bytes(image)
    print(f"image={output}")
    print(f"bytes={len(image)}")
    print(f"hello_cluster={HELLO_CLUSTER}")
    print(f"index_cluster={INDEX_CLUSTER}")
    print(f"large_cluster={LARGE_CLUSTER}")
    print(f"chain_cluster_first={CHAIN_CLUSTER_FIRST}")
    print(f"chain_cluster_second={CHAIN_CLUSTER_SECOND}")
    print(f"journal_cluster={JOURNAL_CLUSTER}")
    print(f"root_sector={cluster_sector(ROOT_CLUSTER)}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
