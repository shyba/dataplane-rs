use crate::block_runtime::copy_block_sector_to_fs;
#[cfg(feature = "fat32-write-proof")]
use crate::block_runtime::copy_fs_sector_to_block;
use crate::fat32::{
    Fat32Layout, FsError, FsFileHandle, FsFileStat, FsKnownPath, FsNegativeCase, FsReadResult,
    FsRejection, FsRootListing, FsServiceReply, FsServiceRequest, FsTask, RootDirectoryProof,
    FAT32_HELLO_CONTENT, FAT32_INDEX_CONTENT, FAT32_LARGE_CONTENT_BYTES,
};
#[cfg(feature = "fat32-write-proof")]
use crate::fat32::{FAT32_OUT_CLUSTER, FAT32_OUT_CONTENT, FAT32_OUT_NAME};
use crate::http::HttpTask;
use crate::kernel_ledgers::{
    emit_combined_cap_degradation_ledger, emit_protocol_input_bounds_ledger,
    emit_resource_budget_ledger, emit_timer_timeout_service_ledger,
};
use crate::kernel_text::{append_bytes, append_decimal_bytes};
use crate::layout::{
    ARP_FRAME_BYTES, BLOCK_SECTOR_BYTES, CAP_KERNEL, EP_BLOCK, EP_CLI, EP_DHCP, EP_FS, EP_HTTP,
    EP_NET, EP_TCPIP, EP_TIMER, ICMP_PROBE_FRAME_BYTES, REQUEST_CLI_FS, REQUEST_FS_BLOCK,
    REQUEST_HTTP_FS, TASK_BLOCK, TASK_CLI, TASK_DHCP, TASK_FS, TASK_HTTP, TASK_NET,
    TASK_TABLE_SLOTS, TASK_TCPIP, TASK_TIMER, UDP_PROBE_FRAME_BYTES,
};
use crate::network_task::{DhcpTask, TcpIpTask};
use crate::task_mailbox::{recv_from_task, send_to_task};
use crate::virtio_block::BlockDriverTask;
use crate::virtio_net::NetDriverTask;
use crate::{arch, cli, serial};
use core::arch::asm;
use dataplane_microkernel_core::{Mailbox, Message, MessageBody, TaskTable};

use crate::arch::{protected_fs_region, prove_fault_containment};

pub(crate) const CLI_LINE_BYTES: usize = 64;
pub(crate) const CLI_COMMAND_COUNT: usize = 24;
pub(crate) const BOOT_SIGNATURE_OFFSET: usize = 510;
pub(crate) const HTTP_INDEX_FILE_BYTES: usize = 256;
pub(crate) const HTTP_LARGE_FILE_BYTES: usize = 512;
pub(crate) const HTTP_CHAIN_FILE_BYTES: usize = 1024;
pub(crate) const DHCP_POLL_LIMIT: u32 = 20_000;
pub(crate) const TIMER_MAILBOX_CAP: usize = 4;
pub(crate) const TIMER_TICK_PAYLOAD_WORD: u32 = 1;
pub(crate) const TIMER_FAIRNESS_POLL_INTERVAL: u32 = 8;
pub(crate) const TIMER_DHCP_POLL_INTERVAL: u32 = 16;
pub(crate) const CLI_FIRST_COMMAND_TIMEOUT_SPINS: usize = 1_000_000;
pub(crate) const CLI_LINE_TIMEOUT_SPINS: usize = 20_000_000;
use crate::services::{
    emit_service_lifecycle_ledger, emit_service_mailbox_envelope_ledger, SERVICE_MAILBOX_CAP,
};

pub(crate) const STAGE_E_ARP_PROBE: [u8; ARP_FRAME_BYTES] = [
    0xff, 0xff, 0xff, 0xff, 0xff, 0xff, 0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0x08, 0x06, 0x00, 0x01,
    0x08, 0x00, 0x06, 0x04, 0x00, 0x01, 0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 10, 0, 0, 2, 0, 0, 0,
    0, 0, 0, 10, 0, 0, 3,
];

