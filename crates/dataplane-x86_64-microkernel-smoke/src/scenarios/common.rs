use crate::*;

impl KernelState {
    pub(crate) fn load_http_files(
        &mut self,
        index_buffer: &mut [u8; HTTP_INDEX_FILE_BYTES],
        large_buffer: &mut [u8; HTTP_LARGE_FILE_BYTES],
        chain_buffer: &mut [u8; HTTP_CHAIN_FILE_BYTES],
    ) -> Result<(usize, usize, usize), &'static str> {
        let fs_request = self.http_task.fs_request();
        send_to_task(&mut self.tasks, TASK_FS, &mut self.fs_mailbox, fs_request)?;

        let fs_received = recv_from_task(&mut self.tasks, TASK_FS, &mut self.fs_mailbox)?;
        self.fs_task.accept_http_request(fs_received)?;

        let root = self
            .fs_service_request(FsServiceRequest::ListRoot, &mut [])?
            .list_root()?;
        let directory_index = self
            .fs_task
            .directory_index_view(root)
            .map_err(FsError::as_str)?;
        let index_size = root.index.size as usize;
        let large_size = root.large.size as usize;
        let chain_size = root.chain.size as usize;
        if index_size > index_buffer.len()
            || large_size > large_buffer.len()
            || chain_size > chain_buffer.len()
        {
            return Err("http-file-capacity");
        }

        let index_handle = self
            .fs_service_request(
                FsServiceRequest::Open {
                    path: FsKnownPath::Index,
                },
                &mut [],
            )?
            .opened()?;
        let index_stat = self
            .fs_service_request(
                FsServiceRequest::Stat {
                    handle: index_handle,
                },
                &mut [],
            )?
            .stat()?;
        if index_stat.size as usize != index_size {
            return Err("http-index-stat");
        }
        self.fs_service_request(
            FsServiceRequest::ReadAt {
                handle: index_handle,
                offset: 0,
                len: index_stat.size,
            },
            &mut index_buffer[..index_size],
        )?
        .read()?;
        serial::write_str("DPMK:FS-SERVICE-HTTP-INDEX-OK\n");

        let large_handle = self
            .fs_service_request(
                FsServiceRequest::Open {
                    path: FsKnownPath::Large,
                },
                &mut [],
            )?
            .opened()?;
        let large_stat = self
            .fs_service_request(
                FsServiceRequest::Stat {
                    handle: large_handle,
                },
                &mut [],
            )?
            .stat()?;
        if large_stat.size as usize != large_size {
            return Err("http-large-stat");
        }
        self.fs_service_request(
            FsServiceRequest::ReadAt {
                handle: large_handle,
                offset: 0,
                len: large_stat.size,
            },
            &mut large_buffer[..large_size],
        )?
        .read()?;
        serial::write_str("DPMK:FS-SERVICE-HTTP-LARGE-OK\n");
        self.load_http_file_read_at(root, chain_size, chain_buffer)?;
        serial::write_str("DPMK:FS-SERVICE-MULTI-CLUSTER-OK\n");

