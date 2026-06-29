use crate::config::SHARD_REGION_SIZE;

const UART0_BASE: usize = 0x3f20_1000;
const UART_DR: usize = 0x00;
const UART_FR: usize = 0x18;
const UART_IBRD: usize = 0x24;
const UART_FBRD: usize = 0x28;
const UART_LCRH: usize = 0x2c;
const UART_CR: usize = 0x30;
const UART_ICR: usize = 0x44;
const UART_FR_RXFE: u32 = 1 << 4;
const UART_FR_TXFF: u32 = 1 << 5;
const UART_CR_UARTEN: u32 = 1 << 0;
const UART_CR_TXE: u32 = 1 << 8;
const UART_CR_RXE: u32 = 1 << 9;
const UART_LCRH_WLEN_8: u32 = 3 << 5;
const UART_ICR_ALL: u32 = 0x7ff;
const UART_REGION_MAGIC: u32 = 0x5541_5254;
const UART_RECORD_OFFSET: usize = 384;
const CHALLENGE: &[u8] = b"DPHOST?\n";
const MESSAGE: &[u8] = b"DPVM:OK\n";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VmUartState {
    Init,
    RecvChallenge,
    Send,
    Done,
}

pub struct VmUartTask {
    state: VmUartState,
    index: usize,
    received: usize,
    sent: usize,
    polls: u32,
}

impl VmUartTask {
    pub const fn new() -> Self {
        Self {
            state: VmUartState::Init,
            index: 0,
            received: 0,
            sent: 0,
            polls: 0,
        }
    }

    pub fn poll(&mut self, region: &mut [u8; SHARD_REGION_SIZE]) {
        self.polls = self.polls.wrapping_add(1);
        match self.state {
            VmUartState::Init => {
                init_uart();
                self.state = VmUartState::RecvChallenge;
            }
            VmUartState::RecvChallenge => {
                if self.received < CHALLENGE.len() && !rx_fifo_empty() {
                    let byte = (read_reg(UART_DR) & 0xff) as u8;
                    if byte == CHALLENGE[self.received] {
                        self.received += 1;
                    } else {
                        self.received = 0;
                    }
                }
                if self.received == CHALLENGE.len() {
                    self.state = VmUartState::Send;
                }
            }
            VmUartState::Send => {
                if self.index < MESSAGE.len() && !tx_fifo_full() {
                    write_reg(UART_DR, MESSAGE[self.index] as u32);
                    self.index += 1;
                    self.sent = self.index;
                }
                if self.index == MESSAGE.len() {
                    self.state = VmUartState::Done;
                }
            }
            VmUartState::Done => {}
        }
        record_state(region, self);
    }

    pub fn communicated(&self) -> bool {
        self.state == VmUartState::Done
            && self.received == CHALLENGE.len()
            && self.sent == MESSAGE.len()
    }
}

fn init_uart() {
    write_reg(UART_CR, 0);
    write_reg(UART_ICR, UART_ICR_ALL);
    write_reg(UART_IBRD, 1);
    write_reg(UART_FBRD, 40);
    write_reg(UART_LCRH, UART_LCRH_WLEN_8);
    write_reg(UART_CR, UART_CR_UARTEN | UART_CR_TXE | UART_CR_RXE);
}

fn tx_fifo_full() -> bool {
    read_reg(UART_FR) & UART_FR_TXFF != 0
}

fn rx_fifo_empty() -> bool {
    read_reg(UART_FR) & UART_FR_RXFE != 0
}

fn record_state(region: &mut [u8; SHARD_REGION_SIZE], task: &VmUartTask) {
    write_u32(region, UART_RECORD_OFFSET, UART_REGION_MAGIC);
    write_u32(region, UART_RECORD_OFFSET + 4, task.polls);
    write_u32(region, UART_RECORD_OFFSET + 8, task.state as u32);
    write_u32(region, UART_RECORD_OFFSET + 12, task.received as u32);
    write_u32(region, UART_RECORD_OFFSET + 16, task.sent as u32);
}

fn write_u32(region: &mut [u8; SHARD_REGION_SIZE], offset: usize, value: u32) {
    let bytes = value.to_le_bytes();
    let mut index = 0usize;
    while index < bytes.len() {
        region[offset + index] = bytes[index];
        index += 1;
    }
}

fn read_reg(offset: usize) -> u32 {
    unsafe { core::ptr::read_volatile((UART0_BASE + offset) as *const u32) }
}

fn write_reg(offset: usize, value: u32) {
    unsafe {
        core::ptr::write_volatile((UART0_BASE + offset) as *mut u32, value);
    }
}