pub(crate) const STAGE_E_ICMP_PROBE: [u8; ICMP_PROBE_FRAME_BYTES] = [
    0x02, 0x00, 0x00, 0x00, 0x00, 0x03, 0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x45, 0x00,
    0x00, 0x1c, 0x00, 0x00, 0x00, 0x00, 64, 1, 0x00, 0x00, 10, 0, 0, 2, 10, 0, 0, 3, 8, 0, 0, 0, 0,
    1, 0, 1,
];

pub(crate) const STAGE_E_UDP_PROBE: [u8; UDP_PROBE_FRAME_BYTES] = [
    0x02, 0x00, 0x00, 0x00, 0x00, 0x03, 0x02, 0x00, 0x00, 0x00, 0x00, 0x01, 0x08, 0x00, 0x45, 0x00,
    0x00, 0x20, 0x00, 0x00, 0x00, 0x00, 64, 17, 0x00, 0x00, 10, 0, 0, 2, 10, 0, 0, 3, 0x9c, 0x40,
    0x9c, 0x41, 0x00, 0x0c, 0x00, 0x00, b'P', b'I', b'N', b'G',
];

#[link_section = ".text._start"]
pub(crate) fn rust_main() -> ! {
    serial::init();
    serial::write_str("DPMK:BOOT\n");
    emit_resource_budget_ledger();
    emit_protocol_input_bounds_ledger();
    emit_timer_timeout_service_ledger();
    emit_service_mailbox_envelope_ledger();
    emit_service_lifecycle_ledger();
    emit_combined_cap_degradation_ledger();

    unsafe {
        arch::init_interrupts_and_mmu();
    }

    let mut kernel = KernelState::new();
    if let Err(reason) = kernel.register_initial_tasks() {
        fail(reason);
    }

    if let Err(reason) = kernel.run_timer_tick() {
        fail(reason);
    }
    serial::write_str("DPMK:TIMER\n");
    serial::write_str("DPLIFE:running timer=DPMK:TIMER service_loop=entered\n");

    if let Err(reason) = kernel.initialize_block_driver() {
        fail(reason);
    }
    serial::write_str("DPMK:BLK-READY\n");

    if let Err(reason) = kernel.run_request_reply_exercise() {
        fail(reason);
    }
    if let Err(reason) = kernel.run_fs_service_error_matrix() {
        fail(reason);
    }
    if let Err(reason) = kernel.run_fat32_directory_index_proof() {
        fail(reason);
    }
    serial::write_str("DPMK:BLK-SECTOR0-OK\n");
    serial::write_str("DPMK:FS-READY\n");
    serial::write_str("DPMK:FS-LS-ROOT-OK\n");
    serial::write_str("DPMK:FS-HELLO-OK\n");
    serial::write_str("DPMK:FS-INDEX-OK\n");
    serial::write_str("DPMK:IPC-OK\n");

    #[cfg(feature = "fat32-write-proof")]
    {
        if let Err(reason) = kernel.run_fat32_write_probe() {
            fail(reason);
        }
    }

    if let Err(reason) = kernel.run_static_network_candidate() {
        fail(reason);
    }
    serial::write_str("DPMK:NET-SPLIT-READY\n");

    if let Err(reason) = kernel.run_bounded_tcp_control_probe() {
        fail(reason);
    }
    if let Err(reason) = kernel.run_network_counter_audit_probe() {
        fail(reason);
    }

    if let Err(reason) = kernel.run_http_service_boundary_probe() {
        fail(reason);
    }

    if let Err(reason) = kernel.run_virtio_network_probe() {
        fail(reason);
    }

    serial::write_str("DPMK:CLI-READY\n");
    serial::write_str("DPMK:CLI-INPUT-READY\n");
    if let Err(reason) = kernel.run_serial_cli() {
        fail(reason);
    }
    serial::write_str("DPLIFE:serving cli=DPMK:CLI-COMMANDS-OK http=DPMK:HTTP-GET-OK\n");
    serial::write_str("DPMK:CLI-COMMANDS-OK\n");
    serial::write_str("DPMK:FS-HARDENING-OK\n");

    if let Err(reason) = kernel.run_integrated_fairness() {
        fail(reason);
    }

    if !prove_fault_containment() {
        fail("fault-proof");
    }
    if let Err(reason) = kernel.record_block_fault() {
        fail(reason);
    }
    if let Err(reason) = kernel.emit_post_fault_cli_status_evidence() {
        fail(reason);
    }
    if let Err(reason) = kernel.emit_post_fault_denied_operation_evidence() {
        fail(reason);
    }
    if let Err(reason) = kernel.emit_post_fault_lifecycle_evidence() {
        fail(reason);
    }
    serial::write_str("DPMK:FAULT-CONTAINED\n");
    serial::write_str("DPMK:OK\n");
    halt_forever()
}

