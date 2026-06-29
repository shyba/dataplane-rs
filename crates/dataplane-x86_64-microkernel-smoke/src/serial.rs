use core::arch::asm;

use crate::{inb, outb, CLI_LINE_TIMEOUT_SPINS};

const SERIAL_COM1: u16 = 0x3f8;

pub fn init() {
    outb(SERIAL_COM1 + 1, 0x00);
    outb(SERIAL_COM1 + 3, 0x80);
    outb(SERIAL_COM1, 0x03);
    outb(SERIAL_COM1 + 1, 0x00);
    outb(SERIAL_COM1 + 3, 0x03);
    outb(SERIAL_COM1 + 2, 0xc7);
    outb(SERIAL_COM1 + 4, 0x0b);
}

pub fn write_str(s: &str) {
    write_bytes(s.as_bytes());
}

pub fn write_bytes(bytes: &[u8]) {
    for byte in bytes {
        write_byte(*byte);
    }
}

pub fn write_decimal(mut value: u32) {
    let mut digits = [0u8; 10];
    let mut len = 0;
    if value == 0 {
        write_byte(b'0');
        return;
    }
    while value != 0 {
        digits[len] = b'0' + (value % 10) as u8;
        value /= 10;
        len += 1;
    }
    while len != 0 {
        len -= 1;
        write_byte(digits[len]);
    }
}

pub fn read_line(buffer: &mut [u8]) -> Result<&[u8], &'static str> {
    read_line_with_timeout(buffer, CLI_LINE_TIMEOUT_SPINS)?.ok_or("cli-read-timeout")
}

pub fn read_line_with_timeout(
    buffer: &mut [u8],
    max_spins: usize,
) -> Result<Option<&[u8]>, &'static str> {
    let mut len = 0;
    let mut spins = 0usize;
    while spins < max_spins {
        if let Some(byte) = try_read_byte() {
            if byte == b'\n' {
                return Ok(Some(&buffer[..len]));
            }
            if byte != b'\r' {
                if len == buffer.len() {
                    return Err("cli-line-too-long");
                }
                buffer[len] = byte;
                len += 1;
            }
        } else {
            spins += 1;
            unsafe { asm!("pause", options(nomem, nostack, preserves_flags)) };
        }
    }
    Ok(None)
}

fn write_byte(byte: u8) {
    while inb(SERIAL_COM1 + 5) & 0x20 == 0 {}
    outb(SERIAL_COM1, byte);
}

fn try_read_byte() -> Option<u8> {
    if inb(SERIAL_COM1 + 5) & 0x01 == 0 {
        None
    } else {
        Some(inb(SERIAL_COM1))
    }
}
