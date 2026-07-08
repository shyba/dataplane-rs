#!/usr/bin/env python3
import importlib.util
import os
import select
import sqlite3
import tempfile
import unittest
from pathlib import Path


SCRIPT = Path(__file__).with_name("scd41_sync_sqlite.py")
spec = importlib.util.spec_from_file_location("scd41_sync_sqlite", SCRIPT)
scd41 = importlib.util.module_from_spec(spec)
assert spec.loader is not None
spec.loader.exec_module(scd41)


def word_with_crc(value: int) -> bytes:
    word = value.to_bytes(2, "big")
    return word + bytes([scd41.sensirion_crc8(word)])


class Scd41SyncSqliteTests(unittest.TestCase):
    def test_store_page_decodes_all_measurements(self) -> None:
        raw = word_with_crc(500) + word_with_crc(25_000) + word_with_crc(40_000)
        payload = raw * scd41.RECORDS_PER_PAGE

        with tempfile.TemporaryDirectory() as tmp:
            db_path = Path(tmp) / "scd41.sqlite"
            conn = sqlite3.connect(db_path)
            scd41.init_db(conn)
            scd41.store_page(conn, 7, payload.hex())

            pages = conn.execute("select counter, payload_hex from pages").fetchall()
            self.assertEqual(pages, [(7, payload.hex())])

            count, min_co2, max_co2 = conn.execute(
                "select count(*), min(co2_ppm), max(co2_ppm) from measurements"
            ).fetchone()
            self.assertEqual(count, scd41.RECORDS_PER_PAGE)
            self.assertEqual(min_co2, 500)
            self.assertEqual(max_co2, 500)
            conn.close()

    def test_store_page_is_idempotent_for_duplicate_counter(self) -> None:
        raw = word_with_crc(650) + word_with_crc(26_000) + word_with_crc(41_000)
        payload = raw * scd41.RECORDS_PER_PAGE

        with tempfile.TemporaryDirectory() as tmp:
            db_path = Path(tmp) / "scd41.sqlite"
            conn = sqlite3.connect(db_path)
            scd41.init_db(conn)

            scd41.store_page(conn, 11, payload.hex())
            scd41.store_page(conn, 11, payload.hex())

            page_count = conn.execute("select count(*) from pages").fetchone()[0]
            measurement_count = conn.execute(
                "select count(*) from measurements"
            ).fetchone()[0]
            self.assertEqual(page_count, 1)
            self.assertEqual(measurement_count, scd41.RECORDS_PER_PAGE)
            conn.close()

    def test_crc_rejects_corrupt_measurement(self) -> None:
        raw = bytearray(word_with_crc(500) + word_with_crc(25_000) + word_with_crc(40_000))
        raw[1] ^= 0x01
        with self.assertRaises(ValueError):
            scd41.decode_measurement(bytes(raw))

    def test_store_page_rejects_wrong_payload_length(self) -> None:
        raw = word_with_crc(500) + word_with_crc(25_000) + word_with_crc(40_000)
        with tempfile.TemporaryDirectory() as tmp:
            db_path = Path(tmp) / "scd41.sqlite"
            conn = sqlite3.connect(db_path)
            scd41.init_db(conn)
            with self.assertRaises(ValueError):
                scd41.store_page(conn, 3, raw.hex())
            conn.close()

    def test_parse_page_accepts_only_page_lines(self) -> None:
        payload = "00" * (scd41.RECORDS_PER_PAGE * scd41.RAW_MEASUREMENT_BYTES)
        self.assertEqual(scd41.parse_page(f"PAGE 42 {payload}\n"), (42, payload))
        self.assertIsNone(scd41.parse_page("OK"))
        self.assertIsNone(scd41.parse_page("co2_ppm=500 temperature_c=20.00"))
        self.assertIsNone(scd41.parse_page(f"PAGE nope {payload}"))

    def test_decode_measurement_uses_scd41_conversion(self) -> None:
        raw = word_with_crc(700) + word_with_crc(30_000) + word_with_crc(45_000)
        co2, temperature_c, humidity_rh = scd41.decode_measurement(raw)
        self.assertEqual(co2, 700)
        self.assertAlmostEqual(temperature_c, -45.0 + 175.0 * 30_000 / 65535.0)
        self.assertAlmostEqual(humidity_rh, 100.0 * 45_000 / 65535.0)

    def test_write_all_retries_partial_nonblocking_writes(self) -> None:
        calls: list[bytes] = []
        original_write = os.write
        original_select = select.select

        def fake_write(fd: int, data: bytes) -> int:
            self.assertEqual(fd, 99)
            calls.append(data)
            if len(calls) == 1:
                raise BlockingIOError()
            return min(2, len(data))

        def fake_select(read, write, error, timeout=None):
            return [], write, []

        try:
            os.write = fake_write  # type: ignore[assignment]
            select.select = fake_select  # type: ignore[assignment]
            scd41.write_all(99, b"READ 4\n")
        finally:
            os.write = original_write  # type: ignore[assignment]
            select.select = original_select  # type: ignore[assignment]

        self.assertGreater(len(calls), 1)

    def test_backfill_read_amount_includes_current_page(self) -> None:
        self.assertEqual(scd41.backfill_read_amount(300, 301), 0)
        self.assertEqual(scd41.backfill_read_amount(300, 500), 200)
        self.assertEqual(scd41.backfill_read_amount(500, 499), 0)


if __name__ == "__main__":
    unittest.main()