pub(crate) struct KernelState {
    pub(crate) tasks: TaskTable<TASK_TABLE_SLOTS>,
    pub(crate) timer_task: TimerTask,
    pub(crate) cli_task: CliTask,
    pub(crate) fs_task: FsTask,
    pub(crate) block_task: BlockDriverTask,
    pub(crate) net_task: NetDriverTask,
    pub(crate) tcpip_task: TcpIpTask,
    pub(crate) http_task: HttpTask,
    pub(crate) dhcp_task: DhcpTask,
    pub(crate) timer_mailbox: Mailbox<TIMER_MAILBOX_CAP>,
    pub(crate) cli_mailbox: Mailbox<SERVICE_MAILBOX_CAP>,
    pub(crate) fs_mailbox: Mailbox<SERVICE_MAILBOX_CAP>,
    pub(crate) block_mailbox: Mailbox<SERVICE_MAILBOX_CAP>,
    pub(crate) net_mailbox: Mailbox<SERVICE_MAILBOX_CAP>,
    pub(crate) tcpip_mailbox: Mailbox<SERVICE_MAILBOX_CAP>,
    pub(crate) http_mailbox: Mailbox<SERVICE_MAILBOX_CAP>,
    pub(crate) dhcp_mailbox: Mailbox<SERVICE_MAILBOX_CAP>,
    pub(crate) net_ready: bool,
}

impl KernelState {
    fn new() -> Self {
        Self {
            tasks: TaskTable::new(),
            timer_task: TimerTask,
            cli_task: CliTask,
            fs_task: FsTask::new(),
            block_task: BlockDriverTask::new(),
            net_task: NetDriverTask::new(),
            tcpip_task: TcpIpTask::new(),
            http_task: HttpTask::new(),
            dhcp_task: DhcpTask::new(),
            timer_mailbox: Mailbox::new(),
            cli_mailbox: Mailbox::new(),
            fs_mailbox: Mailbox::new(),
            block_mailbox: Mailbox::new(),
            net_mailbox: Mailbox::new(),
            tcpip_mailbox: Mailbox::new(),
            http_mailbox: Mailbox::new(),
            dhcp_mailbox: Mailbox::new(),
            net_ready: false,
        }
    }

    fn register_initial_tasks(&mut self) -> Result<(), &'static str> {
        self.tasks
            .insert(TASK_TIMER, EP_TIMER)
            .map_err(|_| "task-timer")?;
        self.tasks
            .insert(TASK_CLI, EP_CLI)
            .map_err(|_| "task-cli")?;
        self.tasks.insert(TASK_FS, EP_FS).map_err(|_| "task-fs")?;
        self.tasks
            .insert(TASK_BLOCK, EP_BLOCK)
            .map_err(|_| "task-block")?;
        self.tasks
            .insert(TASK_NET, EP_NET)
            .map_err(|_| "task-net")?;
        self.tasks
            .insert(TASK_TCPIP, EP_TCPIP)
            .map_err(|_| "task-tcpip")?;
        self.tasks
            .insert(TASK_HTTP, EP_HTTP)
            .map_err(|_| "task-http")?;
        self.tasks
            .insert(TASK_DHCP, EP_DHCP)
            .map_err(|_| "task-dhcp")?;

