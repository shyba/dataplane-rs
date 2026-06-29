use super::frame;
use super::parsing::{self, ipv4_checksum, read_ipv4};
use crate::layout::{
    DHCP_CLIENT_PORT, DHCP_SERVER_PORT, EP_DHCP, REQUEST_DHCP_NET, TASK_NET, VM_MAC,
};
use crate::layout::{ETHER_TYPE_IPV4, IPV4_HEADER_BYTES, IPV4_PROTOCOL_UDP};
use crate::net_protocol::{
    read_be_u16, read_be_u32, read_mac, write_be_u16, write_be_u32, write_ipv4_header,
};
use dataplane_microkernel_core::{Message, MessageBody};

pub(crate) const TCP_STREAM_RX_BYTES: usize = NET_REPLY_PAYLOAD_BYTES - IPV4_HEADER_BYTES - 20;
pub(crate) const TCP_STREAM_SEGMENT_BYTES: usize = NET_REPLY_PAYLOAD_BYTES - IPV4_HEADER_BYTES - 20;
pub(crate) const TCP_STREAM_TX_BYTES: usize = 1024;
pub(crate) const TCP_STREAM_SESSIONS: usize = 4;
pub(crate) const TCP_CONTROL_OVERFLOW_BYTES: usize = TCP_STREAM_RX_BYTES + 1;
pub(crate) const TCP_STREAM_TIMEOUT_TICKS: u32 = 8;
pub(crate) const TCP_FLAG_FIN: u8 = 0x01;
pub(crate) const TCP_FLAG_SYN: u8 = 0x02;
pub(crate) const TCP_FLAG_RST: u8 = 0x04;
pub(crate) const TCP_FLAG_PSH: u8 = 0x08;
pub(crate) const TCP_FLAG_ACK: u8 = 0x10;
pub(crate) const UDP_FAIRNESS_PACKETS: u32 = 16;
pub(crate) const STAGE_G_ROUNDS: u32 = 8;
pub(crate) const UDP_ECHO_PAYLOAD_BYTES: usize = 16;
#[allow(dead_code)]
const IPV4_UDP_REPLY_BYTES: usize = IPV4_HEADER_BYTES + 8 + UDP_ECHO_PAYLOAD_BYTES;
pub(crate) const NET_REPLY_PAYLOAD_BYTES: usize = 512;
const DHCP_PACKET_BYTES: usize = 300;
const DHCP_FIXED_BYTES: usize = 240;
const DHCP_OPTIONS_OFFSET: usize = 236;
const DHCP_XID: u32 = 0x4450_4843;

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum NetworkPacketKind {
    ArpRequest,
    Ipv4Icmp,
    Ipv4Tcp,
    Ipv4Udp,
}

#[derive(Clone, Copy)]
pub(crate) enum NetworkReplyKind {
    Arp,
    Icmp,
    Udp { seq: u32 },
    DhcpDiscover,
    DhcpRequest,
    TcpSynAck,
    TcpAck,
    TcpFinAck,
    HttpGet,
    HttpHead,
    Http404,
    Http405,
    Http413,
    Http500,
    HttpBackpressure,
}

pub(crate) struct NetworkReply {
    pub(crate) dst: [u8; 6],
    pub(crate) src: [u8; 6],
    pub(crate) ethertype: u16,
    pub(crate) payload: [u8; NET_REPLY_PAYLOAD_BYTES],
    pub(crate) payload_len: usize,
    pub(crate) kind: NetworkReplyKind,
}

