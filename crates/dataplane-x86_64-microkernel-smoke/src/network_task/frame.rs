use crate::layout::{
    ETHERNET_HEADER_BYTES, ETHERNET_MIN_FRAME_BYTES, ETHER_TYPE_ARP, ETHER_TYPE_IPV4,
    NET_FRAME_OFFSET, NET_TASK_BYTES, VM_MAC,
};
use dataplane_microkernel_core::{
    MessageBody, NetworkFrameBufferId, NetworkFrameDescriptor, NetworkFrameDirection,
    NetworkFrameType,
};

#[derive(Clone, Copy)]
pub(crate) struct EthernetFrameSpec<'a> {
    pub(crate) dst: [u8; 6],
    pub(crate) src: [u8; 6],
    pub(crate) ethertype: u16,
    pub(crate) payload: &'a [u8],
    pub(crate) pad: u8,
}

pub(crate) fn net_tx_probe_frame() -> EthernetFrameSpec<'static> {
    EthernetFrameSpec {
        dst: [0xff; 6],
        src: VM_MAC,
        ethertype: 0x88b7,
        payload: b"DPMK-NET-TX-PROBE",
        pad: 0,
    }
}
pub(crate) fn build_unsupported_ethertype_frame() -> EthernetFrameSpec<'static> {
    EthernetFrameSpec {
        dst: [0xff; 6],
        src: [0x02, 0x00, 0x00, 0x00, 0x00, 0x03],
        ethertype: 0x88b8,
        payload: b"DPMK-NET-UNSUPPORTED-ETHERTYPE",
        pad: 0,
    }
}
pub(crate) fn build_arp_malformed_frame() -> EthernetFrameSpec<'static> {
    EthernetFrameSpec {
        dst: [0xff; 6],
        src: [0x02, 0x00, 0x00, 0x00, 0x00, 0x04],
        ethertype: ETHER_TYPE_ARP,
        payload: b"DPMK-NET-ARP-MALFORMED",
        pad: 0,
    }
}
pub(crate) fn build_icmp_malformed_frame() -> EthernetFrameSpec<'static> {
    EthernetFrameSpec {
        dst: [0xff; 6],
        src: [0x02, 0x00, 0x00, 0x00, 0x00, 0x05],
        ethertype: ETHER_TYPE_IPV4,
        payload: b"DPMK-NET-ICMP-MALFORMED",
        pad: 0,
    }
}
pub(crate) fn ethernet_frame_len(payload_len: usize) -> usize {
    let frame_len = ETHERNET_HEADER_BYTES + payload_len;
    if frame_len < ETHERNET_MIN_FRAME_BYTES {
        ETHERNET_MIN_FRAME_BYTES
    } else {
        frame_len
    }
}
pub(crate) fn net_frame_message_body(
    direction: NetworkFrameDirection,
    buffer: NetworkFrameBufferId,
    len: u32,
) -> Result<MessageBody, &'static str> {
    NetworkFrameDescriptor::new(
        direction,
        NetworkFrameType::Ethernet,
        buffer,
        len as usize,
        NET_TASK_BYTES - NET_FRAME_OFFSET,
    )
    .map_err(|_| "net-frame-descriptor")?;
    Ok(MessageBody::Pair(len, 0))
}
