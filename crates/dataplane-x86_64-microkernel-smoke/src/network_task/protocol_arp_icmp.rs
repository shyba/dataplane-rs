use super::config::{NetworkPacketKind, NetworkReply, NetworkReplyKind, NET_REPLY_PAYLOAD_BYTES};
use super::counters::Ipv4Route;
use super::parsing::{self, ipv4_checksum, read_ipv4};
use super::task::TcpIpTask;
use crate::layout::{
    ARP_FRAME_BYTES, ETHERNET_HEADER_BYTES, ETHER_TYPE_ARP, ETHER_TYPE_IPV4, IPV4_HEADER_BYTES,
    IPV4_PROTOCOL_ICMP, VM_IPV4, VM_MAC,
};
use crate::net_protocol::{read_be_u16, read_mac, write_be_u16, write_ipv4_header};

impl TcpIpTask {
    pub(crate) fn classify_arp(&mut self, frame: &[u8]) -> Result<NetworkPacketKind, &'static str> {
        if frame.len() < ARP_FRAME_BYTES {
            self.network_drop_counters.arp_malformed += 1;
            return Err("tcpip-arp-short");
        }
        if read_be_u16(frame, 14) != 1 || read_be_u16(frame, 16) != ETHER_TYPE_IPV4 {
            self.network_drop_counters.arp_malformed += 1;
            return Err("tcpip-arp-hw");
        }
        if frame[18] != 6 || frame[19] != 4 {
            self.network_drop_counters.arp_malformed += 1;
            return Err("tcpip-arp-len");
        }
        if read_be_u16(frame, 20) != 1 {
            self.network_drop_counters.arp_malformed += 1;
            return Err("tcpip-arp-op");
        }
        Ok(NetworkPacketKind::ArpRequest)
    }

    pub(crate) fn prepare_arp_reply(
        &mut self,
        frame: &[u8],
    ) -> Result<Option<NetworkReply>, &'static str> {
        self.classify_arp(frame)?;
        let target_ipv4 = read_ipv4(frame, 38);
        if !self.accepts_ipv4(target_ipv4) {
            self.network_drop_counters.arp_wrong_target += 1;
            return Ok(None);
        }
        let peer_mac = self.learn_arp_peer(frame)?;
        let mut payload = [0; NET_REPLY_PAYLOAD_BYTES];
        payload[0..2].copy_from_slice(&[0x00, 0x01]);
        payload[2..4].copy_from_slice(&[0x08, 0x00]);
        payload[4] = 6;
        payload[5] = 4;
        payload[6..8].copy_from_slice(&[0x00, 0x02]);
        payload[8..14].copy_from_slice(&VM_MAC);
        payload[14..18].copy_from_slice(&target_ipv4);
        payload[18..24].copy_from_slice(&peer_mac);
        payload[24..28].copy_from_slice(&frame[28..32]);
        Ok(Some(NetworkReply {
            dst: read_mac(frame, 6),
            src: VM_MAC,
            ethertype: ETHER_TYPE_ARP,
            payload,
            payload_len: 28,
            kind: NetworkReplyKind::Arp,
        }))
    }

    pub(crate) fn classify_ipv4(
        &mut self,
        frame: &[u8],
    ) -> Result<NetworkPacketKind, &'static str> {
        if frame.len() < ETHERNET_HEADER_BYTES + IPV4_HEADER_BYTES {
            self.network_drop_counters.icmp_malformed += 1;
            return Err("tcpip-ipv4-short");
        }
        let ip = ETHERNET_HEADER_BYTES;
        let version = frame[ip] >> 4;
        let header_len = usize::from(frame[ip] & 0x0f) * 4;
        if version != 4 || header_len < IPV4_HEADER_BYTES {
            self.network_drop_counters.icmp_malformed += 1;
            return Err("tcpip-ipv4-header");
        }
        let total_len = usize::from(read_be_u16(frame, ip + 2));
        if total_len < header_len || frame.len() < ETHERNET_HEADER_BYTES + total_len {
            self.network_drop_counters.icmp_malformed += 1;
            return Err("tcpip-ipv4-len");
        }
        match frame[ip + 9] {
            IPV4_PROTOCOL_ICMP => {
                if total_len < header_len + 8 {
                    self.network_drop_counters.icmp_malformed += 1;
                    return Err("tcpip-icmp-short");
                }
                Ok(NetworkPacketKind::Ipv4Icmp)
            }
            crate::layout::IPV4_PROTOCOL_TCP => {
                let tcp = ip + header_len;
                if total_len < header_len + 20 {
                    return Err("tcpip-tcp-short");
                }
                let tcp_header_len = usize::from(frame[tcp + 12] >> 4) * 4;
                if tcp_header_len < 20 || header_len + tcp_header_len > total_len {
                    return Err("tcpip-tcp-header");
                }
                Ok(NetworkPacketKind::Ipv4Tcp)
            }
            crate::layout::IPV4_PROTOCOL_UDP => {
                let udp = ip + header_len;
                if total_len < header_len + 8 {
                    self.network_drop_counters.udp_malformed += 1;
                    return Err("tcpip-udp-short");
                }
                let udp_len = usize::from(read_be_u16(frame, udp + 4));
                if udp_len < 8 || header_len + udp_len > total_len {
                    self.network_drop_counters.udp_malformed += 1;
                    return Err("tcpip-udp-len");
                }
                Ok(NetworkPacketKind::Ipv4Udp)
            }
            _ => {
                self.network_drop_counters.unsupported_ipv4_protocol += 1;
                Err("tcpip-ipv4-protocol")
            }
        }
    }

    pub(crate) fn prepare_ipv4_reply(
        &mut self,
        frame: &[u8],
        http_task: &mut crate::http::HttpTask,
        http_files: &crate::http::HttpFileSet<'_>,
    ) -> Result<Option<NetworkReply>, &'static str> {
        if frame.len() < ETHERNET_HEADER_BYTES + IPV4_HEADER_BYTES {
            return Ok(None);
        }
        if frame[0..6] != VM_MAC && frame[0..6] != [0xff; 6] {
            return Ok(None);
        }
        let ip = ETHERNET_HEADER_BYTES;
        let version = frame[ip] >> 4;
        let header_len = usize::from(frame[ip] & 0x0f) * 4;
        if version != 4 || header_len != IPV4_HEADER_BYTES {
            return Ok(None);
        }
        let total_len = usize::from(read_be_u16(frame, ip + 2));
        if total_len < header_len || frame.len() < ETHERNET_HEADER_BYTES + total_len {
            return Ok(None);
        }
        let local_ipv4 = read_ipv4(frame, ip + 16);
        if !self.accepts_ipv4(local_ipv4) {
            self.network_drop_counters.ipv4_wrong_target += 1;
            return Ok(None);
        }
        if ipv4_checksum(&frame[ip..ip + IPV4_HEADER_BYTES]) != 0 {
            return Ok(None);
        }
        self.learn_ipv4_peer(frame)?;
        let route = Ipv4Route {
            local_ipv4,
            peer_ipv4: read_ipv4(frame, ip + 12),
        };
        match frame[ip + 9] {
            IPV4_PROTOCOL_ICMP => self
                .prepare_icmp_reply(frame, ip, total_len, route)
                .map(Some),
            crate::layout::IPV4_PROTOCOL_TCP => {
                self.prepare_tcp_reply(frame, ip, total_len, http_task, http_files, route)
            }
            crate::layout::IPV4_PROTOCOL_UDP => self.prepare_udp_reply(frame, ip, total_len, route),
            _ => Ok(None),
        }
    }

    fn accepts_ipv4(&self, ipv4: [u8; 4]) -> bool {
        ipv4 == VM_IPV4 || self.dynamic_ipv4 == Some(ipv4)
    }

    fn learn_arp_peer(&mut self, frame: &[u8]) -> Result<[u8; 6], &'static str> {
        let eth_src = read_mac(frame, 6);
        let arp_src = read_mac(frame, 22);
        if eth_src != arp_src {
            return Err("tcpip-arp-peer-mac");
        }
        self.learn_peer_mac(eth_src)?;
        Ok(eth_src)
    }

    fn learn_ipv4_peer(&mut self, frame: &[u8]) -> Result<[u8; 6], &'static str> {
        let eth_src = read_mac(frame, 6);
        self.learn_peer_mac(eth_src)?;
        Ok(eth_src)
    }

    fn learn_peer_mac(&mut self, mac: [u8; 6]) -> Result<(), &'static str> {
        if mac == [0; 6] || mac == [0xff; 6] || mac == VM_MAC {
            return Err("tcpip-peer-mac");
        }
        match self.peer_mac {
            Some(peer_mac) if peer_mac != mac => Err("tcpip-peer-mac-change"),
            Some(_) => Ok(()),
            None => {
                self.peer_mac = Some(mac);
                Ok(())
            }
        }
    }

    fn prepare_icmp_reply(
        &mut self,
        frame: &[u8],
        ip: usize,
        total_len: usize,
        route: Ipv4Route,
    ) -> Result<NetworkReply, &'static str> {
        let icmp = ip + IPV4_HEADER_BYTES;
        let icmp_len = total_len - IPV4_HEADER_BYTES;
        if !(8..=NET_REPLY_PAYLOAD_BYTES - IPV4_HEADER_BYTES).contains(&icmp_len) {
            return Err("tcpip-icmp-reply-len");
        }
        if frame[icmp] != 8 {
            self.network_drop_counters.icmp_non_echo += 1;
            return Err("tcpip-icmp-type");
        }
        let mut payload = [0; NET_REPLY_PAYLOAD_BYTES];
        write_ipv4_header(
            &mut payload[..IPV4_HEADER_BYTES],
            IPV4_PROTOCOL_ICMP,
            icmp_len,
            read_be_u16(frame, ip + 4),
            route.local_ipv4,
            route.peer_ipv4,
        );
        payload[IPV4_HEADER_BYTES..IPV4_HEADER_BYTES + icmp_len]
            .copy_from_slice(&frame[icmp..icmp + icmp_len]);
        let reply_icmp = IPV4_HEADER_BYTES;
        payload[reply_icmp] = 0;
        payload[reply_icmp + 2] = 0;
        payload[reply_icmp + 3] = 0;
        let checksum = parsing::internet_checksum(&payload[reply_icmp..reply_icmp + icmp_len]);
        write_be_u16(&mut payload[reply_icmp + 2..reply_icmp + 4], checksum);
        Ok(NetworkReply {
            dst: read_mac(frame, 6),
            src: VM_MAC,
            ethertype: ETHER_TYPE_IPV4,
            payload,
            payload_len: IPV4_HEADER_BYTES + icmp_len,
            kind: NetworkReplyKind::Icmp,
        })
    }
}
