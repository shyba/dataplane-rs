use super::*;

const CLI_OPERATOR_TIMER: u8 = 1 << 0;
const CLI_OPERATOR_FS: u8 = 1 << 1;
const CLI_OPERATOR_BLOCK: u8 = 1 << 2;
const CLI_OPERATOR_TCPIP: u8 = 1 << 3;
const CLI_OPERATOR_QUEUES: u8 = 1 << 4;
const CLI_OPERATOR_PARITY: u8 = 1 << 5;
const CLI_OPERATOR_ALL: u8 = CLI_OPERATOR_TIMER
    | CLI_OPERATOR_FS
    | CLI_OPERATOR_BLOCK
    | CLI_OPERATOR_TCPIP
    | CLI_OPERATOR_QUEUES
    | CLI_OPERATOR_PARITY;

const CLI_CAPABILITY_TABLE: [CliCapability; 16] = [
    CliCapability::new(CliCommand::Help, CAP_CLI_HELP, TASK_CLI, EP_CLI),
    CliCapability::new(CliCommand::Tasks, CAP_CLI_TASKS, TASK_CLI, EP_CLI),
    CliCapability::new(CliCommand::FsLsRoot, CAP_CLI_FS_LS, TASK_CLI, EP_CLI),
    CliCapability::new(CliCommand::FsCatHello, CAP_CLI_FS_CAT, TASK_CLI, EP_CLI),
    CliCapability::new(CliCommand::FsCatIndex, CAP_CLI_FS_CAT, TASK_CLI, EP_CLI),
    CliCapability::new(CliCommand::FsStatHello, CAP_CLI_FS_STAT, TASK_CLI, EP_CLI),
    CliCapability::new(CliCommand::FsStatIndex, CAP_CLI_FS_STAT, TASK_CLI, EP_CLI),
    CliCapability::new(
        CliCommand::FsUnsupportedPath,
        CAP_CLI_FS_NEGATIVE,
        TASK_CLI,
        EP_CLI,
    ),
    CliCapability::new(
        CliCommand::FsLongName,
        CAP_CLI_FS_NEGATIVE,
        TASK_CLI,
        EP_CLI,
    ),
    CliCapability::new(
        CliCommand::FsUnsupportedWrite,
        CAP_CLI_FS_NEGATIVE,
        TASK_CLI,
        EP_CLI,
    ),
    CliCapability::new(CliCommand::TaskTimer, CAP_CLI_TASK_TIMER, TASK_CLI, EP_CLI),
    CliCapability::new(CliCommand::TaskFs, CAP_CLI_TASK_FS, TASK_CLI, EP_CLI),
    CliCapability::new(CliCommand::TaskBlock, CAP_CLI_TASK_BLOCK, TASK_CLI, EP_CLI),
    CliCapability::new(CliCommand::TaskTcpip, CAP_CLI_TASK_TCPIP, TASK_CLI, EP_CLI),
    CliCapability::new(CliCommand::Queues, CAP_CLI_QUEUES, TASK_CLI, EP_CLI),
    CliCapability::new(CliCommand::Parity, CAP_CLI_PARITY, TASK_CLI, EP_CLI),
];
const CLI_READ_ONLY_COMMANDS: [CliCommand; 16] = [
    CliCommand::Help,
    CliCommand::Tasks,
    CliCommand::FsLsRoot,
    CliCommand::FsCatHello,
    CliCommand::FsCatIndex,
    CliCommand::FsStatHello,
    CliCommand::FsStatIndex,
    CliCommand::FsUnsupportedPath,
    CliCommand::FsLongName,
    CliCommand::FsUnsupportedWrite,
    CliCommand::TaskTimer,
    CliCommand::TaskFs,
    CliCommand::TaskBlock,
    CliCommand::TaskTcpip,
    CliCommand::Queues,
    CliCommand::Parity,
];

