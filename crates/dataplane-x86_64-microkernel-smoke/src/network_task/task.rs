use super::config::{NetworkPacketKind, NetworkReply, NetworkReplyKind, TCP_STREAM_SESSIONS};
use super::counters::NetworkDropCounters;
use super::parsing::ethernet_frame_view;
use crate::http::{HttpFileSet, HttpTask};
use crate::layout::{
    CAP_KERNEL, EP_NET, EP_TCPIP, NET_TASK_BYTES, REQUEST_TCPIP_NET, TASK_NET, TASK_TCPIP,
};
use crate::tcp_stream::{BoundedTcpStream, TcpControlCounters, TcpPayloadOutcome};
use dataplane_microkernel_core::{Message, MessageBody};

pub(crate) struct TcpIpTask {
    pub(crate) peer_mac: Option<[u8; 6]>,
    pub(crate) dynamic_ipv4: Option<[u8; 4]>,
    pub(crate) http_streams: [BoundedTcpStream; TCP_STREAM_SESSIONS],
    pub(crate) tcp_counters: TcpControlCounters,
    pub(crate) network_drop_counters: NetworkDropCounters,
    pub(crate) timer_ticks: u32,
    pub(crate) next_session_generation: u32,
}

impl TcpIpTask {
    pub(crate) const fn new() -> Self {
        Self {
            peer_mac: None,
            dynamic_ipv4: None,
            http_streams: [BoundedTcpStream::new(); TCP_STREAM_SESSIONS],
            tcp_counters: TcpControlCounters::new(),
            network_drop_counters: NetworkDropCounters::new(),
            timer_ticks: 0,
            next_session_generation: 1,
        }
    }

    pub(crate) fn set_dynamic_ipv4(&mut self, ipv4: [u8; 4]) {
        self.dynamic_ipv4 = Some(ipv4);
    }

    pub(crate) fn has_dynamic_ipv4(&self) -> bool {
        self.dynamic_ipv4.is_some()
    }

