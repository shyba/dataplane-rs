//! QEMU microbit (nRF51/Cortex-M0) execution and SysTick smoke test.
//! Semihosting exits success only after fixed-capacity and timer assertions pass.
//! Optional `uart-sessions` expects the bounded scripted workload supplied by
//! `tools/qemu_cortexm0_uart_sessions_run.sh`, not arbitrary network input.
//! The runner owns the wall-clock timeout; WFI/MMIO waits are not real-time deadlines.
#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

#[cfg(target_os = "none")]
use core::cell::Cell;
#[cfg(target_os = "none")]
use cortex_m::interrupt::Mutex;
#[cfg(target_os = "none")]
use cortex_m::peripheral::syst::SystClkSource;
#[cfg(target_os = "none")]
use cortex_m_rt::{entry, exception};
#[cfg(target_os = "none")]
use cortex_m_semihosting::debug::{exit, EXIT_FAILURE, EXIT_SUCCESS};
#[cfg(target_os = "none")]
use dataplane_runtime::noalloc::{
    drive_counts_step, FixedLocalExec, FixedLocalExecCounts, FixedTaskSlots,
};

#[cfg(target_os = "none")]
static TICKS: Mutex<Cell<u32>> = Mutex::new(Cell::new(0));

#[cfg(all(target_os = "none", feature = "uart-sessions"))]
const FRAME_IN_MAGIC: u8 = 0xa5;
#[cfg(all(target_os = "none", feature = "uart-sessions"))]
const FRAME_ACK_MAGIC: u8 = 0x5a;
#[cfg(all(target_os = "none", feature = "uart-sessions"))]
const SESSIONS: usize = 3;
#[cfg(all(target_os = "none", feature = "uart-sessions"))]
const SESSION_TARGETS: [usize; SESSIONS] = [64, 24, 24];
#[cfg(all(target_os = "none", feature = "uart-sessions"))]
const UART_RX_BUDGET: usize = 16;
#[cfg(all(target_os = "none", feature = "uart-sessions"))]
const SESSION_RUN_BUDGET: usize = 1;
#[cfg(all(target_os = "none", feature = "uart-sessions"))]
const BLINK_PERIOD_TICKS: u32 = 7;

#[cfg(all(target_os = "none", feature = "uart-sessions"))]
mod nrf51 {
    const UART_BASE: usize = 0x4000_2000;
    const GPIO_BASE: usize = 0x5000_0000;

    const UART_TASKS_STARTRX: usize = UART_BASE;
    const UART_TASKS_STARTTX: usize = UART_BASE + 0x008;
    const UART_EVENTS_RXDRDY: usize = UART_BASE + 0x108;
    const UART_EVENTS_TXDRDY: usize = UART_BASE + 0x11c;
    const UART_ENABLE: usize = UART_BASE + 0x500;
    const UART_PSELTXD: usize = UART_BASE + 0x50c;
    const UART_PSELRXD: usize = UART_BASE + 0x514;
    const UART_RXD: usize = UART_BASE + 0x518;
    const UART_TXD: usize = UART_BASE + 0x51c;
    const UART_BAUDRATE: usize = UART_BASE + 0x524;

    const GPIO_OUTSET: usize = GPIO_BASE + 0x508;
    const GPIO_OUTCLR: usize = GPIO_BASE + 0x50c;
    const GPIO_PIN_CNF_BASE: usize = GPIO_BASE + 0x700;
    const LED_PIN: usize = 13;

    #[inline(always)]
    fn write32(addr: usize, value: u32) {
        // SAFETY: callers in this private module use aligned nRF51 UART/GPIO
        // register addresses mapped by the QEMU microbit machine; no RAM references
        // alias these device registers.
        unsafe { core::ptr::write_volatile(addr as *mut u32, value) };
    }

    #[inline(always)]
    fn read32(addr: usize) -> u32 {
        // SAFETY: callers in this private module use aligned nRF51 UART/GPIO
        // register addresses mapped by the QEMU microbit machine; no RAM references
        // alias these device registers.
        unsafe { core::ptr::read_volatile(addr as *const u32) }
    }

    pub fn init_uart() {
        write32(UART_ENABLE, 4);
        write32(UART_PSELTXD, 24);
        write32(UART_PSELRXD, 25);
        write32(UART_BAUDRATE, 0x01d7_e000);
        write32(UART_EVENTS_RXDRDY, 0);
        write32(UART_EVENTS_TXDRDY, 0);
        write32(UART_TASKS_STARTRX, 1);
        write32(UART_TASKS_STARTTX, 1);
    }

    pub fn uart_try_read() -> Option<u8> {
        if read32(UART_EVENTS_RXDRDY) == 0 {
            return None;
        }

        write32(UART_EVENTS_RXDRDY, 0);
        Some(read32(UART_RXD) as u8)
    }

    pub fn uart_write(byte: u8) {
        write32(UART_EVENTS_TXDRDY, 0);
        write32(UART_TXD, byte as u32);
        while read32(UART_EVENTS_TXDRDY) == 0 {
            cortex_m::asm::nop();
        }
    }