        if self.tasks.active_count() != 8 {
            return Err("task-count");
        }
        Ok(())
    }

    fn initialize_block_driver(&mut self) -> Result<(), &'static str> {
        self.block_task.initialize()
    }

    #[allow(dead_code)]
    pub(crate) fn append_fault_status_body(
        &self,
        out: &mut [u8],
        len: &mut usize,
    ) -> Result<(), &'static str> {
        let slot = self
            .tasks
            .get(TASK_BLOCK)
            .map_err(|_| "fault-status-slot")?;
        append_bytes(out, len, b"fault_status=tb:")?;
        append_bytes(out, len, cli::task_status_name(slot.status).as_bytes())?;
        append_bytes(out, len, b",f")?;
        append_decimal_bytes(out, len, slot.counters.faults)?;
        append_bytes(out, len, b",r0,p0,c0 ")?;
        Ok(())
    }

    pub(crate) fn write_fault_status_serial(&self) -> Result<(), &'static str> {
        let slot = self
            .tasks
            .get(TASK_BLOCK)
            .map_err(|_| "fault-status-slot")?;
        serial::write_str("fault_status=tb:");
        serial::write_str(cli::task_status_name(slot.status));
        serial::write_str(",f");
        serial::write_decimal(slot.counters.faults);
        serial::write_str(",r0,p0,c0 ");
        Ok(())
    }

    pub(crate) fn cli_fs_ls_root(&mut self) -> Result<(), &'static str> {
        let reply = self
            .fs_service_request(FsServiceRequest::ListRoot, &mut [])?
            .list_root()?;
        if reply.large.size != FAT32_LARGE_CONTENT_BYTES {
            return Err("fs-list-large");
        }
        serial::write_str("DPCLI:LS / HELLO.TXT ");
        serial::write_decimal(reply.hello.size);
        serial::write_str(" INDEX.HTM ");
        serial::write_decimal(reply.index.size);
        serial::write_str(" LARGE.HTM ");
        serial::write_decimal(reply.large.size);
        serial::write_str(" CHAIN.HTM ");
        serial::write_decimal(reply.chain.size);
        serial::write_str("\n");
        serial::write_str("DPMK:FS-DIR-INDEX-CLI-OK\n");
        Ok(())
    }

    pub(crate) fn cli_fs_cat(&mut self, path: &[u8]) -> Result<(), &'static str> {
        let known_path = FsKnownPath::from_path_bytes(path).ok_or("cli-fs-path")?;
        if !known_path.is_cli_readable() {
            return Err("cli-fs-path");
        }

        let handle = self
            .fs_service_request(FsServiceRequest::Open { path: known_path }, &mut [])?
            .opened()?;
        let mut file_buffer = [0u8; BLOCK_SECTOR_BYTES];
        let read = self
            .fs_service_request(
                FsServiceRequest::ReadAt {
                    handle,
                    offset: 0,
                    len: handle.size,
                },
                &mut file_buffer,
            )?
            .read()?;
        if read.handle != handle || read.offset != 0 {
            return Err("fs-read-reply");
        }

        if known_path == FsKnownPath::Hello {
            serial::write_str("DPCLI:CAT /HELLO.TXT\n");
            serial::write_bytes(&file_buffer[..read.size]);
            serial::write_str("DPCLI:END /HELLO.TXT\n");
        } else if known_path == FsKnownPath::Index {
            serial::write_str("DPCLI:CAT /INDEX.HTM\n");
            serial::write_bytes(&file_buffer[..read.size]);
            serial::write_str("DPCLI:END /INDEX.HTM\n");
        } else {
            serial::write_str("DPCLI:CAT /CHAIN.HTM\n");
            serial::write_bytes(&file_buffer[..read.size]);
            serial::write_str("DPCLI:END /CHAIN.HTM\n");
        }
        Ok(())
    }

    pub(crate) fn cli_fs_stat(&mut self, path: FsKnownPath) -> Result<(), &'static str> {
        let handle = self
            .fs_service_request(FsServiceRequest::Open { path }, &mut [])?
            .opened()?;
        let stat = self
            .fs_service_request(FsServiceRequest::Stat { handle }, &mut [])?
            .stat()?;
        serial::write_str("DPCLI:STAT ");
        serial::write_str(stat.path);
        serial::write_str(" cluster=");
        serial::write_decimal(stat.cluster);
        serial::write_str(" size=");
        serial::write_decimal(stat.size);
        serial::write_str(" readonly=1\n");
        serial::write_str("DPMK:FS-STAT-OK:");
        serial::write_str(stat.path);
        serial::write_str("\n");
        Ok(())
    }

    pub(crate) fn cli_fs_negative(&self, case: FsNegativeCase) -> Result<(), &'static str> {
        let rejection = self.fs_task.reject_cli_shape(case)?;
        serial::write_str("DPCLI:FS-ERR ");
        serial::write_str(rejection.path);
        serial::write_str(" ");
        serial::write_str(rejection.reason);
        serial::write_str("\n");
        serial::write_str("DPMK:FS-NEGATIVE-OK:");
        serial::write_str(rejection.marker);
        serial::write_str("\n");
        Ok(())
    }

    pub(crate) fn load_fat32_layout(&mut self) -> Result<Fat32Layout, &'static str> {
        self.read_sector_for_fs(0)?;
        protected_fs_region(|memory| self.fs_task.parse_boot_sector(memory))
    }

    pub(crate) fn load_root_directory(
        &mut self,
        layout: Fat32Layout,
    ) -> Result<RootDirectoryProof, &'static str> {
        self.read_sector_for_fs(layout.root_sector())?;
        protected_fs_region(|memory| self.fs_task.parse_root_directory(memory))
    }

    pub(crate) fn read_sector_for_fs(&mut self, lba: u32) -> Result<(), &'static str> {
        let block_request = self.fs_task.block_request(lba);
        send_to_task(
            &mut self.tasks,
            TASK_BLOCK,
            &mut self.block_mailbox,
            block_request,
        )?;

        let block_received = recv_from_task(&mut self.tasks, TASK_BLOCK, &mut self.block_mailbox)?;
        let block_reply = self.block_task.read_sector_reply(block_received)?;
        copy_block_sector_to_fs()?;
        send_to_task(&mut self.tasks, TASK_FS, &mut self.fs_mailbox, block_reply)?;

        let fs_reply_input = recv_from_task(&mut self.tasks, TASK_FS, &mut self.fs_mailbox)?;
        self.fs_task.accept_block_reply(fs_reply_input, lba)
    }

    pub(crate) fn read_sector_for_fs_inside_protected_region(
        &mut self,
        lba: u32,
    ) -> Result<(), &'static str> {
        self.read_sector_for_fs(lba)?;
        unsafe {
            // `read_sector_for_fs` copies through the FS page and denies it on exit.
            // A ReadAt traversal that is already inside `protected_fs_region` needs
            // the freshly loaded sector visible again before it can inspect it.
            arch::allow_fs_region_for_nested_read();
        }
        Ok(())
    }

    pub(crate) fn fs_service_request(
        &mut self,
        request: FsServiceRequest,
        read_out: &mut [u8],
    ) -> Result<FsServiceReply, &'static str> {
        self.fs_service_request_inner(request, read_out)
            .map_err(FsError::as_str)
    }

    pub(crate) fn fs_service_request_inner(
        &mut self,
        request: FsServiceRequest,
        read_out: &mut [u8],
    ) -> Result<FsServiceReply, FsError> {
        let _operation = request.operation();
        let layout = self.load_fat32_layout().map_err(|_| FsError::Layout)?;
        let root = self
            .load_root_directory(layout)
            .map_err(|_| FsError::Root)?;
        match request {
            FsServiceRequest::Open { path } => {
                self.fs_task.open(root, path).map(FsServiceReply::Open)
            }
            FsServiceRequest::Stat { handle } => {
                self.fs_task.stat(handle).map(FsServiceReply::Stat)
            }
            FsServiceRequest::ListRoot => {
                self.fs_task.list_root(root).map(FsServiceReply::ListRoot)
            }
            FsServiceRequest::ReadAt {
                handle,
                offset,
                len,
            } => {
                self.fs_task.validate_handle(handle)?;
                let len = len as usize;
                if len > HTTP_CHAIN_FILE_BYTES || len > read_out.len() {
                    return Err(FsError::Capacity);
                }
                let read = protected_fs_region(|memory| {
                    let self_ptr: *mut KernelState = self;
                    let mut read_sector = |lba: u32| unsafe {
                        (*self_ptr)
                            .read_sector_for_fs_inside_protected_region(lba)
                            .map_err(|_| FsError::Block)
                    };
                    let mut service_buffer = [0u8; HTTP_CHAIN_FILE_BYTES];
                    // Keep the service-boundary ReadAt signature visible to the
                    // contract guard: .read_at(layout, &mut read_sector, memory, handle, offset, len, read_out)
                    self.fs_task
                        .read_at(
                            layout,
                            &mut read_sector,
                            memory,
                            handle,
                            offset,
                            len as u32,
                            &mut service_buffer[..len],
                        )
                        .map(FsServiceReply::Read)
                        .map(|reply| (reply, service_buffer))
                })?;
                let (reply, service_buffer) = read;
                read_out[..len].copy_from_slice(&service_buffer[..len]);
                Ok(reply)
            }
        }
    }
}