    pub(super) fn stream_for_syn(
        &mut self,
        _peer_port: u16,
    ) -> Result<&mut BoundedTcpStream, &'static str> {
        let mut index = 0;
        while index < self.http_streams.len() {
            if self.http_streams[index].is_available() {
                return Ok(&mut self.http_streams[index]);
            }
            index += 1;
        }
        self.tcp_counters.too_many_sessions += 1;
        self.tcp_counters.queue_full_events += 1;
        self.tcp_counters.dropped_packets += 1;
        Err("tcp-session-full")
    }

    pub(super) fn stream_for_port(
        &mut self,
        peer_port: u16,
    ) -> Result<&mut BoundedTcpStream, &'static str> {
        let mut index = 0;
        while index < self.http_streams.len() {
            if self.http_streams[index].is_active()
                && self.http_streams[index].peer_port() == peer_port
            {
                return Ok(&mut self.http_streams[index]);
            }
            index += 1;
        }
        Err("tcp-session-peer")
    }

    pub(crate) fn active_tcp_sessions(&self) -> u32 {
        let mut count = 0;
        let mut index = 0;
        while index < self.http_streams.len() {
            if self.http_streams[index].is_active() {
                count += 1;
            }
            index += 1;
        }
        count
    }

    pub(crate) fn tcp_counters(&self) -> TcpControlCounters {
        self.tcp_counters
    }

    pub(crate) fn network_drop_counters(&self) -> NetworkDropCounters {
        self.network_drop_counters
    }

    pub(super) fn tick_for_session_event(&mut self) -> u32 {
        self.timer_ticks = self.timer_ticks.wrapping_add(1);
        self.tcp_counters.timer_ticks = self.timer_ticks;
        self.timer_ticks
    }

    pub(crate) fn probe_accept_syn(
        &mut self,
        peer_port: u16,
        peer_seq: u32,
    ) -> Result<(), &'static str> {
        let now_ticks = self.tick_for_session_event();
        if let Ok(stream) = self.stream_for_port(peer_port) {
            let _ = stream.retransmit_syn_ack(now_ticks);
            self.tcp_counters.retry_events = self.tcp_counters.retry_events.saturating_add(1);
        } else {
            let session_generation = self.next_session_generation;
            self.next_session_generation = self.next_session_generation.wrapping_add(1);
            self.stream_for_syn(peer_port)?.accept_syn(
                peer_port,
                peer_seq,
                now_ticks,
                session_generation,
            );
        }
        self.tcp_counters.active_sessions = self.active_tcp_sessions();
        Ok(())
    }

    pub(crate) fn probe_ingest_payload(
        &mut self,
        peer_port: u16,
        seq: u32,
        payload: &[u8],
    ) -> Result<TcpPayloadOutcome, &'static str> {
        let now_ticks = self.tick_for_session_event();
        let outcome = self
            .stream_for_port(peer_port)?
            .ingest_in_order_payload(peer_port, seq, payload, now_ticks)?;
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
        Ok(outcome)
    }

    pub(crate) fn probe_accept_rst(&mut self, peer_port: u16) -> Result<(), &'static str> {
        let now_ticks = self.tick_for_session_event();
        let reset_before_request_completion = {
            let stream = self.stream_for_port(peer_port)?;
            let reset_before_request_completion = stream.has_partial_request();
            stream.accept_rst(peer_port, now_ticks)?;
            reset_before_request_completion
        };
        self.tcp_counters.resets += 1;
        if reset_before_request_completion {
            self.tcp_counters.reset_before_request_completion += 1;
        }
        self.tcp_counters.active_sessions = self.active_tcp_sessions();
        Ok(())
    }

    pub(crate) fn probe_accept_fin_during_response(
        &mut self,
        peer_port: u16,
        seq: u32,
    ) -> Result<(), &'static str> {
        let _now_ticks = self.tick_for_session_event();
        let fin_during_response = {
            let stream = self.stream_for_port(peer_port)?;
            stream.queue_send_bytes(b"HTTP/1.0 200 OK\r\n\r\n", NetworkReplyKind::HttpGet)?;
            let fin_during_response = stream.has_queued_response();
            stream.accept_fin(peer_port, seq)?;
            fin_during_response
        };
        if fin_during_response {
            self.tcp_counters.fin_during_response += 1;
        }
        self.tcp_counters.active_sessions = self.active_tcp_sessions();
        Ok(())
    }

    pub(crate) fn net_frame_request(&self, len: usize) -> Result<Message, &'static str> {
        if len == 0 || len > NET_TASK_BYTES {
            return Err("tcpip-frame-len");
        }
        Ok(Message::new(
            TASK_TCPIP,
            EP_NET,
            REQUEST_TCPIP_NET,
            CAP_KERNEL,
            MessageBody::Pair(len as u32, 0),
        ))
    }

    pub(crate) fn classify_net_frame(
        &mut self,
        message: Message,
        frame: &[u8],
    ) -> Result<NetworkPacketKind, &'static str> {
        if message.from != TASK_NET || message.to != EP_TCPIP {
            return Err("tcpip-net-route");
        }
        let len = match message.body {
            MessageBody::Pair(len, 0) => len as usize,
            _ => return Err("tcpip-net-body"),
        };
        if len != frame.len() {
            return Err("tcpip-frame-body-len");
        }
        self.classify_frame(frame)
    }

    pub(crate) fn classify_frame(
        &mut self,
        frame: &[u8],
    ) -> Result<NetworkPacketKind, &'static str> {
        let frame = match ethernet_frame_view(frame) {
            Some(frame) => frame,
            None => return Err("tcpip-frame-short"),
        };
        if frame.len() < crate::layout::ETHERNET_HEADER_BYTES {
            return Err("tcpip-frame-short");
        }
        match crate::net_protocol::read_be_u16(frame, 12) {
            crate::layout::ETHER_TYPE_ARP => self.classify_arp(frame),
            crate::layout::ETHER_TYPE_IPV4 => self.classify_ipv4(frame),
            _ => {
                if self.should_account_unsupported_ethertype(frame) {
                    self.network_drop_counters.unsupported_ethertype += 1;
                }
                Err("tcpip-ethertype")
            }
        }
    }

    fn should_account_unsupported_ethertype(&self, frame: &[u8]) -> bool {
        let src = crate::net_protocol::read_mac(frame, 6);
        let dst = crate::net_protocol::read_mac(frame, 0);
        src != crate::layout::VM_MAC && (dst == crate::layout::VM_MAC || dst == [0xff; 6])
    }

    pub(crate) fn handle_net_frame(
        &mut self,
        message: Message,
        frame: &[u8],
        http_task: &mut HttpTask,
        http_files: &HttpFileSet<'_>,
    ) -> Result<Option<NetworkReply>, &'static str> {
        if message.from != TASK_NET || message.to != EP_TCPIP {
            return Err("tcpip-net-route");
        }
        let len = match message.body {
            MessageBody::Pair(len, 0) => len as usize,
            _ => return Err("tcpip-net-body"),
        };
        if len != frame.len() {
            return Err("tcpip-frame-body-len");
        }
        let Some(frame) = ethernet_frame_view(frame) else {
            return Ok(None);
        };
        match crate::net_protocol::read_be_u16(frame, 12) {
            crate::layout::ETHER_TYPE_ARP => self.prepare_arp_reply(frame),
            crate::layout::ETHER_TYPE_IPV4 => self.prepare_ipv4_reply(frame, http_task, http_files),
            _ => Ok(None),
        }
    }
}