    pub fn init_led() {
        let pin_cnf = GPIO_PIN_CNF_BASE + LED_PIN * 4;
        write32(pin_cnf, 0b11);
        write32(GPIO_OUTCLR, 1 << LED_PIN);
    }

    pub fn set_led(on: bool) {
        if on {
            write32(GPIO_OUTSET, 1 << LED_PIN);
        } else {
            write32(GPIO_OUTCLR, 1 << LED_PIN);
        }
    }
}

#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    exit(EXIT_FAILURE);
    loop {
        cortex_m::asm::bkpt();
    }
}

#[cfg(target_os = "none")]
#[exception]
fn SysTick() {
    cortex_m::interrupt::free(|cs| {
        let ticks = TICKS.borrow(cs).get();
        TICKS.borrow(cs).set(ticks.wrapping_add(1));
    });
}

#[cfg(target_os = "none")]
fn read_ticks() -> u32 {
    cortex_m::interrupt::free(|cs| TICKS.borrow(cs).get())
}

#[cfg(target_os = "none")]
fn wait_for_next_tick(last_tick: u32) -> u32 {
    for _ in 0..1_000_000 {
        let now = read_ticks();
        if now != last_tick {
            return now;
        }
        cortex_m::asm::wfi();
    }

    exit(EXIT_FAILURE);
    loop {
        cortex_m::asm::bkpt();
    }
}

#[cfg(target_os = "none")]
fn drive_counts_from_systick_ticks() {
    let mut peripherals = cortex_m::Peripherals::take().unwrap();
    peripherals.SYST.set_clock_source(SystClkSource::Core);
    peripherals.SYST.set_reload(16_000 - 1);
    peripherals.SYST.clear_current();
    peripherals.SYST.enable_interrupt();
    peripherals.SYST.enable_counter();

    let mut counts: FixedLocalExecCounts<1, 4> = FixedLocalExecCounts::new();
    counts.push_count(0, 3).unwrap();

    let mut last_tick = read_ticks();
    let mut progress = 0u32;
    while counts.has_work() {
        last_tick = wait_for_next_tick(last_tick);
        let step = drive_counts_step(&mut counts, 1, 1, false);
        progress += step.progressed as u32;
    }

    assert_eq!(progress, 3);
    assert!(read_ticks() >= 3);

    run_systick_flooder_like_workload(&mut last_tick);

    assert!(read_ticks() >= 4);

    #[cfg(feature = "uart-sessions")]
    {
        nrf51::init_uart();
        nrf51::init_led();
        run_three_uart_sessions_while_blinking(&mut last_tick);
    }

    peripherals.SYST.disable_interrupt();
    peripherals.SYST.disable_counter();
}

#[cfg(target_os = "none")]
fn run_systick_flooder_like_workload(last_tick: &mut u32) {
    const TOTAL_TASKS: usize = 128;
    const YIELDS_PER_TASK: usize = 1;
    const STEPS_PER_TASK: usize = YIELDS_PER_TASK + 1;
    const BURST: usize = 8;
    const SLOTS: usize = 4;
    const MAX_PER_SLOT: usize = 64;
    const SESSION_RUN_BUDGET: usize = 4;

    let mut counts: FixedLocalExecCounts<SLOTS, MAX_PER_SLOT> = FixedLocalExecCounts::new();
    let mut spawned = 0usize;
    let mut progressed = 0usize;

    while spawned < TOTAL_TASKS || counts.has_work() {
        *last_tick = wait_for_next_tick(*last_tick);

        let mut batch = 0usize;
        while batch < BURST && spawned < TOTAL_TASKS {
            counts.push_count(spawned % SLOTS, STEPS_PER_TASK).unwrap();
            spawned += 1;
            batch += 1;
        }

        let step = drive_counts_step(&mut counts, SLOTS, SESSION_RUN_BUDGET, false);
        progressed += step.progressed;
    }

    assert_eq!(spawned, TOTAL_TASKS);
    assert_eq!(progressed, TOTAL_TASKS * STEPS_PER_TASK);
    assert_eq!(counts.pending(), 0);
}

#[cfg(all(target_os = "none", feature = "uart-sessions"))]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum UartFrameParser {
    Magic,
    Session,
    Units { session: u8 },
    Checksum { session: u8, units: u8 },
}

#[cfg(all(target_os = "none", feature = "uart-sessions"))]
impl UartFrameParser {
    fn push<const CAP: usize>(
        &mut self,
        byte: u8,
        work: &mut FixedLocalExec<u8, SESSIONS, CAP>,
        received: &mut [usize; SESSIONS],
    ) {
        match *self {
            Self::Magic => {
                if byte == FRAME_IN_MAGIC {
                    *self = Self::Session;
                }
            }
            Self::Session => {
                if (byte as usize) < SESSIONS {
                    *self = Self::Units { session: byte };
                } else {
                    *self = Self::Magic;
                }
            }
            Self::Units { session } => {
                if byte == 0 {
                    *self = Self::Magic;
                } else {
                    *self = Self::Checksum {
                        session,
                        units: byte,
                    };
                }
            }
            Self::Checksum { session, units } => {
                if byte == (FRAME_IN_MAGIC ^ session ^ units) {
                    let slot = session as usize;
                    let mut pushed = 0u8;
                    while pushed < units {
                        work.push(slot, session).unwrap();
                        received[slot] += 1;
                        pushed += 1;
                    }
                }
                *self = Self::Magic;
            }
        }
    }
}

