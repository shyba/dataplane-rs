use super::config::{
    NetworkReply, NetworkReplyKind, TCP_FLAG_FIN, TCP_FLAG_RST, TCP_FLAG_SYN, TCP_STREAM_TX_BYTES,
};
use super::counters::Ipv4Route;
use super::task::TcpIpTask;
use crate::http::{HttpFileSet, HttpResponseKind, HttpTask};
use crate::layout::{
    ETHER_TYPE_IPV4, HTTP_SERVER_PORT, IPV4_HEADER_BYTES, IPV4_PROTOCOL_TCP, VM_MAC,
};
use crate::net_protocol::{
    read_be_u16, read_be_u32, read_mac, write_be_u16, write_be_u32, write_ipv4_header,
};
use crate::tcp_stream::TcpPayloadOutcome;

struct TcpSegmentSpec<'a> {
    dst_port: u16,
    seq: u32,
    ack: u32,
    flags: u8,
    data: &'a [u8],
    kind: NetworkReplyKind,
    frame: &'a [u8],
    route: Ipv4Route,
}

impl TcpIpTask {
    pub(crate) fn prepare_tcp_reply(
        &mut self,
        frame: &[u8],
        ip: usize,
        total_len: usize,
        http_task: &mut HttpTask,
        http_files: &HttpFileSet<'_>,
        route: Ipv4Route,
    ) -> Result<Option<NetworkReply>, &'static str> {
        let tcp = ip + IPV4_HEADER_BYTES;
        if total_len < IPV4_HEADER_BYTES + 20 {
            return Ok(None);
        }
        let tcp_header_len = usize::from(frame[tcp + 12] >> 4) * 4;
        if tcp_header_len < 20 || IPV4_HEADER_BYTES + tcp_header_len > total_len {
            return Ok(None);
        }
        let src_port = read_be_u16(frame, tcp);
        let dst_port = read_be_u16(frame, tcp + 2);
        if dst_port != HTTP_SERVER_PORT {
            return Ok(None);
        }
        let flags = frame[tcp + 13];
        let seq = read_be_u32(frame, tcp + 4);
        let payload_offset = tcp + tcp_header_len;
        let payload_len = total_len - IPV4_HEADER_BYTES - tcp_header_len;
        let request_payload = &frame[payload_offset..payload_offset + payload_len];
        if flags & TCP_FLAG_RST != 0 {
            let now_ticks = self.tick_for_session_event();
            let reset_before_request_completion = {
                let Ok(stream) = self.stream_for_port(src_port) else {
                    return Ok(None);
                };
                let reset_before_request_completion = stream.has_partial_request();
                stream.accept_rst(src_port, now_ticks)?;
                reset_before_request_completion
            };
            self.tcp_counters.resets += 1;
            if reset_before_request_completion {
                self.tcp_counters.reset_before_request_completion += 1;
            }
            self.tcp_counters.active_sessions = self.active_tcp_sessions();
            return Ok(None);
        }
        if flags & TCP_FLAG_SYN != 0 {
            let now_ticks = self.tick_for_session_event();
            let duplicate_syn = if let Ok(stream) = self.stream_for_port(src_port) {
                let segment = stream.retransmit_syn_ack(now_ticks);
                self.tcp_counters.retry_events = self.tcp_counters.retry_events.saturating_add(1);
                Some(segment)
            } else {
                None
            };
            let segment = if let Some(segment) = duplicate_syn {
                segment
            } else {
                let session_generation = self.next_session_generation;
                self.next_session_generation = self.next_session_generation.wrapping_add(1);
                match self.stream_for_syn(src_port) {
                    Ok(stream) => stream.accept_syn(src_port, seq, now_ticks, session_generation),
                    Err("tcp-session-full") => {
                        self.tcp_counters.active_sessions = self.active_tcp_sessions();
                        return Ok(None);
                    }
                    Err(reason) => return Err(reason),
                }
            };
            self.tcp_counters.active_sessions = self.active_tcp_sessions();
            return Self::prepare_tcp_segment(TcpSegmentSpec {
                dst_port: segment.dst_port,
                seq: segment.seq,
                ack: segment.ack,
                flags: segment.flags,
                data: &[],
                kind: segment.kind,
                frame,
                route,
            })
            .map(Some);
        }
        if flags & TCP_FLAG_FIN != 0 && payload_len == 0 {
            let fin_during_response = {
                let Ok(stream) = self.stream_for_port(src_port) else {
                    return Ok(None);
                };
                let fin_during_response = stream.has_queued_response();
                let segment = stream.accept_fin(src_port, seq)?;
                (fin_during_response, segment)
            };
            if fin_during_response.0 {
                self.tcp_counters.fin_during_response += 1;
            }
            self.tcp_counters.active_sessions = self.active_tcp_sessions();
            let segment = fin_during_response.1;
            return Self::prepare_tcp_segment(TcpSegmentSpec {
                dst_port: segment.dst_port,
                seq: segment.seq,
                ack: segment.ack,
                flags: segment.flags,
                data: &[],
                kind: segment.kind,
                frame,
                route,
            })
            .map(Some);
        }
        if payload_len == 0 {
            let Ok(stream) = self.stream_for_port(src_port) else {
                return Ok(None);
            };
            if !stream.has_queued_response() {
                return Ok(None);
            }
            let segment = stream.emit_next_segment()?;
            return Self::prepare_tcp_segment(TcpSegmentSpec {
                dst_port: segment.dst_port,
                seq: segment.seq,
                ack: segment.ack,
                flags: segment.flags,
                data: stream.queued_tx(&segment),
                kind: segment.kind,
                frame,
                route,
            })
            .map(Some);
        }
        let pending_segment = {
            let now_ticks = self.tick_for_session_event();
            let Ok(stream) = self.stream_for_port(src_port) else {
                self.tcp_counters.dropped_packets += 1;
                self.tcp_counters.active_sessions = self.active_tcp_sessions();
                return Ok(None);
            };
            match stream.ingest_in_order_payload(src_port, seq, request_payload, now_ticks)? {
                TcpPayloadOutcome::Complete => None,
                TcpPayloadOutcome::Pending => Some((
                    TcpPayloadOutcome::Pending,
                    stream.ack_segment(NetworkReplyKind::TcpAck),
                )),
                TcpPayloadOutcome::Duplicate => Some((
                    TcpPayloadOutcome::Duplicate,
                    stream.ack_segment(NetworkReplyKind::TcpAck),
                )),
                TcpPayloadOutcome::Overflow => Some((
                    TcpPayloadOutcome::Overflow,
                    stream.ack_segment(NetworkReplyKind::TcpAck),
                )),
                TcpPayloadOutcome::BadSequence => Some((
                    TcpPayloadOutcome::BadSequence,
                    stream.ack_segment(NetworkReplyKind::TcpAck),
                )),
            }
        };
        if let Some((outcome, segment)) = pending_segment {
            match outcome {
                TcpPayloadOutcome::Pending => self.tcp_counters.partial_requests += 1,
                TcpPayloadOutcome::Duplicate => self.tcp_counters.duplicate_payloads += 1,
                TcpPayloadOutcome::Overflow => {
                    self.tcp_counters.rx_overflows += 1;
                    self.tcp_counters.queue_full_events += 1;
                    self.tcp_counters.dropped_packets += 1;
                }
                TcpPayloadOutcome::BadSequence => self.tcp_counters.dropped_packets += 1,
                TcpPayloadOutcome::Complete => {}
            }
            self.tcp_counters.active_sessions = self.active_tcp_sessions();
            return Self::prepare_tcp_segment(TcpSegmentSpec {
                dst_port: segment.dst_port,
                seq: segment.seq,
                ack: segment.ack,
                flags: segment.flags,
                data: &[],
                kind: segment.kind,
                frame,
                route,
            })
            .map(Some);
        }
        let stream = self.stream_for_port(src_port)?;
        let request = stream.request();
        let mut http_bytes = [0u8; TCP_STREAM_TX_BYTES];
        let response = http_task.build_response(request, http_files, &mut http_bytes)?;
        let kind = match response.kind {
            HttpResponseKind::GetIndex => NetworkReplyKind::HttpGet,
            HttpResponseKind::HeadIndex => NetworkReplyKind::HttpHead,
            HttpResponseKind::GetLarge => NetworkReplyKind::HttpBackpressure,
            HttpResponseKind::GetChain => NetworkReplyKind::HttpGet,
            HttpResponseKind::GetStatus => NetworkReplyKind::HttpGet,
            HttpResponseKind::Missing => NetworkReplyKind::Http404,
            HttpResponseKind::UnsupportedMethod => NetworkReplyKind::Http405,
            HttpResponseKind::PayloadTooLarge => NetworkReplyKind::Http413,
            HttpResponseKind::InternalError => NetworkReplyKind::Http500,
        };
        stream.queue_send_bytes(&http_bytes[..response.len], kind)?;
        let segment = stream.emit_next_segment()?;
        Self::prepare_tcp_segment(TcpSegmentSpec {
            dst_port: segment.dst_port,
            seq: segment.seq,
            ack: segment.ack,
            flags: segment.flags,
            data: stream.queued_tx(&segment),
            kind: segment.kind,
            frame,
            route,
        })
        .map(Some)
    }

    fn prepare_tcp_segment(spec: TcpSegmentSpec<'_>) -> Result<NetworkReply, &'static str> {
        let tcp_len = 20 + spec.data.len();
        if IPV4_HEADER_BYTES + tcp_len > super::config::NET_REPLY_PAYLOAD_BYTES {
            return Err("tcpip-tcp-reply-len");
        }
        let mut payload = [0u8; super::config::NET_REPLY_PAYLOAD_BYTES];
        write_ipv4_header(
            &mut payload[..IPV4_HEADER_BYTES],
            IPV4_PROTOCOL_TCP,
            tcp_len,
            spec.dst_port,
            spec.route.local_ipv4,
            spec.route.peer_ipv4,
        );
        let tcp = IPV4_HEADER_BYTES;
        write_be_u16(&mut payload[tcp..tcp + 2], HTTP_SERVER_PORT);
        write_be_u16(&mut payload[tcp + 2..tcp + 4], spec.dst_port);
        write_be_u32(&mut payload[tcp + 4..tcp + 8], spec.seq);
        write_be_u32(&mut payload[tcp + 8..tcp + 12], spec.ack);
        payload[tcp + 12] = 5 << 4;
        payload[tcp + 13] = spec.flags;
        write_be_u16(&mut payload[tcp + 14..tcp + 16], 4096);
        write_be_u16(&mut payload[tcp + 16..tcp + 18], 0);
        write_be_u16(&mut payload[tcp + 18..tcp + 20], 0);
        payload[tcp + 20..tcp + 20 + spec.data.len()].copy_from_slice(spec.data);
        let checksum = super::parsing::tcp_checksum(
            spec.route.local_ipv4,
            spec.route.peer_ipv4,
            &payload[tcp..tcp + tcp_len],
        );
        write_be_u16(&mut payload[tcp + 16..tcp + 18], checksum);
        Ok(NetworkReply {
            dst: read_mac(spec.frame, 6),
            src: VM_MAC,
            ethertype: ETHER_TYPE_IPV4,
            payload,
            payload_len: IPV4_HEADER_BYTES + tcp_len,
            kind: spec.kind,
        })
    }
}
