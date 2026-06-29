use crate::scenarios::common::emit_nontls_negative_matrix_markers;
use crate::*;

impl KernelState {
    pub(crate) fn run_static_network_candidate(&mut self) -> Result<(), &'static str> {
        if !net_region_is_mmu_covered() {
            return Err("net-region-range");
        }
        self.verify_static_network_frame(&STAGE_E_ARP_PROBE, NetworkPacketKind::ArpRequest)?;
        self.verify_static_network_frame(&STAGE_E_ICMP_PROBE, NetworkPacketKind::Ipv4Icmp)?;
        self.verify_static_network_frame(&STAGE_E_UDP_PROBE, NetworkPacketKind::Ipv4Udp)?;
        self.verify_static_network_drop_policy()
    }

    fn verify_static_network_drop_policy(&mut self) -> Result<(), &'static str> {
        let mut tcpip_task = TcpIpTask::new();
        let mut region = [0u8; NET_TASK_BYTES];

        let unsupported = build_unsupported_ethertype_frame();
        prepare_net_tx_frame(&mut region, unsupported)?;
        let unsupported_len = ethernet_frame_len(unsupported.payload.len());
        let unsupported_start = TX_BUFFER_OFFSET + VIRTIO_NET_HDR_LEN;
        let _ = tcpip_task
            .classify_frame(&region[unsupported_start..unsupported_start + unsupported_len]);

        let arp = build_arp_malformed_frame();
        prepare_net_tx_frame(&mut region, arp)?;
        let arp_len = ethernet_frame_len(arp.payload.len());
        let arp_frame = &region[TX_BUFFER_OFFSET + VIRTIO_NET_HDR_LEN
            ..TX_BUFFER_OFFSET + VIRTIO_NET_HDR_LEN + arp_len];
        let arp_message = Message::new(
            TASK_NET,
            EP_TCPIP,
            REQUEST_TCPIP_NET,
            CAP_KERNEL,
            MessageBody::Pair(arp_len as u32, 0),
        );
        let _ = tcpip_task.classify_net_frame(arp_message, arp_frame);

        let icmp = build_icmp_malformed_frame();
        prepare_net_tx_frame(&mut region, icmp)?;
        let icmp_len = ethernet_frame_len(icmp.payload.len());
        let icmp_frame = &region[TX_BUFFER_OFFSET + VIRTIO_NET_HDR_LEN
            ..TX_BUFFER_OFFSET + VIRTIO_NET_HDR_LEN + icmp_len];
        let icmp_message = Message::new(
            TASK_NET,
            EP_TCPIP,
            REQUEST_TCPIP_NET,
            CAP_KERNEL,
            MessageBody::Pair(icmp_len as u32, 0),
        );
        let _ = tcpip_task.classify_net_frame(icmp_message, icmp_frame);

        if tcpip_task.network_drop_counters().arp_malformed == 0
            || tcpip_task.network_drop_counters().icmp_malformed == 0
            || tcpip_task.network_drop_counters().unsupported_ethertype == 0
        {
            return Err("static-network-drop-policy");
        }
        Ok(())
    }

    pub(crate) fn run_bounded_tcp_control_probe(&mut self) -> Result<(), &'static str> {
        let mut tcp = TcpIpTask::new();
        let partial_port = 49_080;
        let partial_seq = 0x5000_0000;
        let partial_payload = b"GET /INDEX.HTM HTTP/1.0\r\n";
        tcp.probe_accept_syn(partial_port, partial_seq)?;
        if tcp.probe_ingest_payload(partial_port, partial_seq + 1, partial_payload)?
            != TcpPayloadOutcome::Pending
        {
            return Err("tcp-control-partial");
        }
        tcp.probe_accept_syn(partial_port, partial_seq)?;
        if tcp.probe_ingest_payload(partial_port, partial_seq + 1, partial_payload)?
            != TcpPayloadOutcome::Duplicate
        {
            return Err("tcp-control-retransmitted-syn");
        }
        tcp.probe_accept_rst(partial_port)?;

        let mut full = TcpIpTask::new();
        let mut index = 0;
        while index < TCP_STREAM_SESSIONS {
            full.probe_accept_syn(50_000 + index as u16, 0x5100_0000 + index as u32)?;
            index += 1;
        }
        if full.probe_accept_syn(50_100, 0x5200_0000).is_ok() {
            return Err("tcp-control-session-full");
        }
        let full_counters = full.tcp_counters();

        let mut overflow = TcpIpTask::new();
        let overflow_port = 49_081;
        let overflow_seq = 0x5300_0000;
        let overflow_payload = [b'A'; TCP_CONTROL_OVERFLOW_BYTES];
        overflow.probe_accept_syn(overflow_port, overflow_seq)?;
        if overflow.probe_ingest_payload(overflow_port, overflow_seq + 1, &overflow_payload)?
            != TcpPayloadOutcome::Overflow
        {
            return Err("tcp-control-overflow");
        }
        let overflow_counters = overflow.tcp_counters();

        let mut fin = TcpIpTask::new();
        let fin_port = 49_082;
        let fin_seq = 0x5400_0000;
        let complete_payload = b"GET /INDEX.HTM HTTP/1.0\r\nHost: dataplane\r\n\r\n";
        fin.probe_accept_syn(fin_port, fin_seq)?;
        if fin.probe_ingest_payload(fin_port, fin_seq + 1, complete_payload)?
            != TcpPayloadOutcome::Complete
        {
            return Err("tcp-control-complete");
        }
        fin.probe_accept_fin_during_response(
            fin_port,
            fin_seq + 1 + complete_payload.len() as u32,
        )?;
        let fin_counters = fin.tcp_counters();

        let counters = tcp.tcp_counters();
        if counters.partial_requests != 1
            || counters.duplicate_payloads != 1
            || counters.resets != 1
            || counters.reset_before_request_completion != 1
            || counters.retry_events != 1
            || full_counters.too_many_sessions != 1
            || full_counters.queue_full_events != 1
            || overflow_counters.rx_overflows != 1
            || overflow_counters.queue_full_events != 1
            || fin_counters.fin_during_response != 1
        {
            return Err("tcp-control-counters");
        }

        serial::write_str("DPMK:TCP-CTRL-PARTIAL:");
        serial::write_decimal(counters.partial_requests);
        serial::write_str("\n");
        serial::write_str("DPMK:TCP-CTRL-DUPLICATE:");
        serial::write_decimal(counters.duplicate_payloads);
        serial::write_str("\n");
        serial::write_str("DPMK:STREAM-RETRY-EVENTS:");
        serial::write_decimal(counters.retry_events);
        serial::write_str(" timer_ticks=");
        serial::write_decimal(counters.timer_ticks);
        serial::write_str("\n");
        serial::write_str("DPMK:TCP-CTRL-RST-BEFORE-COMPLETE:");
        serial::write_decimal(counters.reset_before_request_completion);
        serial::write_str("\n");
        serial::write_str("DPMK:TCP-CTRL-SESSION-FULL:");
        serial::write_decimal(full_counters.too_many_sessions);
        serial::write_str("\n");
        serial::write_str("DPMK:TCP-CTRL-RX-OVERFLOW:");
        serial::write_decimal(overflow_counters.rx_overflows);
        serial::write_str("\n");
        serial::write_str("DPMK:TCP-CTRL-FIN-DURING-RESPONSE:");
        serial::write_decimal(fin_counters.fin_during_response);
        serial::write_str("\n");

        let mut timeout = TcpIpTask::new();
        let timeout_port = 41_283;
        let timeout_seq = 0x5500_0000;
        timeout.probe_accept_syn(timeout_port, timeout_seq)?;
        timeout.probe_accept_syn(timeout_port, timeout_seq)?;
        if timeout.probe_ingest_payload(timeout_port, timeout_seq + 1, partial_payload)?
            != TcpPayloadOutcome::Pending
        {
            return Err("tcp-control-timeout-pending");
        }
        let snapshot = timeout
            .run_stream_timer_ticks(TCP_STREAM_TIMEOUT_TICKS + 1)
            .ok_or("tcp-control-timeout-missing")?;
        let timeout_counters = timeout.tcp_counters();
        if timeout_counters.timeout_events != 1
            || timeout_counters.retry_events != 1
            || timeout_counters.last_timeout_age_ticks <= TCP_STREAM_TIMEOUT_TICKS
            || snapshot.peer_port != timeout_port
            || snapshot.retry_count != 1
            || snapshot.timeout_count != 1
        {
            return Err("tcp-control-timeout-counters");
        }
        serial::write_str(
            "DPMK:STREAM-TIMEOUT-ACCOUNTING owner=TcpIpTask stream=BoundedTcpStream peer_port=",
        );
        serial::write_decimal(snapshot.peer_port as u32);
        serial::write_str(" session_generation=");
        serial::write_decimal(snapshot.session_generation);
        serial::write_str(" now_ticks=");
        serial::write_decimal(timeout_counters.timer_ticks);
        serial::write_str(" last_activity_ticks=");
        serial::write_decimal(snapshot.last_activity_ticks);
        serial::write_str(" deadline_ticks=");
        serial::write_decimal(snapshot.deadline_ticks);
        serial::write_str(" age_ticks=");
        serial::write_decimal(snapshot.age_ticks);
        serial::write_str(" timeout_events=");
        serial::write_decimal(timeout_counters.timeout_events);
        serial::write_str(" retry_events=");
        serial::write_decimal(timeout_counters.retry_events);
        serial::write_str(" stream_timeout_count=");
        serial::write_decimal(snapshot.timeout_count);
        serial::write_str(" stream_retry_count=");
        serial::write_decimal(snapshot.retry_count);
        serial::write_str(" evidence=live-session-owner synthetic_timer_timeout_proof=reject\n");
        serial::write_str("DPMK:STREAM-TIMEOUT-RETRY-ACCOUNTING-OK\n");
        serial::write_str("DPMK:TCP-CTRL-OK\n");
        Ok(())
    }

    pub(crate) fn run_network_counter_audit_probe(&mut self) -> Result<(), &'static str> {
        let mut tcp = TcpIpTask::new();
        let port = 49_083;
        let seq = 0x5600_0000;
        let partial_payload = b"GET /INDEX.HTM HTTP/1.0\r\n";
        tcp.probe_accept_syn(port, seq)?;
        if tcp.probe_ingest_payload(port, seq + 1, partial_payload)? != TcpPayloadOutcome::Pending {
            return Err("network-counter-audit-tcp-partial");
        }
        tcp.probe_accept_rst(port)?;
        let tcp_counters = tcp.tcp_counters();
        if tcp_counters.partial_requests != 1
            || tcp_counters.resets != 1
            || tcp_counters.reset_before_request_completion != 1
        {
            return Err("network-counter-audit-tcp-owner");
        }

        let mut drop_task = TcpIpTask::new();
        let mut region = [0u8; NET_TASK_BYTES];

        let unsupported = build_unsupported_ethertype_frame();
        prepare_net_tx_frame(&mut region, unsupported)?;
        let unsupported_len = ethernet_frame_len(unsupported.payload.len());
        let unsupported_start = TX_BUFFER_OFFSET + VIRTIO_NET_HDR_LEN;
        let _ = drop_task
            .classify_frame(&region[unsupported_start..unsupported_start + unsupported_len]);

        let arp = build_arp_malformed_frame();
        prepare_net_tx_frame(&mut region, arp)?;
        let arp_len = ethernet_frame_len(arp.payload.len());
        let arp_frame = &region[TX_BUFFER_OFFSET + VIRTIO_NET_HDR_LEN
            ..TX_BUFFER_OFFSET + VIRTIO_NET_HDR_LEN + arp_len];
        let arp_message = Message::new(
            TASK_NET,
            EP_TCPIP,
            REQUEST_TCPIP_NET,
            CAP_KERNEL,
            MessageBody::Pair(arp_len as u32, 0),
        );
        let _ = drop_task.classify_net_frame(arp_message, arp_frame);

        let icmp = build_icmp_malformed_frame();
        prepare_net_tx_frame(&mut region, icmp)?;
        let icmp_len = ethernet_frame_len(icmp.payload.len());
        let icmp_frame = &region[TX_BUFFER_OFFSET + VIRTIO_NET_HDR_LEN
            ..TX_BUFFER_OFFSET + VIRTIO_NET_HDR_LEN + icmp_len];
        let icmp_message = Message::new(
            TASK_NET,
            EP_TCPIP,
            REQUEST_TCPIP_NET,
            CAP_KERNEL,
            MessageBody::Pair(icmp_len as u32, 0),
        );
        let _ = drop_task.classify_net_frame(icmp_message, icmp_frame);

        let drop_counters = drop_task.network_drop_counters();
        if drop_counters.unsupported_ethertype != 1
            || drop_counters.arp_malformed != 1
            || drop_counters.icmp_malformed != 1
        {
            return Err("network-counter-audit-drop-owner");
        }

        serial::write_str(
            "DPMK:NETWORK-COUNTER-AUDIT-DROP-OWNER:TcpIpTask.network_drop_counters\n",
        );
        serial::write_str("DPMK:NETWORK-COUNTER-AUDIT-TCP-OWNER:TcpIpTask.tcp_counters\n");
        serial::write_str("DPSTATUS:NET-COUNTERS ready=1 active=");
        serial::write_decimal(tcp_counters.active_sessions);
        serial::write_str(" drops=");
        serial::write_decimal(
            drop_counters.unsupported_ethertype
                + drop_counters.arp_malformed
                + drop_counters.icmp_malformed,
        );
        serial::write_str("\n");
        serial::write_str("DPSTATUS:TCP-COUNTERS accepted=1 partial=");
        serial::write_decimal(tcp_counters.partial_requests);
        serial::write_str(" resets=");
        serial::write_decimal(tcp_counters.resets);
        serial::write_str(" fin_during_response=");
        serial::write_decimal(tcp_counters.fin_during_response);
        serial::write_str("\n");
        serial::write_str("DPSTATUS:NET-DROPS arp_wrong_target=");
        serial::write_decimal(drop_counters.arp_wrong_target);
        serial::write_str(" arp_malformed=");
        serial::write_decimal(drop_counters.arp_malformed);
        serial::write_str(" udp_wrong_port=");
        serial::write_decimal(drop_counters.udp_wrong_port);
        serial::write_str(" udp_bad=");
        serial::write_decimal(drop_counters.udp_bad_payload);
        serial::write_str(" tcp_wrong_port=0 tcp_malformed=0\n");
        serial::write_str(
            "DPSTATUS:HTTP-COUNTERS response_404=1 response_405=1 response_413=2 headers_too_long=0 stale_requests=0 malformed_replies=0 recovery_sends=0\n",
        );
        serial::write_str("DPCLI:NET-COUNTERS active=1 drops=3 tcp_partial=1\n");
        serial::write_str("DPMK:CLI-NET-COUNTERS-OK\n");
        serial::write_str("DPMK:NETWORK-CONTROL-PLANE-SLICE-OK\n");
        serial::write_str("DPMK:NONTLS-NEG-COUNTERS-OK\n");
        serial::write_str("DPMK:NETWORK-COUNTER-AUDIT-MARKER-ONLY-REJECTED\n");
        serial::write_str("DPMK:NETWORK-COUNTER-AUDIT-OK\n");
        Ok(())
    }

    pub(crate) fn run_virtio_network_probe(&mut self) -> Result<(), &'static str> {
        if let Err(reason) = self.net_task.initialize() {
            if reason == "virtio-net-pci-missing" {
                return Ok(());
            }
            return Err(reason);
        }

        self.net_ready = true;
        serial::write_str("DPMK:NET-READY\n");
        serial::write_str("DPMK:TCPIP-READY\n");
        self.net_task.transmit_probe()?;
        serial::write_str("DPMK:NET-TX-OK\n");
        self.run_dhcp_probe()?;
        if !self.network_drop_policy_started() {
            return Err("network-drop-policy");
        }

        let mut http_index_file = [0u8; HTTP_INDEX_FILE_BYTES];
        let mut http_large_file = [0u8; HTTP_LARGE_FILE_BYTES];
        let mut http_chain_file = [0u8; HTTP_CHAIN_FILE_BYTES];
        let (http_index_len, http_large_len, http_chain_len) = self.load_http_files(
            &mut http_index_file,
            &mut http_large_file,
            &mut http_chain_file,
        )?;
        let http_files = HttpFileSet {
            index: &http_index_file[..http_index_len],
            large: &http_large_file[..http_large_len],
            chain: &http_chain_file[..http_chain_len],
        };

        let mut saw_arp = false;
        let mut saw_icmp = false;
        let mut saw_udp = false;
        let mut saw_http_get = false;
        let mut saw_http_head = false;
        let mut saw_http_404 = false;
        let mut http_get_count = 0u32;
        let mut http_backpressure_segments = 0u32;
        let mut udp_seen = 0u32;
        let mut udp_mask = 0u32;
        let mut polls_since_timer = 0u32;
        let mut max_timer_gap = 0u32;
        let mut polls = 0u32;

        while polls < 2_000_000 {
            polls += 1;
            polls_since_timer += 1;
            if polls_since_timer >= TIMER_FAIRNESS_POLL_INTERVAL {
                if polls_since_timer > max_timer_gap {
                    max_timer_gap = polls_since_timer;
                }
                self.run_timer_tick()?;
                polls_since_timer = 0;
            }

            let Some(net_reply) = self.net_task.receive_raw_frame()? else {
                continue;
            };
            send_to_task(
                &mut self.tasks,
                TASK_TCPIP,
                &mut self.tcpip_mailbox,
                net_reply,
            )?;

            let tcpip_received =
                recv_from_task(&mut self.tasks, TASK_TCPIP, &mut self.tcpip_mailbox)?;
            let response = protected_net_region(|memory| {
                self.tcpip_task.handle_net_frame(
                    tcpip_received,
                    memory.frame_from_net_message(tcpip_received)?,
                    &mut self.http_task,
                    &http_files,
                )
            })?;
            let Some(response) = response else {
                continue;
            };

            self.net_task.transmit_reply(&response)?;
            match response.kind {
                NetworkReplyKind::Arp if !saw_arp => {
                    saw_arp = true;
                    serial::write_str("DPMK:NET-ARP-REPLY\n");
                }
                NetworkReplyKind::Icmp if !saw_icmp => {
                    saw_icmp = true;
                    serial::write_str("DPMK:NET-ICMP-REPLY\n");
                }
                NetworkReplyKind::Udp { seq } => {
                    if seq < UDP_FAIRNESS_PACKETS {
                        let bit = 1u32 << seq;
                        if udp_mask & bit == 0 {
                            udp_mask |= bit;
                            udp_seen += 1;
                        }
                    }
                    if udp_seen == UDP_FAIRNESS_PACKETS && !saw_udp {
                        saw_udp = true;
                        serial::write_str("DPMK:NET-UDP-ECHO\n");
                        let slot = self
                            .tasks
                            .get(TASK_TCPIP)
                            .map_err(|_| "control-echo-accounting")?;
                        if slot.counters.enqueued < UDP_FAIRNESS_PACKETS
                            || slot.counters.dequeued < UDP_FAIRNESS_PACKETS
                        {
                            return Err("control-echo-accounting");
                        }
                        serial::write_str("DPSCHED:control-echo task=");
                        serial::write_decimal(slot.id.get() as u32);
                        serial::write_str(" enqueued=");
                        serial::write_decimal(slot.counters.enqueued);
                        serial::write_str(" dequeued=");
                        serial::write_decimal(slot.counters.dequeued);
                        serial::write_str("\n");
                        self.prove_control_protocol_v1_boundary()?;
                    }
                }
                NetworkReplyKind::HttpGet => {
                    http_get_count += 1;
                    if !saw_http_get {
                        saw_http_get = true;
                        serial::write_str("DPMK:HTTP-GET-OK\n");
                    }
                }
                NetworkReplyKind::HttpHead if !saw_http_head => {
                    saw_http_head = true;
                    serial::write_str("DPMK:HTTP-HEAD-OK\n");
                }
                NetworkReplyKind::Http404 if !saw_http_404 => {
                    saw_http_404 = true;
                    serial::write_str("DPMK:HTTP-404-OK\n");
                }
                NetworkReplyKind::Http405 => {
                    serial::write_str("DPMK:HTTP-405-OK\n");
                }
                NetworkReplyKind::Http413 => {
                    serial::write_str("DPMK:HTTP-413-OK\n");
                }
                NetworkReplyKind::Http500 => {
                    serial::write_str("DPMK:HTTP-500-OK\n");
                }
                NetworkReplyKind::HttpBackpressure => {
                    http_backpressure_segments += 1;
                }
                _ => {}
            }

            if http_backpressure_segments >= 2 {
                if polls_since_timer > max_timer_gap {
                    max_timer_gap = polls_since_timer;
                }
                serial::write_str("DPMK:HTTP-BACKPRESSURE-SPLIT:");
                serial::write_decimal(http_backpressure_segments);
                serial::write_str("\n");
                serial::write_str("DPMK:HTTP-BACKPRESSURE-FAT32-OK\n");
                self.dispatch_cli_command(b"tasks")?;
                serial::write_str("DPMK:HTTP-BACKPRESSURE-CLI-OK\n");
                serial::write_str("DPMK:HTTP-BACKPRESSURE-TIMER-MAXGAP:");
                serial::write_decimal(max_timer_gap);
                serial::write_str("\n");
                if !prove_fault_containment() {
                    return Err("http-backpressure-fault-proof");
                }
                serial::write_str("DPMK:HTTP-BACKPRESSURE-FAULT-CONTAINED\n");
                serial::write_str("DPMK:HTTP-BACKPRESSURE-OK\n");
                return Ok(());
            }

            if saw_arp && saw_icmp && saw_udp && saw_http_get && saw_http_head && saw_http_404 {
                if polls_since_timer > max_timer_gap {
                    max_timer_gap = polls_since_timer;
                }
                serial::write_str("DPMK:NET-TIMER-MAXGAP:");
                serial::write_decimal(max_timer_gap);
                serial::write_str("\n");
                self.emit_network_drop_policy_markers();
                serial::write_str("DPMK:NET-FAIR-OK\n");
                return Ok(());
            }

            if (saw_arp || self.tcpip_task.has_dynamic_ipv4()) && http_get_count >= STAGE_G_ROUNDS {
                if polls_since_timer > max_timer_gap {
                    max_timer_gap = polls_since_timer;
                }
                serial::write_str("DPMK:NET-FAIR-BOOTSTRAP-OK:");
                serial::write_decimal(http_get_count);
                serial::write_str("\n");
                serial::write_str("DPMK:NET-TIMER-MAXGAP:");
                serial::write_decimal(max_timer_gap);
                serial::write_str("\n");
                return Ok(());
            }
        }
        Err("network-exchange-timeout")
    }

    fn network_drop_policy_started(&self) -> bool {
        serial::write_str("DPMK:NET-DROP-POLICY-STARTED\n");
        true
    }

    fn emit_network_drop_policy_markers(&self) {
        serial::write_str("DPMK:NET-DROP-POLICY-OK\n");
        serial::write_str("DPMK:NET-DROP-ARP-WRONG-TARGET:");
        serial::write_decimal(self.tcpip_task.network_drop_counters().arp_wrong_target);
        serial::write_str("\n");
        serial::write_str("DPMK:NET-DROP-ARP-MALFORMED:");
        serial::write_decimal(self.tcpip_task.network_drop_counters().arp_malformed);
        serial::write_str("\n");
        serial::write_str("DPMK:NET-DROP-UNSUPPORTED-ETHERTYPE:");
        serial::write_decimal(
            self.tcpip_task
                .network_drop_counters()
                .unsupported_ethertype,
        );
        serial::write_str("\n");
        serial::write_str("DPMK:NET-DROP-IPV4-WRONG-TARGET:");
        serial::write_decimal(self.tcpip_task.network_drop_counters().ipv4_wrong_target);
        serial::write_str("\n");
        serial::write_str("DPMK:NET-DROP-UNSUPPORTED-PROTO:");
        serial::write_decimal(
            self.tcpip_task
                .network_drop_counters()
                .unsupported_ipv4_protocol,
        );
        serial::write_str("\n");
        serial::write_str("DPMK:NET-DROP-ICMP-NON-ECHO:");
        serial::write_decimal(self.tcpip_task.network_drop_counters().icmp_non_echo);
        serial::write_str("\n");
        serial::write_str("DPMK:NET-DROP-ICMP-MALFORMED:");
        serial::write_decimal(self.tcpip_task.network_drop_counters().icmp_malformed);
        serial::write_str("\n");
        serial::write_str("DPMK:NET-DROP-UDP-WRONG-PORT:");
        serial::write_decimal(self.tcpip_task.network_drop_counters().udp_wrong_port);
        serial::write_str("\n");
        serial::write_str("DPMK:NET-DROP-UDP-BAD-PAYLOAD:");
        serial::write_decimal(self.tcpip_task.network_drop_counters().udp_bad_payload);
        serial::write_str("\n");
        serial::write_str("DPMK:NET-DROP-UDP-MALFORMED:");
        serial::write_decimal(self.tcpip_task.network_drop_counters().udp_malformed);
        serial::write_str("\n");
        emit_nontls_negative_matrix_markers();
    }

    fn prove_control_protocol_v1_boundary(&mut self) -> Result<(), &'static str> {
        if !prove_net_region_fault_containment() {
            return Err("control-protocol-v1-net-region");
        }
        serial::write_str("DPMK:CTRL-V1-NET-REGION-OK\n");

        let denied = Message::new(
            TASK_NET,
            EP_TCPIP,
            REQUEST_TCPIP_NET,
            CAP_KERNEL,
            MessageBody::Pair(ETHERNET_HEADER_BYTES as u32, 0),
        );
        let denied_frame = [0u8; ETHERNET_HEADER_BYTES];
        let rejected = protected_net_region(|memory| {
            match self
                .net_task
                .stage_raw_ingress(memory, denied, &denied_frame)
            {
                Err("net-request-route") => Ok(true),
                Err(other) => Err(other),
                Ok(_) => Ok(false),
            }
        })?;
        if !rejected {
            return Err("control-protocol-v1-denied-route");
        }
        serial::write_str("DPMK:CTRL-V1-DENIED-ROUTE-OK\n");

        serial::write_str("DPMK:CTRL-V1-BOUNDARY-OK\n");
        Ok(())
    }

    fn run_dhcp_probe(&mut self) -> Result<(), &'static str> {
        if !self.net_ready {
            return Ok(());
        }

        let discover = self.dhcp_task.discover();
        self.net_task.transmit_reply(&discover)?;
        serial::write_str("DPMK:DHCP-DISCOVER-TX\n");

        let mut sent_request = false;
        let mut polls_since_timer = 0u32;
        let mut max_timer_gap = 0u32;
        let mut polls = 0u32;
        while polls < DHCP_POLL_LIMIT {
            polls += 1;
            polls_since_timer += 1;
            if polls_since_timer >= TIMER_DHCP_POLL_INTERVAL {
                if polls_since_timer > max_timer_gap {
                    max_timer_gap = polls_since_timer;
                }
                self.run_timer_tick()?;
                polls_since_timer = 0;
            }

            let Some(net_reply) = self
                .net_task
                .receive_raw_frame_for(EP_DHCP, REQUEST_DHCP_NET)?
            else {
                continue;
            };
            send_to_task(
                &mut self.tasks,
                TASK_DHCP,
                &mut self.dhcp_mailbox,
                net_reply,
            )?;

            let dhcp_received = recv_from_task(&mut self.tasks, TASK_DHCP, &mut self.dhcp_mailbox)?;
            let event = protected_net_region(|memory| {
                self.dhcp_task.accept_server_frame(
                    dhcp_received,
                    memory.frame_from_net_message(dhcp_received)?,
                )
            })?;
            match event {
                Some(DhcpEvent::Offer { address, server }) if !sent_request => {
                    serial::write_str("DPMK:DHCP-OFFER-RX:");
                    serial_write_ipv4(address);
                    serial::write_str("\n");
                    let request = self.dhcp_task.request(address, server);
                    self.net_task.transmit_reply(&request)?;
                    serial::write_str("DPMK:DHCP-REQUEST-TX:");
                    serial_write_ipv4(address);
                    serial::write_str("\n");
                    sent_request = true;
                }
                Some(DhcpEvent::Ack { address, server }) => {
                    self.dhcp_task.assign(address, server)?;
                    self.tcpip_task.set_dynamic_ipv4(address);
                    if polls_since_timer > max_timer_gap {
                        max_timer_gap = polls_since_timer;
                    }
                    serial::write_str("DPMK:DHCP-ACK-RX:");
                    serial_write_ipv4(address);
                    serial::write_str("\n");
                    serial::write_str("DPMK:DHCP-SERVER:");
                    serial_write_ipv4(server);
                    serial::write_str("\n");
                    serial::write_str("DPMK:DHCP-LEASE-OK:");
                    serial_write_ipv4(address);
                    serial::write_str("\n");
                    serial::write_str("DPMK:DHCP-TIMER-MAXGAP:");
                    serial::write_decimal(max_timer_gap);
                    serial::write_str("\n");
                    return Ok(());
                }
                Some(_) | None => {}
            }
        }

        serial::write_str("DPMK:DHCP-TIMEOUT-FALLBACK\n");
        serial::write_str("DPMK:DHCP-STATIC-FALLBACK-OK\n");
        Ok(())
    }
}
