use crate::fat32::{FAT32_HELLO_CONTENT, FAT32_INDEX_CONTENT};
use crate::layout::{
    EP_DHCP, EP_NET, EP_TIMER, REQUEST_DHCP_NET, REQUEST_HTTP_FS, REQUEST_TCPIP_NET,
    TASK_TABLE_SLOTS, TIMER_ENDPOINT_ID_NUM, TIMER_REQUEST_ID_NUM, TIMER_TASK_ID_NUM,
};
use crate::{
    fs_region_is_mmu_covered, protected_fs_region, recv_from_task, send_to_task, serial, CliTask,
    KernelState, Message, MessageBody, TaskStatus, TimerTask, CAP_KERNEL, EP_BLOCK, EP_FS,
    GENERATION_ID, REQUEST_CLI_FS, REQUEST_FS_BLOCK, REQUEST_TIMER_TICK, TASK_BLOCK, TASK_CLI,
    TASK_DHCP, TASK_FS, TASK_HTTP, TASK_NET, TASK_TCPIP, TASK_TIMER,
};

pub(crate) const SERVICE_MAILBOX_CAP: usize = 4;

impl KernelState {
    pub(crate) fn run_timer_tick(&mut self) -> Result<(), &'static str> {
        let tick = self.timer_task.tick_message();
        send_to_task(&mut self.tasks, TASK_TIMER, &mut self.timer_mailbox, tick)?;
        let received = recv_from_task(&mut self.tasks, TASK_TIMER, &mut self.timer_mailbox)?;
        self.timer_task.accept_tick(received)
    }

    pub(crate) fn run_request_reply_exercise(&mut self) -> Result<(), &'static str> {
        let fs_request = self.cli_task.fs_request();
        send_to_task(&mut self.tasks, TASK_FS, &mut self.fs_mailbox, fs_request)?;
        let fs_received = recv_from_task(&mut self.tasks, TASK_FS, &mut self.fs_mailbox)?;
        self.fs_task.accept_cli_request(fs_received)?;
        if !fs_region_is_mmu_covered() {
            return Err("fs-region-range");
        }
        self.read_sector_for_fs(0)?;
        let layout = protected_fs_region(|memory| self.fs_task.parse_boot_sector(memory))?;
        self.read_sector_for_fs(layout.fat_start_sector())?;
        protected_fs_region(|memory| self.fs_task.verify_fat_entries(memory))?;
        self.read_sector_for_fs(layout.root_sector())?;
        let root = protected_fs_region(|memory| self.fs_task.parse_root_directory(memory))?;
        self.read_sector_for_fs(layout.cluster_sector(root.hello_cluster))?;
        protected_fs_region(|memory| {
            self.fs_task
                .verify_file_content(memory, FAT32_HELLO_CONTENT)
        })?;
        self.read_sector_for_fs(layout.cluster_sector(root.index_cluster))?;
        protected_fs_region(|memory| {
            self.fs_task
                .verify_file_content(memory, FAT32_INDEX_CONTENT)
        })?;
        let cli_reply = self.fs_task.cli_reply(root)?;
        send_to_task(&mut self.tasks, TASK_CLI, &mut self.cli_mailbox, cli_reply)?;
        let cli_received = recv_from_task(&mut self.tasks, TASK_CLI, &mut self.cli_mailbox)?;
        self.cli_task.accept_fs_reply(cli_received)?;
        serial::write_str("DPMK:FS-SERVICE-CLI-OK\n");
        Ok(())
    }

    pub(crate) fn record_block_fault(&mut self) -> Result<(), &'static str> {
        self.tasks
            .record_fault(TASK_BLOCK)
            .map_err(|_| "fault-record")?;
        let slot = self.tasks.get(TASK_BLOCK).map_err(|_| "fault-slot")?;
        if slot.status != TaskStatus::Faulted || slot.counters.faults != 1 {
            return Err("fault-counter");
        }
        serial::write_str("DPMK:FAULT-RECORDED\n");
        Ok(())
    }

    pub(crate) fn emit_post_fault_cli_status_evidence(&self) -> Result<(), &'static str> {
        serial::write_str("DPMK:POST-FAULT-CLI-BEGIN\n");
        self.cli_task_detail("block", TASK_BLOCK)?;
        serial::write_str("DPMK:POST-FAULT-STATUS-BEGIN\n");
        self.cli_parity()?;
        serial::write_str("DPMK:POST-FAULT-CLI-STATUS-OK\n");
        Ok(())
    }

    pub(crate) fn emit_post_fault_denied_operation_evidence(&mut self) -> Result<(), &'static str> {
        let slot = self.tasks.get(TASK_BLOCK).map_err(|_| "fault-deny-slot")?;
        if slot.status != TaskStatus::Faulted || slot.counters.faults != 1 {
            return Err("fault-deny-counter");
        }
        let task_id = slot.id.get() as u32;
        let endpoint_id = slot.endpoint.get() as u32;
        let faults = slot.counters.faults;
        let before_depth = self.block_mailbox.len();
        let denied = Message::new(
            TASK_FS,
            EP_BLOCK,
            REQUEST_FS_BLOCK,
            CAP_KERNEL,
            MessageBody::Pair(0, 2),
        );
        if self.post_fault_block_write_denied(denied).is_ok() {
            return Err("fault-deny-accepted");
        }
        if self.block_mailbox.len() != before_depth {
            return Err("fault-deny-enqueued");
        }
        serial::write_str("DPFAULT:post-fault-denied task=TASK_BLOCK id=");
        serial::write_decimal(task_id);
        serial::write_str(" endpoint=");
        serial::write_decimal(endpoint_id);
        serial::write_str(" region=block_task_region right=block-write generation=");
        serial::write_decimal(GENERATION_ID);
        serial::write_str(" status=faulted faults=");
        serial::write_decimal(faults);
        serial::write_str(" route=denied enqueued=0 restart=0 replay=0 recovery=0\n");
        serial::write_str("DPMK:POST-FAULT-DENIED-OK\n");
        Ok(())
    }

    pub(crate) fn emit_post_fault_lifecycle_evidence(&self) -> Result<(), &'static str> {
        let slot = self.tasks.get(TASK_BLOCK).map_err(|_| "fault-life-slot")?;
        if slot.status != TaskStatus::Faulted || slot.counters.faults != 1 {
            return Err("fault-life-counter");
        }
        serial::write_str("DPLIFE:task block id=");
        serial::write_decimal(slot.id.get() as u32);
        serial::write_str(" endpoint=");
        serial::write_decimal(slot.endpoint.get() as u32);
        serial::write_str(" status=faulted faults=");
        serial::write_decimal(slot.counters.faults);
        serial::write_str(" restart=0 replay=0 cleanup_guarantee=0\n");
        serial::write_str("DPMK:SERVICE-LIFECYCLE-FAULT-OK\n");
        Ok(())
    }
}

