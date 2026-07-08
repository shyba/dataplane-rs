#!/usr/bin/env python3
import argparse
import fcntl
import os
import select
import sqlite3
import struct
import termios
import time
import tty


RECORDS_PER_PAGE = 27
RAW_MEASUREMENT_BYTES = 9


def sensirion_crc8(data: bytes) -> int:
    crc = 0xFF
    for byte in data:
        crc ^= byte
        for _ in range(8):
            if crc & 0x80:
                crc = ((crc << 1) ^ 0x31) & 0xFF
            else:
                crc = (crc << 1) & 0xFF
    return crc


def decode_measurement(raw: bytes) -> tuple[int, float, float]:
    if len(raw) != RAW_MEASUREMENT_BYTES:
        raise ValueError("raw measurement must be 9 bytes")
    for offset in (0, 3, 6):
        word = raw[offset : offset + 2]
        if sensirion_crc8(word) != raw[offset + 2]:
            raise ValueError("measurement crc mismatch")
    co2 = int.from_bytes(raw[0:2], "big")
    raw_temp = int.from_bytes(raw[3:5], "big")
    raw_rh = int.from_bytes(raw[6:8], "big")
    temperature_c = -45.0 + 175.0 * raw_temp / 65535.0
    humidity_rh = 100.0 * raw_rh / 65535.0
    return co2, temperature_c, humidity_rh


def open_serial(path: str) -> int:
    fd = os.open(path, os.O_RDWR | os.O_NOCTTY | os.O_NONBLOCK)
    tty.setraw(fd)
    attrs = termios.tcgetattr(fd)
    attrs[4] = termios.B115200
    attrs[5] = termios.B115200
    attrs[2] |= termios.CLOCAL | termios.CREAD
    attrs[6][termios.VMIN] = 0
    attrs[6][termios.VTIME] = 0
    termios.tcsetattr(fd, termios.TCSANOW, attrs)
    fcntl.ioctl(
        fd,
        termios.TIOCMBIS,
        struct.pack("I", termios.TIOCM_DTR | termios.TIOCM_RTS),
    )
    termios.tcflush(fd, termios.TCIFLUSH)
    return fd


def init_db(conn: sqlite3.Connection) -> None:
    conn.executescript(
        """
        create table if not exists pages (
            counter integer primary key,
            received_unix real not null,
            payload_hex text not null
        );
        create table if not exists measurements (
            counter integer not null,
            sample_index integer not null,
            co2_ppm integer not null,
            temperature_c real not null,
            humidity_rh real not null,
            raw_hex text not null,
            primary key (counter, sample_index)
        );
        """
    )


def last_counter(conn: sqlite3.Connection) -> int | None:
    row = conn.execute("select max(counter) from pages").fetchone()
    return row[0] if row and row[0] is not None else None


def store_page(conn: sqlite3.Connection, counter: int, payload_hex: str) -> None:
    payload = bytes.fromhex(payload_hex)
    if len(payload) != RECORDS_PER_PAGE * RAW_MEASUREMENT_BYTES:
        raise ValueError("unexpected page payload length")

    now = time.time()
    with conn:
        conn.execute(
            """
            insert or ignore into pages(counter, received_unix, payload_hex)
            values (?, ?, ?)
            """,
            (counter, now, payload_hex),
        )
        for index in range(RECORDS_PER_PAGE):
            raw = payload[
                index * RAW_MEASUREMENT_BYTES : (index + 1) * RAW_MEASUREMENT_BYTES
            ]
            co2, temperature_c, humidity_rh = decode_measurement(raw)
            conn.execute(
                """
                insert or ignore into measurements(
                    counter, sample_index, co2_ppm, temperature_c, humidity_rh, raw_hex
                ) values (?, ?, ?, ?, ?, ?)
                """,
                (counter, index, co2, temperature_c, humidity_rh, raw.hex()),
            )


def parse_page(line: str) -> tuple[int, str] | None:
    parts = line.strip().split()
    if len(parts) != 3 or parts[0] != "PAGE":
        return None
    try:
        counter = int(parts[1])
    except ValueError:
        return None
    return counter, parts[2]


def backfill_read_amount(last_seen: int, current_counter: int) -> int:
    if current_counter <= last_seen + 1:
        return 0
    # READ N returns the newest N pages, including current_counter. Including
    # the current page is intentional so the first missing page is still in
    # range; inserts are idempotent for the duplicate current page.
    return current_counter - last_seen


def write_all(fd: int, data: bytes) -> None:
    offset = 0
    deadline = time.monotonic() + 5.0
    while offset < len(data):
        try:
            written = os.write(fd, data[offset:])
        except BlockingIOError:
            written = 0

        if written > 0:
            offset += written
            continue

        if time.monotonic() >= deadline:
            raise TimeoutError("serial write timed out")
        _, writable, _ = select.select([], [fd], [], 0.1)
        if not writable and time.monotonic() >= deadline:
            raise TimeoutError("serial write timed out")


def sync(port: str, db_path: str, read_amount: int | None, timeout_s: float) -> None:
    conn = sqlite3.connect(db_path)
    init_db(conn)
    fd = open_serial(port)
    pending = b""
    last_seen = last_counter(conn)

    if read_amount is None:
        read_amount = 0 if last_seen is None else 256
    if read_amount > 0:
        write_all(fd, f"READ {read_amount}\n".encode())

    deadline = time.monotonic() + timeout_s if timeout_s > 0 else None
    try:
        while True:
            timeout = None
            if deadline is not None:
                timeout = max(0.0, deadline - time.monotonic())
                if timeout == 0.0:
                    return

            readable, _, _ = select.select([fd], [], [], timeout)
            if not readable:
                return

            try:
                chunk = os.read(fd, 4096)
            except BlockingIOError:
                continue
            if not chunk:
                continue
            pending += chunk
            while b"\n" in pending:
                raw_line, pending = pending.split(b"\n", 1)
                line = raw_line.decode("ascii", errors="replace").strip()
                parsed = parse_page(line)
                if parsed is None:
                    if line == "OK":
                        continue
                    print(line)
                    continue
                counter, payload_hex = parsed
                if last_seen is not None:
                    missing = backfill_read_amount(last_seen, counter)
                    if missing > 0:
                        write_all(fd, f"READ {missing}\n".encode())
                try:
                    store_page(conn, counter, payload_hex)
                except ValueError as err:
                    print(f"ignored PAGE {counter}: {err}")
                    continue
                if last_seen is None or counter > last_seen:
                    last_seen = counter
                print(f"stored PAGE {counter}")
    finally:
        try:
            os.close(fd)
        except OSError:
            pass
        conn.close()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("port")
    parser.add_argument("database")
    parser.add_argument("--read", type=int, default=None)
    parser.add_argument("--timeout", type=float, default=0.0)
    args = parser.parse_args()
    sync(args.port, args.database, args.read, args.timeout)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
