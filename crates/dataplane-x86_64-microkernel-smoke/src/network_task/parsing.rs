use super::super::layout::{
    ETHERNET_HEADER_BYTES, ETHER_TYPE_ARP, ETHER_TYPE_IPV4, IPV4_PROTOCOL_TCP,
};

pub(crate) fn read_ipv4(frame: &[u8], offset: usize) -> [u8; 4] {
    [
        frame[offset],
        frame[offset + 1],
        frame[offset + 2],
        frame[offset + 3],
    ]
}

pub(crate) fn write_dhcp_option(payload: &mut [u8], option: &mut usize, code: u8, data: &[u8]) {
    payload[*option] = code;
    *option += 1;
    payload[*option] = data.len() as u8;
    *option += 1;
    let mut index = 0;
    while index < data.len() {
        payload[*option + index] = data[index];
        index += 1;
    }
    *option += data.len();
}

pub(crate) fn ethernet_frame_view(frame: &[u8]) -> Option<&[u8]> {
    if frame.len() >= ETHERNET_HEADER_BYTES {
        return Some(frame);
    }
    if frame.len() >= ETHERNET_HEADER_BYTES + 2
        && frame[0] == 0
        && frame[1] == 0
        && matches!(
            crate::net_protocol::read_be_u16(frame, 14),
            ETHER_TYPE_ARP | ETHER_TYPE_IPV4
        )
    {
        return Some(&frame[2..]);
    }
    None
}

pub(crate) fn checksum_add_bytes(sum: u32, bytes: &[u8]) -> u32 {
    crate::net_protocol::checksum_add_bytes(sum, bytes)
}
pub(crate) fn checksum_finish(sum: u32) -> u16 {
    crate::net_protocol::checksum_finish(sum)
}
pub(crate) fn internet_checksum(data: &[u8]) -> u16 {
    checksum_finish(checksum_add_bytes(0, data))
}
pub(crate) fn ipv4_checksum(header: &[u8]) -> u16 {
    internet_checksum(header)
}
pub(crate) fn tcp_checksum(src: [u8; 4], dst: [u8; 4], segment: &[u8]) -> u16 {
    let mut sum = 0u32;
    sum = checksum_add_bytes(sum, &src);
    sum = checksum_add_bytes(sum, &dst);
    sum = checksum_add_bytes(sum, &[0, IPV4_PROTOCOL_TCP]);
    sum = checksum_add_bytes(sum, &[(segment.len() >> 8) as u8, segment.len() as u8]);
    sum = checksum_add_bytes(sum, segment);
    checksum_finish(sum)
}