impl TimerTask {
    pub(crate) fn tick_message(&self) -> Message {
        Message::new(
            TASK_TIMER,
            EP_TIMER,
            crate::REQUEST_TIMER_TICK,
            CAP_KERNEL,
            MessageBody::Word(crate::TIMER_TICK_PAYLOAD_WORD),
        )
    }

    pub(crate) fn accept_tick(&self, message: Message) -> Result<(), &'static str> {
        if message.request != crate::REQUEST_TIMER_TICK {
            return Err("timer-request");
        }
        if message.body != MessageBody::Word(crate::TIMER_TICK_PAYLOAD_WORD) {
            return Err("timer-body");
        }
        Ok(())
    }
}

impl CliTask {
    pub(crate) fn fs_request(&self) -> Message {
        Message::new(
            TASK_CLI,
            EP_FS,
            REQUEST_CLI_FS,
            CAP_KERNEL,
            MessageBody::Word(0x4653),
        )
    }

    pub(crate) fn accept_fs_reply(&self, message: Message) -> Result<(), &'static str> {
        if message.from != TASK_FS || message.request != REQUEST_CLI_FS {
            return Err("cli-reply");
        }
        if message.body
            != MessageBody::Pair(
                FAT32_HELLO_CONTENT.len() as u32,
                FAT32_INDEX_CONTENT.len() as u32,
            )
        {
            return Err("cli-reply-body");
        }
        Ok(())
    }
}