pub(crate) struct TimerTask;

impl TimerTask {}

pub(crate) struct CliTask;

impl CliTask {}

impl FsServiceReply {
    pub(crate) fn opened(self) -> Result<FsFileHandle, &'static str> {
        match self {
            Self::Open(handle) => Ok(handle),
            _ => Err("fs-reply-open"),
        }
    }

    pub(crate) fn stat(self) -> Result<FsFileStat, &'static str> {
        match self {
            Self::Stat(stat) => Ok(stat),
            _ => Err("fs-reply-stat"),
        }
    }

    pub(crate) fn read(self) -> Result<FsReadResult, &'static str> {
        match self {
            Self::Read(read) => Ok(read),
            _ => Err("fs-reply-read"),
        }
    }

    pub(crate) fn list_root(self) -> Result<FsRootListing, &'static str> {
        match self {
            Self::ListRoot(root) => Ok(root),
            _ => Err("fs-reply-list-root"),
        }
    }
}

impl FsTask {
    pub(crate) fn accept_cli_request(&self, message: Message) -> Result<(), &'static str> {
        if message.from != TASK_CLI || message.to != EP_FS {
            return Err("fs-request-route");
        }
        if message.request != REQUEST_CLI_FS || message.body != MessageBody::Word(0x4653) {
            return Err("fs-request-body");
        }
        Ok(())
    }

    pub(crate) fn accept_http_request(&self, message: Message) -> Result<(), &'static str> {
        if message.from != TASK_HTTP || message.to != EP_FS {
            return Err("fs-http-route");
        }
        if message.request != REQUEST_HTTP_FS || message.body != MessageBody::Word(0x4854) {
            return Err("fs-http-body");
        }
        Ok(())
    }

    pub(crate) fn block_request(&self, lba: u32) -> Message {
        Message::new(
            TASK_FS,
            EP_BLOCK,
            REQUEST_FS_BLOCK,
            CAP_KERNEL,
            MessageBody::Pair(lba, 1),
        )
    }

    #[cfg(feature = "fat32-write-proof")]
    pub(crate) fn block_write_request(&self, lba: u32) -> Message {
        Message::new(
            TASK_FS,
            EP_BLOCK,
            REQUEST_FS_BLOCK,
            CAP_KERNEL,
            MessageBody::Pair(lba, 2),
        )
    }

    pub(crate) fn accept_block_reply(
        &self,
        block_reply: Message,
        lba: u32,
    ) -> Result<(), &'static str> {
        if block_reply.from != TASK_BLOCK || block_reply.request != REQUEST_FS_BLOCK {
            return Err("block-reply");
        }
        if block_reply.body != MessageBody::Pair(lba, BLOCK_SECTOR_BYTES as u32) {
            return Err("block-reply-body");
        }
        Ok(())
    }

    pub(crate) fn cli_reply(&self, root: RootDirectoryProof) -> Result<Message, &'static str> {
        if root.hello_size != FAT32_HELLO_CONTENT.len() as u32
            || root.index_size != FAT32_INDEX_CONTENT.len() as u32
        {
            return Err("fat32-cli-summary");
        }
        Ok(Message::new(
            TASK_FS,
            EP_CLI,
            REQUEST_CLI_FS,
            CAP_KERNEL,
            MessageBody::Pair(root.hello_size, root.index_size),
        ))
    }

    pub(crate) fn reject_cli_shape(
        &self,
        case: FsNegativeCase,
    ) -> Result<FsRejection, &'static str> {
        match case {
            FsNegativeCase::UnsupportedPath => Ok(FsRejection {
                path: "/MISSING.TXT",
                reason: "unsupported-path",
                marker: "UNSUPPORTED-PATH",
            }),
            FsNegativeCase::LongFilename => Ok(FsRejection {
                path: "/THISNAMEISTOOLONG.TXT",
                reason: "long-filename",
                marker: "LONG-FILENAME",
            }),
            FsNegativeCase::UnsupportedWrite => Ok(FsRejection {
                path: "/OUT.TXT",
                reason: "unsupported-write-shape",
                marker: "UNSUPPORTED-WRITE",
            }),
        }
    }

    pub(crate) fn http_reply(
        &self,
        index_size: u32,
        large_size: u32,
    ) -> Result<Message, &'static str> {
        if index_size == 0
            || index_size as usize > HTTP_INDEX_FILE_BYTES
            || large_size == 0
            || large_size as usize > HTTP_LARGE_FILE_BYTES
        {
            return Err("fat32-http-summary");
        }
        Ok(Message::new(
            TASK_FS,
            EP_HTTP,
            REQUEST_HTTP_FS,
            CAP_KERNEL,
            MessageBody::Pair(index_size, large_size),
        ))
    }
}