impl NetworkReply {
    pub(crate) fn as_frame(&self) -> frame::EthernetFrameSpec<'_> {
        frame::EthernetFrameSpec {
            dst: self.dst,
            src: self.src,
            ethertype: self.ethertype,
            payload: &self.payload[..self.payload_len],
            pad: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) enum DhcpEvent {
    Offer { address: [u8; 4], server: [u8; 4] },
    Ack { address: [u8; 4], server: [u8; 4] },
}

pub(crate) struct DhcpTask {
    xid: u32,
    offered_ipv4: Option<[u8; 4]>,
    server_ipv4: Option<[u8; 4]>,
    assigned_ipv4: Option<[u8; 4]>,
}

impl DhcpTask {
    pub(crate) const fn new() -> Self {
        Self {
            xid: DHCP_XID,
            offered_ipv4: None,
            server_ipv4: None,
            assigned_ipv4: None,
        }
    }
    pub(crate) fn discover(&self) -> NetworkReply {
        self.build_client_message(1, None, None, NetworkReplyKind::DhcpDiscover)
    }
    pub(crate) fn request(&self, requested_ipv4: [u8; 4], server_ipv4: [u8; 4]) -> NetworkReply {
        self.build_client_message(
            3,
            Some(requested_ipv4),
            Some(server_ipv4),
            NetworkReplyKind::DhcpRequest,
        )
    }
    pub(crate) fn accept_server_frame(
        &mut self,
        message: Message,
        frame: &[u8],
    ) -> Result<Option<DhcpEvent>, &'static str> {
        if message.from != TASK_NET || message.to != EP_DHCP || message.request != REQUEST_DHCP_NET
        {
            return Err("dhcp-net-route");
        }
        let len = match message.body {
            MessageBody::Pair(len, 0) => len as usize,
            _ => return Err("dhcp-net-body"),
        };
        if len != frame.len() {
            return Err("dhcp-frame-body-len");
        }
        let Some(frame) = parsing::ethernet_frame_view(frame) else {
            return Ok(None);
        };
        if read_be_u16(frame, 12) != ETHER_TYPE_IPV4 {
            return Ok(None);
        }
        let ip = crate::layout::ETHERNET_HEADER_BYTES;
        if frame.len() < ip + IPV4_HEADER_BYTES {
            return Ok(None);
        }
        let version = frame[ip] >> 4;
        let header_len = usize::from(frame[ip & 0]) * 4;
        if version != 4 || header_len < IPV4_HEADER_BYTES {
            return Ok(None);
        }
        let total_len = usize::from(read_be_u16(frame, ip + 2));
        if total_len < header_len + 8
            || frame.len() < crate::layout::ETHERNET_HEADER_BYTES + total_len
        {
            return Ok(None);
        }
        if frame[ip + 9] != IPV4_PROTOCOL_UDP {
            return Ok(None);
        }
        if ipv4_checksum(&frame[ip..ip + header_len]) != 0 {
            return Ok(None);
        }
        let udp = ip + header_len;
        let udp_len = usize::from(read_be_u16(frame, udp + 4));
        if udp_len < 8 + DHCP_FIXED_BYTES || header_len + udp_len > total_len {
            return Ok(None);
        }
        if read_be_u16(frame, udp) != DHCP_SERVER_PORT
            || read_be_u16(frame, udp + 2) != DHCP_CLIENT_PORT
        {
            return Ok(None);
        }
        let dhcp = udp + 8;
        if frame[dhcp] != 2
            || frame[dhcp + 1] != 1
            || frame[dhcp + 2] != 6
            || read_be_u32(frame, dhcp + 4) != self.xid
            || read_mac(frame, dhcp + 28) != VM_MAC
        {
            return Ok(None);
        }
        let yiaddr = read_ipv4(frame, dhcp + 16);
        if yiaddr == [0; 4] {
            return Ok(None);
        }
        if frame[dhcp + DHCP_OPTIONS_OFFSET..dhcp + DHCP_OPTIONS_OFFSET + 4] != [99, 130, 83, 99] {
            return Ok(None);
        }
        let mut message_type = 0u8;
        let mut server = read_ipv4(frame, ip + 12);
        let mut option = dhcp + DHCP_OPTIONS_OFFSET + 4;
        let end = dhcp + udp_len - 8;
        while option < end {
            let code = frame[option];
            option += 1;
            if code == 0 {
                continue;
            }
            if code == 255 {
                break;
            }
            if option >= end {
                return Ok(None);
            }
            let len = usize::from(frame[option]);
            option += 1;
            if option + len > end {
                return Ok(None);
            }
            if code == 53 && len == 1 {
                message_type = frame[option];
            } else if code == 54 && len == 4 {
                server = read_ipv4(frame, option);
            }
            option += len;
        }
        match message_type {
            2 => {
                self.offered_ipv4 = Some(yiaddr);
                self.server_ipv4 = Some(server);
                Ok(Some(DhcpEvent::Offer {
                    address: yiaddr,
                    server,
                }))
            }
            5 => Ok(Some(DhcpEvent::Ack {
                address: yiaddr,
                server,
            })),
            _ => Ok(None),
        }
    }
    pub(crate) fn assign(&mut self, address: [u8; 4], server: [u8; 4]) -> Result<(), &'static str> {
        if self.offered_ipv4 != Some(address) || self.server_ipv4 != Some(server) {
            return Err("dhcp-ack-mismatch");
        }
        self.assigned_ipv4 = Some(address);
        if self.assigned_ipv4 != Some(address) {
            return Err("dhcp-assign");
        }
        Ok(())
    }
    fn build_client_message(
        &self,
        message_type: u8,
        requested_ipv4: Option<[u8; 4]>,
        server_ipv4: Option<[u8; 4]>,
        kind: NetworkReplyKind,
    ) -> NetworkReply {
        let mut payload = [0u8; NET_REPLY_PAYLOAD_BYTES];
        write_ipv4_header(
            &mut payload[..IPV4_HEADER_BYTES],
            IPV4_PROTOCOL_UDP,
            8 + DHCP_PACKET_BYTES,
            (self.xid & 0xffff) as u16,
            [0, 0, 0, 0],
            [255, 255, 255, 255],
        );
        let udp = IPV4_HEADER_BYTES;
        write_be_u16(&mut payload[udp..udp + 2], DHCP_CLIENT_PORT);
        write_be_u16(&mut payload[udp + 2..udp + 4], DHCP_SERVER_PORT);
        write_be_u16(
            &mut payload[udp + 4..udp + 6],
            (8 + DHCP_PACKET_BYTES) as u16,
        );
        write_be_u16(&mut payload[udp + 6..udp + 8], 0);
        let dhcp = udp + 8;
        payload[dhcp] = 1;
        payload[dhcp + 1] = 1;
        payload[dhcp + 2] = 6;
        write_be_u32(&mut payload[dhcp + 4..dhcp + 8], self.xid);
        write_be_u16(&mut payload[dhcp + 10..dhcp + 12], 0x8000);
        payload[dhcp + 28..dhcp + 34].copy_from_slice(&VM_MAC);
        payload[dhcp + DHCP_OPTIONS_OFFSET..dhcp + DHCP_OPTIONS_OFFSET + 4]
            .copy_from_slice(&[99, 130, 83, 99]);
        let mut option = dhcp + DHCP_OPTIONS_OFFSET + 4;
        parsing::write_dhcp_option(&mut payload, &mut option, 53, &[message_type]);
        parsing::write_dhcp_option(
            &mut payload,
            &mut option,
            61,
            &[
                1, VM_MAC[0], VM_MAC[1], VM_MAC[2], VM_MAC[3], VM_MAC[4], VM_MAC[5],
            ],
        );
        if let Some(address) = requested_ipv4 {
            parsing::write_dhcp_option(&mut payload, &mut option, 50, &address);
        }
        if let Some(server) = server_ipv4 {
            parsing::write_dhcp_option(&mut payload, &mut option, 54, &server);
        }
        parsing::write_dhcp_option(&mut payload, &mut option, 55, &[1, 3, 6, 15]);
        payload[option] = 255;
        NetworkReply {
            dst: [0xff; 6],
            src: VM_MAC,
            ethertype: ETHER_TYPE_IPV4,
            payload,
            payload_len: IPV4_HEADER_BYTES + 8 + DHCP_PACKET_BYTES,
            kind,
        }
    }
}
