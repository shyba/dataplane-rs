use crate::io::{inb, outb};
use crate::SERIAL_COM1;

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
    for byte in s.bytes() {
        write_byte(byte)
    }
}

#[cfg(feature = "shard-bench")]
pub fn write_u64(mut value: u64) {
    let mut buf = [0u8; 20];
    let mut index = buf.len();
    if value == 0 {
        write_byte(b'0');
        return;
    }
    while value != 0 {
        index -= 1;
        buf[index] = b'0' + (value % 10) as u8;
        value /= 10;
    }
    for byte in &buf[index..] {
        write_byte(*byte);
    }
}

fn write_byte(byte: u8) {
    while inb(SERIAL_COM1 + 5) & 0x20 == 0 {}
    outb(SERIAL_COM1, byte);
}
