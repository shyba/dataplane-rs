#[derive(Clone, Copy)]
pub(crate) struct NetworkDropCounters {
    pub(crate) arp_malformed: u32,
    pub(crate) arp_wrong_target: u32,
    pub(crate) icmp_malformed: u32,
    pub(crate) icmp_non_echo: u32,
    pub(crate) udp_malformed: u32,
    pub(crate) udp_wrong_port: u32,
    pub(crate) udp_bad_payload: u32,
    pub(crate) ipv4_wrong_target: u32,
    pub(crate) unsupported_ipv4_protocol: u32,
    pub(crate) unsupported_ethertype: u32,
}

impl NetworkDropCounters {
    pub(crate) const fn new() -> Self {
        Self {
            arp_malformed: 0,
            arp_wrong_target: 0,
            icmp_malformed: 0,
            icmp_non_echo: 0,
            udp_malformed: 0,
            udp_wrong_port: 0,
            udp_bad_payload: 0,
            ipv4_wrong_target: 0,
            unsupported_ipv4_protocol: 0,
            unsupported_ethertype: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct Ipv4Route {
    pub(crate) local_ipv4: [u8; 4],
    pub(crate) peer_ipv4: [u8; 4],
}

#[derive(Clone, Copy)]
pub(crate) struct TcpQueuedSegment {
    pub(crate) dst_port: u16,
    pub(crate) seq: u32,
    pub(crate) ack: u32,
    pub(crate) flags: u8,
    pub(crate) offset: usize,
    pub(crate) len: usize,
    pub(crate) kind: crate::NetworkReplyKind,
}