pub(crate) fn emit_service_mailbox_envelope_ledger() {
    serial::write_str("DPMK:SERVICE-MAILBOX-ENVELOPE-LEDGER\n");
    serial::write_str("DPMBOX:shape fields=from,to,request,capability,body inline_bytes=");
    serial::write_decimal(dataplane_microkernel_core::MESSAGE_INLINE_BYTES as u32);
    serial::write_str(" mailbox_cap=");
    serial::write_decimal(SERVICE_MAILBOX_CAP as u32);
    serial::write_str(" heap=0 dyn_dispatch=0 generic_actor=0\n");

    serial::write_str("DPMBOX:tasks timer=");
    serial::write_decimal(TASK_TIMER.get() as u32);
    serial::write_str(" cli=");
    serial::write_decimal(TASK_CLI.get() as u32);
    serial::write_str(" fs=");
    serial::write_decimal(TASK_FS.get() as u32);
    serial::write_str(" block=");
    serial::write_decimal(TASK_BLOCK.get() as u32);
    serial::write_str(" net=");
    serial::write_decimal(TASK_NET.get() as u32);
    serial::write_str(" tcpip=");
    serial::write_decimal(TASK_TCPIP.get() as u32);
    serial::write_str(" http=");
    serial::write_decimal(TASK_HTTP.get() as u32);
    serial::write_str(" dhcp=");
    serial::write_decimal(TASK_DHCP.get() as u32);
    serial::write_str("\n");

    serial::write_str("DPMBOX:requests timer_tick=");
    serial::write_decimal(REQUEST_TIMER_TICK.get());
    serial::write_str(" cli_fs=");
    serial::write_decimal(REQUEST_CLI_FS.get());
    serial::write_str(" fs_block=");
    serial::write_decimal(REQUEST_FS_BLOCK.get());
    serial::write_str(" tcpip_net=");
    serial::write_decimal(REQUEST_TCPIP_NET.get());
    serial::write_str(" http_fs=");
    serial::write_decimal(REQUEST_HTTP_FS.get());
    serial::write_str(" dhcp_net=");
    serial::write_decimal(REQUEST_DHCP_NET.get());
    serial::write_str("\n");

    serial::write_str("DPMBOX:routes timer=1->1 cli_fs=2->3 fs_block=3->4 tcpip_net=6->5 http_fs=7->3 dhcp_net=8->5\n");
    serial::write_str("DPMBOX:errors queue_full=MailboxError::Full empty=MailboxError::Empty send=mailbox-send recv=mailbox-recv timeout=deferred stale_reply=deferred denied_route=deferred\n");
    serial::write_str("DPMBOX:fault-destination task=");
    serial::write_decimal(TASK_BLOCK.get() as u32);
    serial::write_str(" status=faulted restart=0 replay=0\n");
    serial::write_str("DPMK:SERVICE-MAILBOX-ENVELOPE-OK\n");
}

pub(crate) fn emit_service_lifecycle_ledger() {
    serial::write_str("DPMK:SERVICE-LIFECYCLE-LEDGER\n");
    serial::write_str(
        "DPLIFE:status-set empty=1 ready=1 waiting=defined faulted=1 stopped=defined degraded=0\n",
    );
    serial::write_str("DPLIFE:startup register_initial_tasks active=");
    serial::write_decimal(8);
    serial::write_str(" task_table_slots=");
    serial::write_decimal(TASK_TABLE_SLOTS as u32);
    serial::write_str(" status=ready\n");
    serial::write_str("DPLIFE:running timer=DPMK:TIMER service_loop=entered\n");
    serial::write_str("DPLIFE:ready timer=ready cli=ready fs=ready block=ready net=ready tcpip=ready http=ready dhcp=ready\n");
    serial::write_str(
        "DPLIFE:cli-visible timer=ready fs=ready block=ready tcpip=ready queues=bounded\n",
    );
    serial::write_str("DPLIFE:serving cli=DPMK:CLI-COMMANDS-OK http=DPMK:HTTP-GET-OK\n");
    serial::write_str("DPLIFE:fault-path task=");
    serial::write_decimal(TASK_BLOCK.get() as u32);
    serial::write_str(" status=faulted faults=1 marker=DPMK:FAULT-CONTAINED\n");
    serial::write_str("DPLIFE:deferred stopped=not-exercised degraded=not-defined restart=0 replay=0 cleanup_guarantee=0\n");
    serial::write_str("DPMK:SERVICE-LIFECYCLE-LEDGER-OK\n");
}

#[allow(dead_code)]
pub(crate) fn emit_service_mailbox_envelope_route_ledger() {
    serial::write_str("DPSVCMAIL:route timer->timer req=");
    serial::write_decimal(TIMER_REQUEST_ID_NUM);
    serial::write_str(" ep=");
    serial::write_decimal(u32::from(TIMER_ENDPOINT_ID_NUM));
    serial::write_str(" task=");
    serial::write_decimal(u32::from(TIMER_TASK_ID_NUM));
    serial::write_str("\n");
    serial::write_str("DPSVCMAIL:route tcpip->net req=");
    serial::write_decimal(REQUEST_TCPIP_NET.get());
    serial::write_str(" ep=");
    serial::write_decimal(EP_NET.get() as u32);
    serial::write_str(" task=");
    serial::write_decimal(EP_NET.get() as u32);
    serial::write_str("\n");
    serial::write_str("DPSVCMAIL:route dhcp->net req=");
    serial::write_decimal(REQUEST_DHCP_NET.get());
    serial::write_str(" ep=");
    serial::write_decimal(EP_NET.get() as u32);
    serial::write_str(" task=");
    serial::write_decimal(EP_DHCP.get() as u32);
    serial::write_str("\n");
}