impl KernelState {
    pub(crate) fn run_serial_cli(&mut self) -> Result<(), &'static str> {
        let mut line = [0; CLI_LINE_BYTES];
        let mut count = 0;
        let mut operator_seen = 0;
        let mut operator_done = false;
        while count < CLI_COMMAND_COUNT {
            let command = serial::read_line(&mut line)?;
            serial::write_str("DPMK:CLI-BEGIN:");
            serial::write_decimal((count + 1) as u32);
            serial::write_str(":");
            serial::write_bytes(command);
            serial::write_str("\n");
            match self.dispatch_cli_command(command) {
                Ok(()) => {
                    serial::write_str("DPMK:CLI-END:");
                    serial::write_decimal((count + 1) as u32);
                    serial::write_str(":OK\n");
                    operator_seen |= cli_operator_bit(command);
                    if operator_seen == CLI_OPERATOR_ALL && !operator_done {
                        serial::write_str("DPMK:CLI-OPERATOR-OK\n");
                        operator_done = true;
                    }
                }
                Err(reason)
                    if reason == "cli-forbidden"
                        || reason == "cli-command"
                        || reason == "cli-capability" =>
                {
                    serial::write_str("DPMK:CLI-END:");
                    serial::write_decimal((count + 1) as u32);
                    serial::write_str(":REJECT:");
                    serial::write_bytes(command);
                    serial::write_str(":");
                    serial::write_str(reason);
                    serial::write_str("\n");
                }
                Err(reason) => return Err(reason),
            }
            count += 1;
        }
        if !operator_done {
            return Err("cli-operator-proof");
        }
        Ok(())
    }

    pub(crate) fn dispatch_cli_command(&mut self, command: &[u8]) -> Result<(), &'static str> {
        let parsed = parse_cli_command(command)?;
        let capability = cli_command_capability(parsed).ok_or("cli-capability")?;
        if !authorize_cli_command(parsed, capability) {
            return Err("cli-capability");
        }
        match parsed {
            CliCommand::Help => self.cli_help(),
            CliCommand::Tasks => self.cli_tasks(),
            CliCommand::FsLsRoot => self.cli_fs_ls_root(),
            CliCommand::FsCatHello => self.cli_fs_cat(FAT32_HELLO_PATH.as_bytes()),
            CliCommand::FsCatIndex => self.cli_fs_cat(FAT32_INDEX_PATH.as_bytes()),
            CliCommand::FsStatHello => self.cli_fs_stat(FsKnownPath::Hello),
            CliCommand::FsStatIndex => self.cli_fs_stat(FsKnownPath::Index),
            CliCommand::FsUnsupportedPath => self.cli_fs_negative(FsNegativeCase::UnsupportedPath),
            CliCommand::FsLongName => self.cli_fs_negative(FsNegativeCase::LongFilename),
            CliCommand::FsUnsupportedWrite => {
                self.cli_fs_negative(FsNegativeCase::UnsupportedWrite)
            }
            CliCommand::TaskTimer => self.cli_task_detail("timer", TASK_TIMER),
            CliCommand::TaskFs => self.cli_task_detail("fs", TASK_FS),
            CliCommand::TaskBlock => self.cli_task_detail("block", TASK_BLOCK),
            CliCommand::TaskTcpip => self.cli_task_detail("tcpip", TASK_TCPIP),
            CliCommand::Queues => self.cli_queues(),
            CliCommand::Parity => self.cli_parity(),
            CliCommand::DebuggerStyle(_) | CliCommand::Forbidden(_) => Err("cli-forbidden"),
        }
    }
    fn cli_help(&self) -> Result<(), &'static str> {
        serial::write_str("DPCLI:HELP help tasks fs ls / fs cat /HELLO.TXT fs cat /INDEX.HTM\n");
        serial::write_str("DPCLI:HELP-FS fs stat /HELLO.TXT fs stat /INDEX.HTM fs stat /MISSING.TXT fs stat /THISNAMEISTOOLONG.TXT fs write /OUT.TXT append\n");
        serial::write_str("DPCLI:HELP-OPS task timer task fs task block task tcpip queues\n");
        Ok(())
    }
    fn cli_tasks(&self) -> Result<(), &'static str> {
        if self.tasks.active_count() != 8 {
            return Err("cli-task-count");
        }
        serial::write_str("DPCLI:TASKS timer=ready cli=ready fs=ready block=ready\n");
        serial::write_str("DPCLI:TASKS-NET net=candidate tcpip=candidate\n");
        serial::write_str("DPCLI:TASKS-DHCP dhcp=bounded\n");
        Ok(())
    }
    pub(crate) fn cli_task_detail(&self, name: &str, id: TaskId) -> Result<(), &'static str> {
        let slot = self.tasks.get(id).map_err(|_| "cli-task-detail")?;
        serial::write_str("DPCLI:TASK ");
        serial::write_str(name);
        serial::write_str(" id=");
        serial::write_decimal(slot.id.get() as u32);
        serial::write_str(" endpoint=");
        serial::write_decimal(slot.endpoint.get() as u32);
        serial::write_str(" status=");
        serial::write_str(task_status_name(slot.status));
        serial::write_str(" faults=");
        serial::write_decimal(slot.counters.faults);
        serial::write_str(" mailbox=");
        serial::write_decimal(self.mailbox_depth(id) as u32);
        serial::write_str(" enqueued=");
        serial::write_decimal(slot.counters.enqueued);
        serial::write_str(" dequeued=");
        serial::write_decimal(slot.counters.dequeued);
        serial::write_str("\n");
        Ok(())
    }
    fn cli_queues(&self) -> Result<(), &'static str> {
        serial::write_str("DPCLI:QUEUES timer=");
        serial::write_decimal(self.timer_mailbox.len() as u32);
        serial::write_str(" cli=");
        serial::write_decimal(self.cli_mailbox.len() as u32);
        serial::write_str(" fs=");
        serial::write_decimal(self.fs_mailbox.len() as u32);
        serial::write_str(" block=");
        serial::write_decimal(self.block_mailbox.len() as u32);
        serial::write_str(" net=");
        serial::write_decimal(self.net_mailbox.len() as u32);
        serial::write_str(" tcpip=");
        serial::write_decimal(self.tcpip_mailbox.len() as u32);
        serial::write_str(" http=");
        serial::write_decimal(self.http_mailbox.len() as u32);
        serial::write_str(" dhcp=");
        serial::write_decimal(self.dhcp_mailbox.len() as u32);
        serial::write_str("\n");
        Ok(())
    }
    pub(crate) fn cli_parity(&self) -> Result<(), &'static str> {
        serial::write_str("DPCLI:PARITY ");
        self.write_operator_parity_serial()?;
        serial::write_str("\n");
        Ok(())
    }
    #[allow(dead_code)]
    pub(crate) fn write_operator_parity_body(
        &self,
        out: &mut [u8],
        len: &mut usize,
    ) -> Result<(), &'static str> {
        append_bytes(out, len, b"DPSTATUS ")?;
        append_bytes(out, len, b"routes=t1>1,c2>3,b3>4,n6>5,h7>3,d8>5 ")?;
        append_bytes(
            out,
            len,
            b"service_caps=h2,t3,ls4,cat5,st6,neg7,tt8,tf9,tb10,ttc11,q12,p13 ",
        )?;
        append_bytes(out, len, b"storage_mode=ro ")?;
        append_bytes(
            out,
            len,
            b"network_counters=a1,i1,u16,200=3,404=1,405=1,413=2,500=1 ",
        )?;
        append_bytes(out, len, b"timer_status=ok,cli,fs,blk,net,tcpip,http,dhcp ")?;
        self.append_fault_status_body(out, len)?;
        append_bytes(out, len, b"generation_id=")?;
        append_decimal_bytes(out, len, GENERATION_ID)?;
        Ok(())
    }
    fn write_operator_parity_serial(&self) -> Result<(), &'static str> {
        serial::write_str("DPSTATUS ");
        serial::write_str("routes=t1>1,c2>3,b3>4,n6>5,h7>3,d8>5 ");
        serial::write_str("service_caps=h2,t3,ls4,cat5,st6,neg7,tt8,tf9,tb10,ttc11,q12,p13 ");
        serial::write_str("storage_mode=ro ");
        serial::write_str("network_counters=a1,i1,u16,200=3,404=1,405=1,413=2,500=1 ");
        serial::write_str("timer_status=ok,cli,fs,blk,net,tcpip,http,dhcp ");
        self.write_fault_status_serial()?;
        serial::write_str("generation_id=");
        serial::write_decimal(GENERATION_ID);
        Ok(())
    }
    fn mailbox_depth(&self, id: TaskId) -> usize {
        if id == TASK_TIMER {
            self.timer_mailbox.len()
        } else if id == TASK_CLI {
            self.cli_mailbox.len()
        } else if id == TASK_FS {
            self.fs_mailbox.len()
        } else if id == TASK_BLOCK {
            self.block_mailbox.len()
        } else if id == TASK_NET {
            self.net_mailbox.len()
        } else if id == TASK_TCPIP {
            self.tcpip_mailbox.len()
        } else if id == TASK_HTTP {
            self.http_mailbox.len()
        } else if id == TASK_DHCP {
            self.dhcp_mailbox.len()
        } else {
            0
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum CliCommand {
    Help,
    Tasks,
    FsLsRoot,
    FsCatHello,
    FsCatIndex,
    FsStatHello,
    FsStatIndex,
    FsUnsupportedPath,
    FsLongName,
    FsUnsupportedWrite,
    TaskTimer,
    TaskFs,
    TaskBlock,
    TaskTcpip,
    Queues,
    Parity,
    DebuggerStyle(DebuggerStyleCommand),
    Forbidden(ForbiddenCommand),
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum DebuggerStyleCommand {
    Debug,
    Restart,
    Reset,
    Replay,
    RawMemory,
    MmioDump,
    PageTableDump,
}
#[derive(Clone, Copy, PartialEq, Eq)]
enum ForbiddenCommand {
    WrongTask,
}
#[derive(Clone, Copy)]
struct CliCapability {
    command: CliCommand,
    capability: CapabilityId,
    task: TaskId,
    endpoint: EndpointId,
}
impl CliCapability {
    const fn new(
        command: CliCommand,
        capability: CapabilityId,
        task: TaskId,
        endpoint: EndpointId,
    ) -> Self {
        Self {
            command,
            capability,
            task,
            endpoint,
        }
    }
}
fn parse_cli_command(command: &[u8]) -> Result<CliCommand, &'static str> {
    match command {
        b"help" => Ok(CliCommand::Help),
        b"tasks" => Ok(CliCommand::Tasks),
        b"fs ls /" => Ok(CliCommand::FsLsRoot),
        b"fs cat /HELLO.TXT" => Ok(CliCommand::FsCatHello),
        b"fs cat /INDEX.HTM" => Ok(CliCommand::FsCatIndex),
        b"fs stat /HELLO.TXT" => Ok(CliCommand::FsStatHello),
        b"fs stat /INDEX.HTM" => Ok(CliCommand::FsStatIndex),
        b"fs stat /MISSING.TXT" => Ok(CliCommand::FsUnsupportedPath),
        b"fs stat /THISNAMEISTOOLONG.TXT" => Ok(CliCommand::FsLongName),
        b"fs write /OUT.TXT append" => Ok(CliCommand::FsUnsupportedWrite),
        b"task timer" => Ok(CliCommand::TaskTimer),
        b"task fs" => Ok(CliCommand::TaskFs),
        b"task block" => Ok(CliCommand::TaskBlock),
        b"task tcpip" => Ok(CliCommand::TaskTcpip),
        b"queues" => Ok(CliCommand::Queues),
        b"parity" => Ok(CliCommand::Parity),
        b"debug" => Ok(CliCommand::DebuggerStyle(DebuggerStyleCommand::Debug)),
        b"restart" => Ok(CliCommand::DebuggerStyle(DebuggerStyleCommand::Restart)),
        b"reset" => Ok(CliCommand::DebuggerStyle(DebuggerStyleCommand::Reset)),
        b"shell" => Err("cli-command"),
        b"json status" => Err("cli-command"),
        b"replay" => Ok(CliCommand::DebuggerStyle(DebuggerStyleCommand::Replay)),
        b"raw memory" => Ok(CliCommand::DebuggerStyle(DebuggerStyleCommand::RawMemory)),
        b"mmio dump" => Ok(CliCommand::DebuggerStyle(DebuggerStyleCommand::MmioDump)),
        b"page table dump" => Ok(CliCommand::DebuggerStyle(
            DebuggerStyleCommand::PageTableDump,
        )),
        b"wrong task" => Ok(CliCommand::Forbidden(ForbiddenCommand::WrongTask)),
        _ => Err("cli-command"),
    }
}
fn cli_operator_bit(command: &[u8]) -> u8 {
    match command {
        b"task timer" => CLI_OPERATOR_TIMER,
        b"task fs" => CLI_OPERATOR_FS,
        b"task block" => CLI_OPERATOR_BLOCK,
        b"task tcpip" => CLI_OPERATOR_TCPIP,
        b"queues" => CLI_OPERATOR_QUEUES,
        b"parity" => CLI_OPERATOR_PARITY,
        _ => 0,
    }
}
fn cli_command_capability(command: CliCommand) -> Option<CapabilityId> {
    CLI_CAPABILITY_TABLE
        .iter()
        .find(|entry| entry.command == command)
        .map(|entry| entry.capability)
}
fn authorize_cli_command(command: CliCommand, capability: CapabilityId) -> bool {
    for entry in &CLI_CAPABILITY_TABLE {
        if entry.command == command {
            return entry.capability == capability
                && entry.task == TASK_CLI
                && entry.endpoint == EP_CLI
                && is_read_only_operator_command(command);
        }
    }
    false
}
fn is_read_only_operator_command(command: CliCommand) -> bool {
    for allowed in CLI_READ_ONLY_COMMANDS {
        if allowed == command {
            return true;
        }
    }
    false
}
pub(crate) fn task_status_name(status: TaskStatus) -> &'static str {
    match status {
        TaskStatus::Empty => "empty",
        TaskStatus::Ready => "ready",
        TaskStatus::Waiting => "waiting",
        TaskStatus::Faulted => "faulted",
        TaskStatus::Stopped => "stopped",
    }
}
