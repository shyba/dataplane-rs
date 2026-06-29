use crate::serial;

pub(crate) fn find_byte(bytes: &[u8], target: u8) -> Option<usize> {
    let mut index = 0;
    while index < bytes.len() {
        if bytes[index] == target {
            return Some(index);
        }
        index += 1;
    }
    None
}

pub(crate) fn append_bytes(
    out: &mut [u8],
    len: &mut usize,
    bytes: &[u8],
) -> Result<(), &'static str> {
    if *len + bytes.len() > out.len() {
        return Err("append-capacity");
    }
    let mut index = 0;
    while index < bytes.len() {
        out[*len + index] = bytes[index];
        index += 1;
    }
    *len += bytes.len();
    Ok(())
}

pub(crate) fn append_decimal_bytes(
    out: &mut [u8],
    len: &mut usize,
    mut value: u32,
) -> Result<(), &'static str> {
    let mut digits = [0u8; 10];
    let mut digit_len = 0;
    if value == 0 {
        return append_bytes(out, len, b"0");
    }
    while value != 0 {
        digits[digit_len] = b'0' + (value % 10) as u8;
        value /= 10;
        digit_len += 1;
    }
    while digit_len != 0 {
        digit_len -= 1;
        append_bytes(out, len, &digits[digit_len..digit_len + 1])?;
    }
    Ok(())
}

pub(crate) fn serial_write_ipv4(address: [u8; 4]) {
    serial::write_decimal(u32::from(address[0]));
    serial::write_str(".");
    serial::write_decimal(u32::from(address[1]));
    serial::write_str(".");
    serial::write_decimal(u32::from(address[2]));
    serial::write_str(".");
    serial::write_decimal(u32::from(address[3]));
}