#[cfg(all(target_os = "none", feature = "uart-sessions"))]
fn send_ack(session: u8, completed: usize) {
    let count = completed as u8;
    let checksum = FRAME_ACK_MAGIC ^ session ^ count;
    nrf51::uart_write(FRAME_ACK_MAGIC);
    nrf51::uart_write(session);
    nrf51::uart_write(count);
    nrf51::uart_write(checksum);
}

#[cfg(all(target_os = "none", feature = "uart-sessions"))]
fn all_targets_reached(completed: &[usize; SESSIONS]) -> bool {
    let mut session = 0usize;
    while session < SESSIONS {
        if completed[session] != SESSION_TARGETS[session] {
            return false;
        }
        session += 1;
    }
    true
}

#[cfg(all(target_os = "none", feature = "uart-sessions"))]
fn run_three_uart_sessions_while_blinking(last_tick: &mut u32) {
    let mut parser = UartFrameParser::Magic;
    let mut work: FixedLocalExec<u8, SESSIONS, 64> = FixedLocalExec::new();
    let mut received = [0usize; SESSIONS];
    let mut completed = [0usize; SESSIONS];
    let mut led_on = false;
    let mut next_blink = read_ticks().wrapping_add(BLINK_PERIOD_TICKS);

    while !all_targets_reached(&completed) {
        *last_tick = wait_for_next_tick(*last_tick);
        let now = read_ticks();
        if now.wrapping_sub(next_blink) < u32::MAX / 2 {
            led_on = !led_on;
            nrf51::set_led(led_on);
            next_blink = now.wrapping_add(BLINK_PERIOD_TICKS);
        }

        let mut rx_budget = 0usize;
        while rx_budget < UART_RX_BUDGET {
            let Some(byte) = nrf51::uart_try_read() else {
                break;
            };
            parser.push(byte, &mut work, &mut received);
            rx_budget += 1;
        }

        let progressed = work.drain(SESSIONS, SESSION_RUN_BUDGET, false, |session| {
            let slot = session as usize;
            completed[slot] += 1;
            send_ack(session, completed[slot]);
        });

        if progressed == 0 && !work.has_work() {
            cortex_m::asm::nop();
        }
    }

    assert_eq!(received, SESSION_TARGETS);
    assert_eq!(completed, SESSION_TARGETS);
}

#[cfg(target_os = "none")]
#[entry]
fn main() -> ! {
    let mut task_slots: FixedTaskSlots<u32, 2> = FixedTaskSlots::new().unwrap();
    let first = task_slots.insert(10).unwrap();
    let second = task_slots.insert(20).unwrap();
    assert_eq!(task_slots.active_count(), 2);
    assert_eq!(task_slots.insert(30), Err(30));

    task_slots.release(first);
    let third = task_slots.insert(30).unwrap();
    assert_eq!(task_slots.get(first), None);
    assert_eq!(task_slots.get(second), Some(20));
    assert_eq!(task_slots.get(third), Some(30));

    let mut items: FixedLocalExec<u8, 2, 4> = FixedLocalExec::new();
    items.push(0, 1).unwrap();
    items.push(0, 2).unwrap();
    items.push(1, 3).unwrap();

    let mut item_total = 0u32;
    let item_progress = items.drain(usize::MAX, 1, false, |item| {
        item_total += item as u32;
    });
    assert_eq!(item_progress, 3);
    assert_eq!(item_total, 6);
    assert!(!items.has_work());

    let mut counts: FixedLocalExecCounts<2, 4> = FixedLocalExecCounts::new();
    counts.push_count(0, 2).unwrap();
    counts.push_count(1, 1).unwrap();

    let mut count_total = 0u32;
    let count_progress = counts.drain(usize::MAX, 1, false, || {
        count_total += 1;
    });
    assert_eq!(count_progress, 3);
    assert_eq!(count_total, 3);
    assert!(!counts.has_work());

    let mut step_counts: FixedLocalExecCounts<1, 4> = FixedLocalExecCounts::new();
    step_counts.push_count(0, 2).unwrap();
    let step = drive_counts_step(&mut step_counts, 1, 1, false);
    assert_eq!(step.progressed, 1);
    assert!(step.work_remaining);

    drive_counts_from_systick_ticks();

    core::hint::black_box((task_slots, third, step_counts));
    exit(EXIT_SUCCESS);
    loop {
        cortex_m::asm::bkpt();
    }
}

#[cfg(not(target_os = "none"))]
fn main() {
    println!("dataplane-qemu-cortexm0-smoke is only active for target_os=none");
}