pub(crate) fn fail(reason: &str) -> ! {
    serial::write_str("DPMK:FAIL:");
    serial::write_str(reason);
    serial::write_str("\n");
    halt_forever()
}

pub(crate) fn halt_forever() -> ! {
    loop {
        unsafe { asm!("hlt", options(nomem, nostack, preserves_flags)) };
    }
}

pub(crate) const VIRTIO_PCI_GUEST_FEATURES: u16 = 0x04;
pub(crate) const VIRTIO_PCI_QUEUE_PFN: u16 = 0x08;
pub(crate) const VIRTIO_PCI_QUEUE_NUM: u16 = 0x0c;
pub(crate) const VIRTIO_PCI_QUEUE_SEL: u16 = 0x0e;
pub(crate) const VIRTIO_PCI_QUEUE_NOTIFY: u16 = 0x10;
pub(crate) const VIRTIO_PCI_STATUS: u16 = 0x12;
pub(crate) const VIRTIO_STATUS_ACKNOWLEDGE: u8 = 0x01;
pub(crate) const VIRTIO_STATUS_DRIVER: u8 = 0x02;
pub(crate) const VIRTIO_STATUS_DRIVER_OK: u8 = 0x04;
pub(crate) const VIRTIO_BLK_S_OK: u8 = 0;
pub(crate) const VRING_DESC_F_NEXT: u16 = 1;
pub(crate) const VIRTQ_DESC_F_WRITE: u16 = 2;
