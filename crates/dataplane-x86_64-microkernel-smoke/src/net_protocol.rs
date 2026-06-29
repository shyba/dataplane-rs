use crate::layout::IPV4_HEADER_BYTES;

pub(crate) fn read_be_u16(region: &[u8], offset: usize) -> u16 {
    (u16::from(region[offset]) << 8) | u16::from(region[offset + 1])
}

pub(crate) fn read_be_u32(region: &[u8], offset: usize) -> u32 {
    (u32::from(region[offset]) << 24)
        | (u32::from(region[offset + 1]) << 16)
        | (u32::from(region[offset + 2]) << 8)
        | u32::from(region[offset + 3])
}

pub(crate) fn write_be_u16(dst: &mut [u8], value: u16) {
    dst[0] = (value >> 8) as u8;
    dst[1] = value as u8;
}

pub(crate) fn write_be_u32(dst: &mut [u8], value: u32) {
    dst[0] = (value >> 24) as u8;
    dst[1] = (value >> 16) as u8;
    dst[2] = (value >> 8) as u8;
    dst[3] = value as u8;
}

pub(crate) fn read_mac(frame: &[u8], offset: usize) -> [u8; 6] {
    [
        frame[offset],
        frame[offset + 1],
        frame[offset + 2],
        frame[offset + 3],
        frame[offset + 4],
        frame[offset + 5],
    ]
}

pub(crate) fn write_ipv4_header(
    header: &mut [u8],
    protocol: u8,
    payload_len: usize,
    ident: u16,
    src: [u8; 4],
    dst: [u8; 4],
) {
    for byte in header.iter_mut() {
        *byte = 0;
    }
    header[0] = 0x45;
    write_be_u16(&mut header[2..4], (IPV4_HEADER_BYTES + payload_len) as u16);
    write_be_u16(&mut header[4..6], ident);
    write_be_u16(&mut header[6..8], 0x4000);
    header[8] = 64;
    header[9] = protocol;
    header[12..16].copy_from_slice(&src);
    header[16..20].copy_from_slice(&dst);
    let checksum = internet_checksum(header);
    write_be_u16(&mut header[10..12], checksum);
}

pub(crate) fn checksum_add_bytes(mut sum: u32, data: &[u8]) -> u32 {
    let mut index = 0;
    while index + 1 < data.len() {
        sum += u32::from((u16::from(data[index]) << 8) | u16::from(data[index + 1]));
        index += 2;
    }
    if index < data.len() {
        sum += u32::from(data[index]) << 8;
    }
    sum
}

pub(crate) fn checksum_finish(mut sum: u32) -> u16 {
    while (sum >> 16) != 0 {
        sum = (sum & 0xffff) + (sum >> 16);
    }
    !(sum as u16)
}

fn internet_checksum(data: &[u8]) -> u16 {
    checksum_finish(checksum_add_bytes(0, data))
}
