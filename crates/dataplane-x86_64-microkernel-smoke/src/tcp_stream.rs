use crate::{
    NetworkReplyKind, TcpQueuedSegment, TCP_FLAG_ACK, TCP_FLAG_FIN, TCP_FLAG_PSH, TCP_FLAG_SYN,
    TCP_STREAM_RX_BYTES, TCP_STREAM_SEGMENT_BYTES, TCP_STREAM_TIMEOUT_TICKS, TCP_STREAM_TX_BYTES,
};

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TcpStreamState {
    Closed,
    SynReceived,
    Established,
    FinWait,
    Reset,
    Error,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(crate) enum TcpPayloadOutcome {
    Pending,
    Complete,
    Duplicate,
    Overflow,
    BadSequence,
}

#[derive(Clone, Copy)]
pub(crate) struct TcpControlCounters {
    pub(crate) active_sessions: u32,
    pub(crate) partial_requests: u32,
    pub(crate) duplicate_payloads: u32,
    pub(crate) resets: u32,
    pub(crate) reset_before_request_completion: u32,
    pub(crate) dropped_packets: u32,
    pub(crate) queue_full_events: u32,
    pub(crate) rx_overflows: u32,
    pub(crate) too_many_sessions: u32,
    pub(crate) fin_during_response: u32,
    pub(crate) timeout_events: u32,
    pub(crate) retry_events: u32,
    pub(crate) last_timeout_age_ticks: u32,
    pub(crate) last_timeout_deadline_ticks: u32,
    pub(crate) last_activity_ticks: u32,
    pub(crate) timer_ticks: u32,
}

impl TcpControlCounters {
    pub(crate) const fn new() -> Self {
        Self {
            active_sessions: 0,
            partial_requests: 0,
            duplicate_payloads: 0,
            resets: 0,
            reset_before_request_completion: 0,
            dropped_packets: 0,
            queue_full_events: 0,
            rx_overflows: 0,
            too_many_sessions: 0,
            fin_during_response: 0,
            timeout_events: 0,
            retry_events: 0,
            last_timeout_age_ticks: 0,
            last_timeout_deadline_ticks: 0,
            last_activity_ticks: 0,
            timer_ticks: 0,
        }
    }
}

#[derive(Clone, Copy)]
pub(crate) struct StreamTimeoutSnapshot {
    pub(crate) peer_port: u16,
    pub(crate) session_generation: u32,
    pub(crate) last_activity_ticks: u32,
    pub(crate) deadline_ticks: u32,
    pub(crate) age_ticks: u32,
    pub(crate) retry_count: u32,
    pub(crate) timeout_count: u32,
}

#[derive(Clone, Copy)]
pub(crate) struct BoundedTcpStream {
    state: TcpStreamState,
    peer_port: u16,
    peer_next_seq: u32,
    local_next_seq: u32,
    session_generation: u32,
    last_activity_ticks: u32,
    timeout_deadline_ticks: u32,
    last_age_ticks: u32,
    retry_count: u32,
    timeout_count: u32,
    timed_out: bool,
    rx: [u8; TCP_STREAM_RX_BYTES],
    rx_len: usize,
    tx: [u8; TCP_STREAM_TX_BYTES],
    tx_len: usize,
    tx_offset: usize,
    response_kind: NetworkReplyKind,
}

impl BoundedTcpStream {
    pub(crate) const fn new() -> Self {
        Self {
            state: TcpStreamState::Closed,
            peer_port: 0,
            peer_next_seq: 0,
            local_next_seq: 0,
            session_generation: 0,
            last_activity_ticks: 0,
            timeout_deadline_ticks: 0,
            last_age_ticks: 0,
            retry_count: 0,
            timeout_count: 0,
            timed_out: false,
            rx: [0; TCP_STREAM_RX_BYTES],
            rx_len: 0,
            tx: [0; TCP_STREAM_TX_BYTES],
            tx_len: 0,
            tx_offset: 0,
            response_kind: NetworkReplyKind::TcpAck,
        }
    }
    pub(crate) fn is_available(&self) -> bool {
        matches!(
            self.state,
            TcpStreamState::Closed
                | TcpStreamState::FinWait
                | TcpStreamState::Reset
                | TcpStreamState::Error
        )
    }
    pub(crate) fn is_active(&self) -> bool {
        !self.is_available()
    }
    pub(crate) fn peer_port(&self) -> u16 {
        self.peer_port
    }
    pub(crate) fn accept_syn(
        &mut self,
        peer_port: u16,
        peer_seq: u32,
        now_ticks: u32,
        session_generation: u32,
    ) -> TcpQueuedSegment {
        self.state = TcpStreamState::SynReceived;
        self.peer_port = peer_port;
        self.peer_next_seq = peer_seq.wrapping_add(1);
        self.local_next_seq = tcp_server_seq(peer_port).wrapping_add(1);
        self.session_generation = session_generation;
        self.last_activity_ticks = now_ticks;
        self.timeout_deadline_ticks = now_ticks.wrapping_add(TCP_STREAM_TIMEOUT_TICKS);
        self.last_age_ticks = 0;
        self.retry_count = 0;
        self.timeout_count = 0;
        self.timed_out = false;
        self.rx_len = 0;
        self.tx_len = 0;
        self.tx_offset = 0;
        self.response_kind = NetworkReplyKind::TcpAck;
        TcpQueuedSegment {
            dst_port: peer_port,
            seq: tcp_server_seq(peer_port),
            ack: self.peer_next_seq,
            flags: TCP_FLAG_SYN | TCP_FLAG_ACK,
            offset: 0,
            len: 0,
            kind: NetworkReplyKind::TcpSynAck,
        }
    }
    pub(crate) fn retransmit_syn_ack(&mut self, now_ticks: u32) -> TcpQueuedSegment {
        self.record_activity(now_ticks);
        self.retry_count = self.retry_count.saturating_add(1);
        TcpQueuedSegment {
            dst_port: self.peer_port,
            seq: tcp_server_seq(self.peer_port),
            ack: self.peer_next_seq,
            flags: TCP_FLAG_SYN | TCP_FLAG_ACK,
            offset: 0,
            len: 0,
            kind: NetworkReplyKind::TcpSynAck,
        }
    }
    pub(crate) fn ingest_in_order_payload(
        &mut self,
        peer_port: u16,
        seq: u32,
        payload: &[u8],
        now_ticks: u32,
    ) -> Result<TcpPayloadOutcome, &'static str> {
        if !self.is_active() || self.peer_port != peer_port {
            return Err("tcp-session-peer");
        }
        self.record_activity(now_ticks);
        if seq != self.peer_next_seq {
            if seq.wrapping_add(payload.len() as u32) == self.peer_next_seq
                && payload.len() <= self.rx_len
            {
                return Ok(TcpPayloadOutcome::Duplicate);
            }
            return Ok(TcpPayloadOutcome::BadSequence);
        }
        if payload.is_empty() {
            return Ok(TcpPayloadOutcome::Pending);
        }
        if self.rx_len + payload.len() > self.rx.len() {
            self.state = TcpStreamState::Error;
            return Ok(TcpPayloadOutcome::Overflow);
        }
        let start = self.rx_len;
        self.rx[start..start + payload.len()].copy_from_slice(payload);
        self.rx_len += payload.len();
        self.peer_next_seq = self.peer_next_seq.wrapping_add(payload.len() as u32);
        self.state = TcpStreamState::Established;
        if self.request_complete() {
            Ok(TcpPayloadOutcome::Complete)
        } else {
            Ok(TcpPayloadOutcome::Pending)
        }
    }
    pub(crate) fn queue_send_bytes(
        &mut self,
        bytes: &[u8],
        kind: NetworkReplyKind,
    ) -> Result<(), &'static str> {
        if self.state != TcpStreamState::Established {
            return Err("tcp-session-state");
        }
        if bytes.is_empty() || bytes.len() > self.tx.len() {
            return Err("tcp-session-tx");
        }
        self.tx[..bytes.len()].copy_from_slice(bytes);
        self.tx_len = bytes.len();
        self.tx_offset = 0;
        self.response_kind = kind;
        Ok(())
    }
    pub(crate) fn emit_next_segment(&mut self) -> Result<TcpQueuedSegment, &'static str> {
        if self.tx_offset >= self.tx_len {
            return Err("tcp-session-empty-tx");
        }
        let offset = self.tx_offset;
        let remaining = self.tx_len - offset;
        let len = remaining.min(TCP_STREAM_SEGMENT_BYTES);
        let last = offset + len == self.tx_len;
        let seq = self.local_next_seq;
        self.local_next_seq = self.local_next_seq.wrapping_add(len as u32);
        self.tx_offset += len;
        Ok(TcpQueuedSegment {
            dst_port: self.peer_port,
            seq,
            ack: self.peer_next_seq,
            flags: if last {
                TCP_FLAG_FIN | TCP_FLAG_PSH | TCP_FLAG_ACK
            } else {
                TCP_FLAG_PSH | TCP_FLAG_ACK
            },
            offset,
            len,
            kind: self.response_kind,
        })
    }
    pub(crate) fn ack_segment(&self, kind: NetworkReplyKind) -> TcpQueuedSegment {
        TcpQueuedSegment {
            dst_port: self.peer_port,
            seq: self.local_next_seq,
            ack: self.peer_next_seq,
            flags: TCP_FLAG_ACK,
            offset: 0,
            len: 0,
            kind,
        }
    }
    pub(crate) fn accept_fin(
        &mut self,
        peer_port: u16,
        seq: u32,
    ) -> Result<TcpQueuedSegment, &'static str> {
        if !self.is_active() || self.peer_port != peer_port {
            return Err("tcp-session-fin-peer");
        }
        if seq != self.peer_next_seq {
            return Err("tcp-session-fin-seq");
        }
        self.peer_next_seq = self.peer_next_seq.wrapping_add(1);
        self.state = TcpStreamState::FinWait;
        Ok(self.ack_segment(NetworkReplyKind::TcpFinAck))
    }
    pub(crate) fn accept_rst(
        &mut self,
        peer_port: u16,
        now_ticks: u32,
    ) -> Result<(), &'static str> {
        if self.peer_port != peer_port {
            return Err("tcp-session-rst-peer");
        }
        self.record_activity(now_ticks);
        self.state = TcpStreamState::Reset;
        Ok(())
    }
    fn record_activity(&mut self, now_ticks: u32) {
        self.last_activity_ticks = now_ticks;
        self.timeout_deadline_ticks = now_ticks.wrapping_add(TCP_STREAM_TIMEOUT_TICKS);
        self.last_age_ticks = 0;
    }
    fn refresh_timeout_age(&mut self, now_ticks: u32) -> u32 {
        self.last_age_ticks = now_ticks.wrapping_sub(self.last_activity_ticks);
        self.last_age_ticks
    }
    pub(crate) fn expire_if_timed_out(&mut self, now_ticks: u32) -> Option<StreamTimeoutSnapshot> {
        if !self.is_active() || self.timed_out {
            return None;
        }
        let age_ticks = self.refresh_timeout_age(now_ticks);
        if age_ticks <= TCP_STREAM_TIMEOUT_TICKS {
            return None;
        }
        self.timed_out = true;
        self.timeout_count = self.timeout_count.saturating_add(1);
        self.state = TcpStreamState::Error;
        Some(StreamTimeoutSnapshot {
            peer_port: self.peer_port,
            session_generation: self.session_generation,
            last_activity_ticks: self.last_activity_ticks,
            deadline_ticks: self.timeout_deadline_ticks,
            age_ticks,
            retry_count: self.retry_count,
            timeout_count: self.timeout_count,
        })
    }
    pub(crate) fn has_partial_request(&self) -> bool {
        self.rx_len > 0 && !self.request_complete()
    }
    pub(crate) fn has_queued_response(&self) -> bool {
        self.tx_offset < self.tx_len
    }
    pub(crate) fn request(&self) -> &[u8] {
        &self.rx[..self.rx_len]
    }
    pub(crate) fn queued_tx(&self, segment: &TcpQueuedSegment) -> &[u8] {
        &self.tx[segment.offset..segment.offset + segment.len]
    }
    fn request_complete(&self) -> bool {
        let bytes = &self.rx[..self.rx_len];
        let mut index = 0;
        while index + 3 < bytes.len() {
            if bytes[index..index + 4] == *b"\r\n\r\n" {
                return true;
            }
            index += 1;
        }
        false
    }
}

fn tcp_server_seq(port: u16) -> u32 {
    (0x4D_4B_00_00u32) | u32::from(port)
}
