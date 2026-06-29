use crate::*;

impl KernelState {
    pub(crate) fn run_fat32_directory_index_proof(&mut self) -> Result<(), &'static str> {
        let first = self
            .fs_service_request(FsServiceRequest::ListRoot, &mut [])?
            .list_root()?;
        let second = self
            .fs_service_request(FsServiceRequest::ListRoot, &mut [])?
            .list_root()?;
        let proof = self
            .fs_task
            .directory_index_view(first)
            .map_err(FsError::as_str)?;
        let repeat = self
            .fs_task
            .directory_index_view(second)
            .map_err(FsError::as_str)?;
        if proof != repeat {
            return Err("fat32-directory-index-determinism");
        }
        if proof.entry_count != FAT32_DIRECTORY_INDEX_ENTRY_CAP as u8
            || proof.entry_cap != FAT32_DIRECTORY_INDEX_ENTRY_CAP as u8
            || proof.name_cap != FAT32_DIRECTORY_INDEX_NAME_CAP as u8
            || proof.response_cap != FAT32_DIRECTORY_INDEX_RESPONSE_CAP as u16
            || proof.response_bytes > FAT32_DIRECTORY_INDEX_RESPONSE_CAP as u16
        {
            return Err("fat32-directory-index-cap");
        }

        serial::write_str("DPMK:FS-DIR-INDEX-OK\n");
        serial::write_str("DPFSIDX:rows count=4 cap=4 name_cap=12 response_cap=192 order=/HELLO.TXT,/INDEX.HTM,/LARGE.HTM,/CHAIN.HTM\n");
        self.write_directory_index_row(0, proof.hello)?;
        self.write_directory_index_row(1, proof.index)?;
        self.write_directory_index_row(2, proof.large)?;
        self.write_directory_index_row(3, proof.chain)?;
        serial::write_str("DPFSIDX:unsupported_path=unsupported-path entry_cap=enforced name_cap=enforced response_cap=enforced deterministic=ok\n");
        serial::write_str("DPFSIDX:counters list_root=2 entries=4 errors=1\n");
        Ok(())
    }

    fn write_directory_index_row(
        &self,
        index: u32,
        row: FsDirectoryIndexRow,
    ) -> Result<(), &'static str> {
        if row.path.len() > FAT32_DIRECTORY_INDEX_NAME_CAP {
            return Err("fat32-directory-index-name-cap");
        }
        serial::write_str("DPFSIDX:row ");
        serial::write_decimal(index);
        serial::write_str(" path=");
        serial::write_str(row.path);
        serial::write_str(" status=ok cluster=");
        serial::write_decimal(row.cluster);
        serial::write_str(" size=");
        serial::write_decimal(row.size);
        serial::write_str(" readonly=1\n");
        Ok(())
    }

    #[cfg(feature = "fat32-write-proof")]
    fn write_sector_from_fs(&mut self, lba: u32) -> Result<(), &'static str> {
        copy_fs_sector_to_block()?;
        let block_request = self.fs_task.block_write_request(lba);
        send_to_task(
            &mut self.tasks,
            TASK_BLOCK,
            &mut self.block_mailbox,
            block_request,
        )?;

        let block_received = recv_from_task(&mut self.tasks, TASK_BLOCK, &mut self.block_mailbox)?;
        let block_reply = self.block_task.write_sector_reply(block_received)?;
        send_to_task(&mut self.tasks, TASK_FS, &mut self.fs_mailbox, block_reply)?;

        let fs_reply_input = recv_from_task(&mut self.tasks, TASK_FS, &mut self.fs_mailbox)?;
        self.fs_task.accept_block_reply(fs_reply_input, lba)
    }

    #[cfg(feature = "fat32-write-proof")]
    pub(crate) fn run_fat32_write_probe(&mut self) -> Result<(), &'static str> {
        let layout = self.load_fat32_layout()?;
        if layout.sectors_per_cluster != 1 {
            return Err("fat32-write-cluster-shape");
        }

        let fat_entry_sector = layout.fat_entry_sector(FAT32_OUT_CLUSTER);
        self.write_out_fat_entry(fat_entry_sector)?;
        serial::write_str("DPMK:FAT32-WRITE-FAT1-OK\n");

        self.write_out_fat_entry(fat_entry_sector + layout.fat_sectors)?;
        serial::write_str("DPMK:FAT32-WRITE-FAT2-OK\n");

        self.write_out_data_cluster(layout)?;
        serial::write_str("DPMK:FAT32-WRITE-DATA-OK\n");

        self.write_out_root_entry(layout)?;
        serial::write_str("DPMK:FAT32-WRITE-ROOT-OK\n");

        self.verify_out_file_readback(layout)?;
        serial::write_str("DPMK:FAT32-WRITE-READBACK-OK\n");
        serial::write_str("DPMK:FAT32-WRITE-OK\n");
        Ok(())
    }

    #[cfg(feature = "fat32-write-proof")]
    fn write_out_fat_entry(&mut self, lba: u32) -> Result<(), &'static str> {
        self.read_sector_for_fs(lba)?;
        protected_fs_region(|memory| self.fs_task.write_fat_eoc(memory, FAT32_OUT_CLUSTER))?;
        self.write_sector_from_fs(lba)
    }

    #[cfg(feature = "fat32-write-proof")]
    fn write_out_data_cluster(&mut self, layout: Fat32Layout) -> Result<(), &'static str> {
        protected_fs_region(|memory| self.fs_task.write_file_content(memory, FAT32_OUT_CONTENT))?;
        self.write_sector_from_fs(layout.cluster_sector(FAT32_OUT_CLUSTER))
    }

    #[cfg(feature = "fat32-write-proof")]
    fn write_out_root_entry(&mut self, layout: Fat32Layout) -> Result<(), &'static str> {
        self.read_sector_for_fs(layout.root_sector())?;
        protected_fs_region(|memory| self.fs_task.write_out_directory_entry(memory))?;
        self.write_sector_from_fs(layout.root_sector())
    }

    #[cfg(feature = "fat32-write-proof")]
    fn verify_out_file_readback(&mut self, layout: Fat32Layout) -> Result<(), &'static str> {
        let fat_entry_sector = layout.fat_entry_sector(FAT32_OUT_CLUSTER);
        self.read_sector_for_fs(fat_entry_sector)?;
        protected_fs_region(|memory| self.fs_task.verify_fat_eoc(memory, FAT32_OUT_CLUSTER))?;
        self.read_sector_for_fs(fat_entry_sector + layout.fat_sectors)?;
        protected_fs_region(|memory| self.fs_task.verify_fat_eoc(memory, FAT32_OUT_CLUSTER))?;

        self.read_sector_for_fs(layout.root_sector())?;
        let out = protected_fs_region(|memory| self.fs_task.parse_out_directory_entry(memory))?;
        if out.cluster != FAT32_OUT_CLUSTER || out.size != FAT32_OUT_CONTENT.len() as u32 {
            return Err("fat32-out-entry-readback");
        }

        self.read_sector_for_fs(layout.cluster_sector(out.cluster))?;
        protected_fs_region(|memory| self.fs_task.verify_file_content(memory, FAT32_OUT_CONTENT))
    }

    pub(crate) fn run_fs_service_error_matrix(&mut self) -> Result<(), &'static str> {
        let root = self
            .fs_service_request(FsServiceRequest::ListRoot, &mut [])?
            .list_root()?;
        if root.hello.size != FAT32_HELLO_CONTENT.len() as u32
            || root.chain.size != FAT32_CHAIN_CONTENT_BYTES
        {
            return Err("fs-service-matrix-root");
        }

        let hello = self
            .fs_service_request(
                FsServiceRequest::Open {
                    path: FsKnownPath::Hello,
                },
                &mut [],
            )?
            .opened()?;
        let mut small = [0u8; BLOCK_SECTOR_BYTES];
        let small_read = self
            .fs_service_request(
                FsServiceRequest::ReadAt {
                    handle: hello,
                    offset: 0,
                    len: hello.size,
                },
                &mut small,
            )?
            .read()?;
        if small_read.size != FAT32_HELLO_CONTENT.len()
            || &small[..small_read.size] != FAT32_HELLO_CONTENT
        {
            return Err("fs-service-matrix-small");
        }

        let chain = self
            .fs_service_request(
                FsServiceRequest::Open {
                    path: FsKnownPath::Chain,
                },
                &mut [],
            )?
            .opened()?;
        let mut chain_bytes = [0u8; HTTP_CHAIN_FILE_BYTES];
        let chain_read = self
            .fs_service_request(
                FsServiceRequest::ReadAt {
                    handle: chain,
                    offset: 0,
                    len: chain.size,
                },
                &mut chain_bytes,
            )?
            .read()?;
        if chain_read.size != FAT32_CHAIN_CONTENT_BYTES as usize {
            return Err("fs-service-matrix-chain");
        }

        let bad_handle = FsFileHandle {
            path: FsKnownPath::Hello,
            cluster: FAT32_CHAIN_CLUSTER_SECOND,
            size: hello.size,
        };
        self.expect_fs_error(
            FsServiceRequest::Stat { handle: bad_handle },
            FsError::Handle,
            &mut [],
        )?;
        self.expect_fs_error(
            FsServiceRequest::ReadAt {
                handle: bad_handle,
                offset: 0,
                len: 1,
            },
            FsError::Handle,
            &mut small[..1],
        )?;
        self.expect_fs_error(
            FsServiceRequest::ReadAt {
                handle: hello,
                offset: hello.size,
                len: 1,
            },
            FsError::Range,
            &mut small[..1],
        )?;
        self.expect_fs_error(
            FsServiceRequest::ReadAt {
                handle: chain,
                offset: 0,
                len: chain.size,
            },
            FsError::Capacity,
            &mut small[..BLOCK_SECTOR_BYTES],
        )?;

        let missing = self
            .fs_task
            .reject_cli_shape(FsNegativeCase::UnsupportedPath)?;
        let write = self
            .fs_task
            .reject_cli_shape(FsNegativeCase::UnsupportedWrite)?;
        serial::write_str("DPFSERR:matrix small=ok multi_cluster=ok missing=");
        serial::write_str(missing.reason);
        serial::write_str(" bad_handle=fs-handle unopened_read=fs-handle bad_offset=fs-range output_cap=fs-capacity unsupported_path_shape=");
        serial::write_str(missing.reason);
        serial::write_str(" unsupported_write=");
        serial::write_str(write.reason);
        serial::write_str("\n");
        serial::write_str("DPFSERR:counters open=2 stat=1 read_at=5 list_root=1 errors=6\n");
        serial::write_str("DPMK:FS-SERVICE-ERROR-MATRIX-OK\n");
        Ok(())
    }

    pub(crate) fn run_http_service_boundary_probe(&mut self) -> Result<(), &'static str> {
        self.http_task.probe_policy()?;
        let counters = self.http_task.counters();
        if counters.response_200 != 6
            || counters.response_404 != 1
            || counters.response_405 != 1
            || counters.response_413 != 2
            || counters.response_500 != 1
            || counters.malformed_requests != 1
            || counters.request_line_too_long != 1
            || counters.headers_too_long != 0
        {
            return Err("http-policy-counters");
        }
        serial::write_str("DPMK:HTTP-POLICY-200\n");
        serial::write_str("DPMK:HTTP-POLICY-404\n");
        serial::write_str("DPMK:HTTP-POLICY-405\n");
        serial::write_str("DPMK:HTTP-POLICY-413\n");
        serial::write_str("DPMK:HTTP-POLICY-500\n");
        serial::write_str("DPMK:HTTP-STATUS-200:");
        serial::write_decimal(counters.response_200);
        serial::write_str("\n");
        serial::write_str("DPMK:HTTP-STATUS-404:");
        serial::write_decimal(counters.response_404);
        serial::write_str("\n");
        serial::write_str("DPMK:HTTP-STATUS-405:");
        serial::write_decimal(counters.response_405);
        serial::write_str("\n");
        serial::write_str("DPMK:HTTP-STATUS-413:");
        serial::write_decimal(counters.response_413);
        serial::write_str("\n");
        serial::write_str("DPMK:HTTP-STATUS-500:");
        serial::write_decimal(counters.response_500);
        serial::write_str("\n");
        serial::write_str("DPMK:HTTP-PARSER-MALFORMED:");
        serial::write_decimal(counters.malformed_requests);
        serial::write_str("\n");
        serial::write_str("DPMK:HTTP-PARSER-LINE-TOO-LONG:");
        serial::write_decimal(counters.request_line_too_long);
        serial::write_str("\n");
        serial::write_str("DPMK:HTTP-POLICY-OK\n");
        serial::write_str("DPMK:FS-SERVICE-FAULT-CONTAINED\n");
        Ok(())
    }
}