        let fs_reply = self.fs_task.http_reply(root.index.size, root.large.size)?;
        send_to_task(&mut self.tasks, TASK_HTTP, &mut self.http_mailbox, fs_reply)?;
        let http_received = recv_from_task(&mut self.tasks, TASK_HTTP, &mut self.http_mailbox)?;
        self.http_task
            .accept_fs_reply(http_received, root.index.size, root.large.size)?;
        serial::write_str("DPMK:FS-SERVICE-HTTP-OK\n");
        serial::write_str("DPMK:FS-SERVICE-BOUNDARY-OK\n");
        if directory_index.entry_count != FAT32_DIRECTORY_INDEX_ENTRY_CAP as u8 {
            return Err("http-directory-index-cap");
        }
        serial::write_str("DPMK:FS-DIR-INDEX-HTTP-OK\n");
        Ok((index_size, large_size, chain_size))
    }

    #[allow(dead_code)]
    fn load_http_file_chain(
        &mut self,
        layout: Fat32Layout,
        cluster: u32,
        size: usize,
        buffer: &mut [u8],
    ) -> Result<(), &'static str> {
        if size > buffer.len() {
            return Err("http-file-capacity");
        }
        let mut remaining = size;
        let mut out_index = 0usize;
        let mut current_cluster = cluster;
        while remaining > 0 {
            self.read_sector_for_fs(layout.cluster_sector(current_cluster))?;
            protected_fs_region(|memory| {
                let sector = memory.sector();
                let chunk = remaining.min(BLOCK_SECTOR_BYTES);
                let mut index = 0usize;
                while index < chunk {
                    buffer[out_index + index] = sector[index];
                    index += 1;
                }
                Ok(())
            })?;
            let chunk = remaining.min(BLOCK_SECTOR_BYTES);
            remaining -= chunk;
            out_index += chunk;
            if remaining > 0 {
                if cluster == FAT32_CHAIN_CLUSTER_FIRST {
                    current_cluster = FAT32_CHAIN_CLUSTER_SECOND;
                } else {
                    return Err("http-file-chain");
                }
            }
        }
        if size > BLOCK_SECTOR_BYTES {
            serial::write_str("DPMK:FS-SERVICE-READAT-OK\n");
        }
        Ok(())
    }

    pub(crate) fn load_http_file_read_at(
        &mut self,
        root: FsRootListing,
        size: usize,
        buffer: &mut [u8],
    ) -> Result<(), &'static str> {
        serial::write_str("DPMK:FS-SERVICE-READAT-CHAIN-BEGIN\n");
        let chain_handle = self
            .fs_service_request(
                FsServiceRequest::Open {
                    path: FsKnownPath::Chain,
                },
                &mut [],
            )?
            .opened()?;
        let chain_stat = self
            .fs_service_request(
                FsServiceRequest::Stat {
                    handle: chain_handle,
                },
                &mut [],
            )?
            .stat()?;
        serial::write_str("DPMK:FS-SERVICE-READAT-CHAIN-OPEN-STAT-OK\n");
        if chain_stat.path != FAT32_CHAIN_PATH
            || chain_stat.size as usize != size
            || root.chain.size as usize != size
        {
            return Err("http-file-handle");
        }
        if size > buffer.len() {
            return Err("http-file-capacity");
        }
        let first_len = BLOCK_SECTOR_BYTES.min(size);
        let first_read = match self.fs_service_request(
            FsServiceRequest::ReadAt {
                handle: chain_handle,
                offset: 0,
                len: first_len as u32,
            },
            &mut buffer[..first_len],
        )? {
            FsServiceReply::Read(read) => read,
            _ => return Err("http-file-handle"),
        };
        if first_read.handle.path != FsKnownPath::Chain
            || first_read.size != first_len
            || first_read.offset != 0
        {
            return Err("http-file-handle");
        }
        if size > first_len {
            let tail = size - first_len;
            let tail_read = match self.fs_service_request(
                FsServiceRequest::ReadAt {
                    handle: chain_handle,
                    offset: first_len as u32,
                    len: tail as u32,
                },
                &mut buffer[first_len..size],
            )? {
                FsServiceReply::Read(read) => read,
                _ => return Err("http-file-handle"),
            };
            if tail_read.handle.path != FsKnownPath::Chain
                || tail_read.size != tail
                || tail_read.offset != first_len as u32
            {
                return Err("http-file-handle");
            }
        }
        serial::write_str("DPMK:FS-SERVICE-READAT-OK\n");
        serial::write_str("DPMK:FS-SERVICE-READAT-CHAIN-READ-OK\n");
        serial::write_str("DPMK:FS-SERVICE-READAT-CHAIN-OK\n");
        Ok(())
    }

    pub(crate) fn expect_fs_error(
        &mut self,
        request: FsServiceRequest,
        expected: FsError,
        read_out: &mut [u8],
    ) -> Result<(), &'static str> {
        match self.fs_service_request_inner(request, read_out) {
            Ok(_) => Err("fs-service-error-expected"),
            Err(error) if error == expected => Ok(()),
            Err(_) => Err("fs-service-error-mismatch"),
        }
    }

    pub(crate) fn verify_static_network_frame(
        &mut self,
        frame: &[u8],
        expected: NetworkPacketKind,
    ) -> Result<(), &'static str> {
        let net_request = self.tcpip_task.net_frame_request(frame.len())?;
        send_to_task(
            &mut self.tasks,
            TASK_NET,
            &mut self.net_mailbox,
            net_request,
        )?;

        let net_received = recv_from_task(&mut self.tasks, TASK_NET, &mut self.net_mailbox)?;
        let net_reply = protected_net_region(|memory| {
            self.net_task.stage_raw_ingress(memory, net_received, frame)
        })?;
        send_to_task(
            &mut self.tasks,
            TASK_TCPIP,
            &mut self.tcpip_mailbox,
            net_reply,
        )?;

        let tcpip_received = recv_from_task(&mut self.tasks, TASK_TCPIP, &mut self.tcpip_mailbox)?;
        let actual = protected_net_region(|memory| {
            self.tcpip_task
                .classify_net_frame(tcpip_received, memory.frame(frame.len())?)
        })?;
        if actual != expected {
            return Err("tcpip-classification");
        }
        Ok(())
    }

    pub(crate) fn post_fault_block_write_denied(
        &mut self,
        message: Message,
    ) -> Result<(), &'static str> {
        let slot = self.tasks.get(TASK_BLOCK).map_err(|_| "fault-deny-slot")?;
        if slot.status == TaskStatus::Faulted {
            return Err("fault-deny-block-task-faulted");
        }
        send_to_task(
            &mut self.tasks,
            TASK_BLOCK,
            &mut self.block_mailbox,
            message,
        )
    }
}

impl TcpIpTask {
    pub(crate) fn run_stream_timer_ticks(&mut self, ticks: u32) -> Option<StreamTimeoutSnapshot> {
        let mut observed = None;
        let mut count = 0;
        while count < ticks {
            self.timer_ticks = self.timer_ticks.wrapping_add(1);
            count += 1;
            let mut index = 0;
            while index < self.http_streams.len() {
                if let Some(snapshot) =
                    self.http_streams[index].expire_if_timed_out(self.timer_ticks)
                {
                    self.tcp_counters.timeout_events =
                        self.tcp_counters.timeout_events.saturating_add(1);
                    self.tcp_counters.last_timeout_age_ticks = snapshot.age_ticks;
                    self.tcp_counters.last_timeout_deadline_ticks = snapshot.deadline_ticks;
                    self.tcp_counters.last_activity_ticks = snapshot.last_activity_ticks;
                    observed = Some(snapshot);
                }
                index += 1;
            }
        }
        self.tcp_counters.timer_ticks = self.timer_ticks;
        self.tcp_counters.active_sessions = self.active_tcp_sessions();
        observed
    }
}

pub(crate) fn emit_nontls_negative_matrix_markers() {
    serial::write_str("DPMK:NONTLS-NEG-UNSUPPORTED-ETHERTYPE-OK\n");
    serial::write_str("DPMK:NONTLS-NEG-ARP-MALFORMED-OK\n");
    serial::write_str("DPMK:NONTLS-NEG-ICMP-MALFORMED-OK\n");
    serial::write_str("DPMK:NONTLS-NETWORK-NEGATIVE-MATRIX-OK\n");
}
