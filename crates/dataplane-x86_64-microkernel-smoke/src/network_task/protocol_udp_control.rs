use super::config::NET_REPLY_PAYLOAD_BYTES;
use super::counters::Ipv4Route;
use super::task::TcpIpTask;
use crate::control_protocol::{
    reject_control_protocol_v1, ControlProtocolV1RejectReason,
    CONTROL_PROTOCOL_V1_OPCODE_STATUS_ECHO, CONTROL_PROTOCOL_V1_ROUTE, CONTROL_PROTOCOL_V1_VERSION,
};
use crate::layout::{
    ETHER_TYPE_IPV4, HOST_UDP_PORT, IPV4_HEADER_BYTES, IPV4_PROTOCOL_UDP, VM_MAC, VM_UDP_PORT,
};
use crate::net_protocol::{
    read_be_u16, read_be_u32, read_mac, write_be_u16, write_be_u32, write_ipv4_header,
};

impl TcpIpTask {
    pub(crate) fn prepare_udp_reply(
        &mut self,
        frame: &[u8],
        ip: usize,
        total_len: usize,
        route: Ipv4Route,
    ) -> Result<Option<super::config::NetworkReply>, &'static str> {
        let protocol = CONTROL_PROTOCOL_V1_ROUTE;
        if protocol.version != CONTROL_PROTOCOL_V1_VERSION
            || protocol.opcode != CONTROL_PROTOCOL_V1_OPCODE_STATUS_ECHO
        {
            return Err("control-protocol-v1-route");
        }
        let udp = ip + IPV4_HEADER_BYTES;
        if total_len < IPV4_HEADER_BYTES + 8 {
            reject_control_protocol_v1(ControlProtocolV1RejectReason::ShortPayload);
            return Ok(None);
        }
        let udp_len = usize::from(read_be_u16(frame, udp + 4));
        let src_port = read_be_u16(frame, udp);
        let dst_port = read_be_u16(frame, udp + 2);
        if src_port != HOST_UDP_PORT {
            self.network_drop_counters.udp_wrong_port += 1;
            reject_control_protocol_v1(ControlProtocolV1RejectReason::WrongSrcPort);
            return Ok(None);
        }
        if dst_port != VM_UDP_PORT {
            self.network_drop_counters.udp_wrong_port += 1;
            reject_control_protocol_v1(ControlProtocolV1RejectReason::WrongRoute);
            return Ok(None);
        }
        let expected_udp_len = 8 + protocol.max_payload_bytes;
        if udp_len < expected_udp_len {
            self.network_drop_counters.udp_bad_payload += 1;
            reject_control_protocol_v1(ControlProtocolV1RejectReason::ShortPayload);
            return Ok(None);
        }
        if udp_len > expected_udp_len {
            self.network_drop_counters.udp_bad_payload += 1;
            reject_control_protocol_v1(ControlProtocolV1RejectReason::OverlongPayload);
            return Ok(None);
        }
        if IPV4_HEADER_BYTES + udp_len > total_len {
            self.network_drop_counters.udp_malformed += 1;
            reject_control_protocol_v1(ControlProtocolV1RejectReason::ShortPayload);
            return Ok(None);
        }
        let udp_payload = udp + 8;
        if frame[udp_payload..udp_payload + protocol.request_magic.len()] != *protocol.request_magic
        {
            self.network_drop_counters.udp_bad_payload += 1;
            reject_control_protocol_v1(ControlProtocolV1RejectReason::BadMagic);
            return Ok(None);
        }
        let seq = read_be_u32(frame, udp_payload + 8);
        let check = read_be_u32(frame, udp_payload + 12);
        if check != (seq ^ 0xa5a5_a5a5) {
            self.network_drop_counters.udp_bad_payload += 1;
            reject_control_protocol_v1(ControlProtocolV1RejectReason::BadCheck);
            return Ok(None);
        }
        let mut payload = [0; NET_REPLY_PAYLOAD_BYTES];
        write_ipv4_header(
            &mut payload[..IPV4_HEADER_BYTES],
            IPV4_PROTOCOL_UDP,
            8 + protocol.max_payload_bytes,
            seq as u16,
            route.local_ipv4,
            route.peer_ipv4,
        );
        let reply_udp = IPV4_HEADER_BYTES;
        write_be_u16(&mut payload[reply_udp..reply_udp + 2], VM_UDP_PORT);
        write_be_u16(&mut payload[reply_udp + 2..reply_udp + 4], HOST_UDP_PORT);
        write_be_u16(
            &mut payload[reply_udp + 4..reply_udp + 6],
            (8 + protocol.max_payload_bytes) as u16,
        );
        write_be_u16(&mut payload[reply_udp + 6..reply_udp + 8], 0);
        let reply_payload = reply_udp + 8;
        payload[reply_payload..reply_payload + protocol.response_magic.len()]
            .copy_from_slice(protocol.response_magic);
        write_be_u32(&mut payload[reply_payload + 8..reply_payload + 12], seq);
        write_be_u32(
            &mut payload[reply_payload + 12..reply_payload + 16],
            seq ^ 0x5a5a_5a5a,
        );
        Ok(Some(super::config::NetworkReply {
            dst: read_mac(frame, 6),
            src: VM_MAC,
            ethertype: ETHER_TYPE_IPV4,
            payload,
            payload_len: NET_REPLY_PAYLOAD_BYTES.min(NET_REPLY_PAYLOAD_BYTES),
            kind: super::config::NetworkReplyKind::Udp { seq },
        }))
    }
}
