use core::cell::RefCell;
use core::cmp;
use core::fmt::{self, Write};
use core::future::Future;
use core::pin::Pin;
use core::task::{Context, Poll};

use dataplane_rp2040_scd41_smoke::rolling_log::{
    Flash, FlashError, Geometry, RAW_MEASUREMENT_BYTES, RECORD_PAYLOAD_BYTES, RECORDS_PER_PAGE,
    RollingLog,
};
use dataplane_runtime::rp2040::hil_recovery::{
    WatchdogBootselConfig, WatchdogBootselRecovery, watchdog_reset_pending,
};
#[cfg(feature = "usb-auto-bootsel")]
use dataplane_runtime::rp2040::usb_auto_bootsel::UsbCdcAutoBootsel;
use embedded_hal_async::delay::DelayNs;
use rp2040_hal as hal;
use scd4x::Scd4xAsync;
use scd4x::types::{RawSensorData, SensorData};
use usb_device::class_prelude::UsbBusAllocator;
use usb_device::prelude::*;
use usbd_serial::{SerialPort, USB_CLASS_CDC};

use hal::Timer;
use hal::clocks::{Clock, init_clocks_and_plls};
use hal::fugit::RateExtU32;
use hal::gpio::{FunctionI2c, Pin as GpioPin};
use hal::i2c::I2C;
use hal::pac;
use hal::sio::Sio;
use hal::usb::UsbBus;
use hal::watchdog::Watchdog;

const XOSC_CRYSTAL_FREQ_HZ: u32 = 12_000_000;
const LOG_PERIOD_US: u64 = 1_000_000;
const POLL_INTERVAL_US: u64 = 1_000;
const SERIAL_LOG_WRITE_RETRIES: usize = 16;
const SERIAL_RESPONSE_WRITE_RETRIES: usize = 4096;
const READ_COMMAND: &[u8] = b"READ ";
const PAGE_LINE_MAX: usize = 5 + 20 + 1 + RECORD_PAYLOAD_BYTES * 2 + 1;

const FLASH_STORAGE_RESERVED_BYTES: usize = 512 * 1024;
const FLASH_TOTAL_BYTES: usize = 2 * 1024 * 1024;
const STORAGE_ALLOCATOR_OFFSET: usize = FLASH_STORAGE_RESERVED_BYTES;
const STORAGE_DATA_OFFSET: usize = STORAGE_ALLOCATOR_OFFSET + 4096;
const STORAGE_PAGE_COUNT: usize = (FLASH_TOTAL_BYTES - STORAGE_DATA_OFFSET) / 256;
const STORAGE_GEOMETRY: Geometry = Geometry::new(
    STORAGE_ALLOCATOR_OFFSET,
    STORAGE_DATA_OFFSET,
    STORAGE_PAGE_COUNT,
);

#[unsafe(link_section = ".boot2")]
#[used]
pub static BOOT2: [u8; 256] = rp2040_boot2::BOOT_LOADER_GENERIC_03H;

#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        cortex_m::asm::wfi();
    }
}

struct DataplaneDelay<'a> {
    timer: &'a Timer,
}

impl<'a> DataplaneDelay<'a> {
    fn new(timer: &'a Timer) -> Self {
        Self { timer }
    }
}

impl DelayNs for DataplaneDelay<'_> {
    async fn delay_ns(&mut self, ns: u32) {
        DelayFuture::new(self.timer, ns_to_us(ns)).await;
    }
}

struct DelayFuture<'a> {
    timer: &'a Timer,
    deadline_us: u64,
}

impl<'a> DelayFuture<'a> {
    fn new(timer: &'a Timer, delay_us: u64) -> Self {
        Self {
            timer,
            deadline_us: now_us(timer).saturating_add(delay_us),
        }
    }
}

impl Future for DelayFuture<'_> {
    type Output = ();

    fn poll(self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        if now_us(self.timer) >= self.deadline_us {
            Poll::Ready(())
        } else {
            cx.waker().wake_by_ref();
            Poll::Pending
        }
    }
}

struct Averager {
    count: u32,
    co2_sum: u64,
    temperature_sum: f32,
    humidity_sum: f32,
}

impl Averager {
    const fn new() -> Self {
        Self {
            count: 0,
            co2_sum: 0,
            temperature_sum: 0.0,
            humidity_sum: 0.0,
        }
    }

    fn push(&mut self, co2: u16, temperature: f32, humidity: f32) {
        self.count = self.count.saturating_add(1);
        self.co2_sum = self.co2_sum.saturating_add(co2 as u64);
        self.temperature_sum += temperature;
        self.humidity_sum += humidity;
    }

    fn take(&mut self) -> Option<Average> {
        if self.count == 0 {
            return None;
        }

        let count = self.count;
        let average = Average {
            samples: count,
            co2_ppm: (self.co2_sum / count as u64) as u16,
            temperature_c: self.temperature_sum / count as f32,
            humidity_rh: self.humidity_sum / count as f32,
        };
        *self = Self::new();
        Some(average)
    }
}

#[derive(Clone, Copy)]
struct Average {
    samples: u32,
    co2_ppm: u16,
    temperature_c: f32,
    humidity_rh: f32,
}

struct SerialLogger<'a> {
    #[cfg(feature = "usb-auto-bootsel")]
    usb: UsbCdcAutoBootsel<'a, UsbBus>,
    #[cfg(not(feature = "usb-auto-bootsel"))]
    serial: SerialPort<'a, UsbBus>,
    #[cfg(not(feature = "usb-auto-bootsel"))]
    usb_dev: UsbDevice<'a, UsbBus>,
    read_command_pos: usize,
    read_amount: usize,
    read_saw_digit: bool,
    read_after_digits_ws: bool,
    pending_read: Option<usize>,
    rx_polls: u32,
    rx_bytes: u32,
    rx_errors: u32,
    parsed_reads: u32,
    rx_recent: [u8; 8],
    rx_recent_pos: usize,
    pending_line: [u8; PAGE_LINE_MAX],
    pending_len: usize,
    pending_pos: usize,
}

impl SerialLogger<'_> {
    fn poll(&mut self) {
        let _ = self.poll_bus();
        self.drain_rx();
        self.flush_pending_line();
    }

    fn poll_bus(&mut self) -> bool {
        #[cfg(feature = "usb-auto-bootsel")]
        let active = self.usb.poll();

        #[cfg(not(feature = "usb-auto-bootsel"))]
        let active = self.usb_dev.poll(&mut [&mut self.serial]);

        active
    }

    fn drain_rx(&mut self) {
        let mut buf = [0u8; 64];
        for _ in 0..4 {
            self.rx_polls = self.rx_polls.wrapping_add(1);
            match self.read_usb(&mut buf) {
                Ok(0) => break,
                Ok(count) => {
                    self.rx_bytes = self.rx_bytes.wrapping_add(count as u32);
                    for byte in &buf[..count] {
                        self.rx_recent[self.rx_recent_pos % self.rx_recent.len()] = *byte;
                        self.rx_recent_pos = self.rx_recent_pos.wrapping_add(1);
                        self.accept_read_command_byte(*byte);
                    }
                }
                Err(usb_device::UsbError::WouldBlock) => break,
                Err(_) => {
                    self.rx_errors = self.rx_errors.wrapping_add(1);
                    break;
                }
            }
        }
    }

    fn accept_read_command_byte(&mut self, byte: u8) {
        if self.read_command_pos < READ_COMMAND.len() {
            if byte == READ_COMMAND[self.read_command_pos] {
                self.read_command_pos += 1;
                if self.read_command_pos == READ_COMMAND.len() {
                    self.read_amount = 0;
                    self.read_saw_digit = false;
                    self.read_after_digits_ws = false;
                }
                return;
            }

            self.read_command_pos = usize::from(byte == READ_COMMAND[0]);
            return;
        }

        match byte {
            b'0'..=b'9' if !self.read_after_digits_ws => {
                self.read_saw_digit = true;
                self.read_amount = self
                    .read_amount
                    .saturating_mul(10)
                    .saturating_add((byte - b'0') as usize);
            }
            b'\r' | b'\n' if self.read_saw_digit => {
                self.pending_read = Some(self.read_amount);
                self.parsed_reads = self.parsed_reads.wrapping_add(1);
                self.reset_read_command();
            }
            b' ' | b'\t' if !self.read_saw_digit => {}
            b' ' | b'\t' => {
                self.read_after_digits_ws = true;
            }
            _ => self.reset_read_command(),
        }
    }

    fn reset_read_command(&mut self) {
        self.read_command_pos = 0;
        self.read_amount = 0;
        self.read_saw_digit = false;
        self.read_after_digits_ws = false;
    }

    fn take_read_command(&mut self) -> Option<usize> {
        self.pending_read.take()
    }

    fn log_average(&mut self, average: Average) {
        self.poll();
        let rx_polls = self.rx_polls;
        let rx_bytes = self.rx_bytes;
        let rx_errors = self.rx_errors;
        let parsed_reads = self.parsed_reads;
        let rx_recent = self.rx_recent_ordered();
        let _ = writeln!(
            self,
            "co2_ppm={} temperature_c={} humidity_rh={} samples={} rx_polls={} rx_bytes={} rx_errors={} parsed_reads={} rx_recent={},{},{},{},{},{},{},{}",
            average.co2_ppm,
            F32_2(average.temperature_c),
            F32_2(average.humidity_rh),
            average.samples,
            rx_polls,
            rx_bytes,
            rx_errors,
            parsed_reads,
            rx_recent[0],
            rx_recent[1],
            rx_recent[2],
            rx_recent[3],
            rx_recent[4],
            rx_recent[5],
            rx_recent[6],
            rx_recent[7]
        );
    }

    fn rx_recent_ordered(&self) -> [u8; 8] {
        let mut out = [0u8; 8];
        let len = cmp::min(self.rx_recent_pos, self.rx_recent.len());
        let start = self.rx_recent_pos.saturating_sub(len);
        for index in 0..len {
            out[self.rx_recent.len() - len + index] =
                self.rx_recent[(start + index) % self.rx_recent.len()];
        }
        out
    }

    fn log_status(&mut self, status: &str) {
        self.poll();
        let _ = writeln!(self, "{status}");
    }

    fn log_storage_recovery(&mut self, valid_pages: usize, next_counter: u64, next_slot: usize) {
        self.poll();
        let _ = writeln!(
            self,
            "storage recovered pages={valid_pages} next_counter={next_counter} next_slot={next_slot}"
        );
    }

    fn log_storage_append_begin(&mut self, counter: u64, slot: usize) {
        self.poll();
        let _ = writeln!(self, "storage append begin counter={counter} slot={slot}");
    }

    fn log_read_begin(&mut self, amount: usize, valid_pages: usize, newest_counter: u64) {
        self.poll();
        let _ = writeln!(
            self,
            "READ begin amount={amount} valid_pages={valid_pages} newest_counter={newest_counter}"
        );
    }

    fn log_page(&mut self, counter: u64, payload: &[u8; RECORD_PAYLOAD_BYTES]) -> bool {
        self.poll();
        if self.pending_len != 0 {
            return false;
        }

        let mut offset = 0usize;

        self.pending_line[offset..offset + 5].copy_from_slice(b"PAGE ");
        offset += 5;
        offset += write_u64_decimal(counter, &mut self.pending_line[offset..]);
        self.pending_line[offset] = b' ';
        offset += 1;

        const HEX: &[u8; 16] = b"0123456789abcdef";
        for byte in payload {
            self.pending_line[offset] = HEX[(byte >> 4) as usize];
            self.pending_line[offset + 1] = HEX[(byte & 0x0F) as usize];
            offset += 2;
        }
        self.pending_line[offset] = b'\n';
        offset += 1;

        self.pending_len = offset;
        self.pending_pos = 0;
        self.flush_pending_line();
        true
    }

    fn has_pending_line(&self) -> bool {
        self.pending_len != 0
    }

    fn flush_pending_line(&mut self) {
        for _ in 0..SERIAL_RESPONSE_WRITE_RETRIES {
            if self.pending_pos >= self.pending_len {
                break;
            }

            let start = self.pending_pos;
            let end = self.pending_len;
            let mut chunk = [0u8; 64];
            let chunk_len = cmp::min(chunk.len(), end - start);
            chunk[..chunk_len].copy_from_slice(&self.pending_line[start..start + chunk_len]);

            match self.write_usb(&chunk[..chunk_len]) {
                Ok(0) | Err(usb_device::UsbError::WouldBlock) => {
                    break;
                }
                Ok(written) => {
                    self.pending_pos += written;
                }
                Err(_) => {
                    self.pending_len = 0;
                    self.pending_pos = 0;
                    return;
                }
            }
        }

        if self.pending_pos >= self.pending_len {
            self.pending_len = 0;
            self.pending_pos = 0;
        }
    }

    fn write_bytes(&mut self, bytes: &[u8]) -> fmt::Result {
        self.write_bytes_with_retries(bytes, SERIAL_LOG_WRITE_RETRIES)
    }

    fn write_bytes_with_retries(&mut self, mut bytes: &[u8], max_retries: usize) -> fmt::Result {
        let mut retries = 0usize;
        while !bytes.is_empty() {
            let _ = self.poll_bus();
            match self.write_usb(bytes) {
                Ok(0) => {
                    retries += 1;
                    if retries >= max_retries {
                        return Ok(());
                    }
                }
                Ok(written) => {
                    bytes = &bytes[written..];
                    retries = 0;
                }
                Err(usb_device::UsbError::WouldBlock) => {
                    retries += 1;
                    if retries >= max_retries {
                        return Ok(());
                    }
                }
                Err(_) => return Err(fmt::Error),
            }
        }
        Ok(())
    }

    #[cfg(feature = "usb-auto-bootsel")]
    fn read_usb(&mut self, buf: &mut [u8]) -> usb_device::Result<usize> {
        self.usb.read(buf)
    }

    #[cfg(not(feature = "usb-auto-bootsel"))]
    fn read_usb(&mut self, buf: &mut [u8]) -> usb_device::Result<usize> {
        self.serial.read(buf)
    }

    #[cfg(feature = "usb-auto-bootsel")]
    fn write_usb(&mut self, bytes: &[u8]) -> usb_device::Result<usize> {
        self.usb.write(bytes)
    }

    #[cfg(not(feature = "usb-auto-bootsel"))]
    fn write_usb(&mut self, bytes: &[u8]) -> usb_device::Result<usize> {
        self.serial.write(bytes)
    }
}

impl Write for SerialLogger<'_> {
    fn write_str(&mut self, s: &str) -> fmt::Result {
        self.write_bytes(s.as_bytes())
    }
}

struct F32_2(f32);

impl fmt::Display for F32_2 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let scaled = (self.0 * 100.0) as i32;
        let whole = scaled / 100;
        let frac = (scaled % 100).abs();
        write!(f, "{whole}.{frac:02}")
    }
}

struct MeasurementPageBuilder {
    payload: [u8; RECORD_PAYLOAD_BYTES],
    count: usize,
}

impl MeasurementPageBuilder {
    const fn new() -> Self {
        Self {
            payload: [0; RECORD_PAYLOAD_BYTES],
            count: 0,
        }
    }

    fn push(&mut self, raw: RawSensorData) -> Option<[u8; RECORD_PAYLOAD_BYTES]> {
        let offset = self.count * RAW_MEASUREMENT_BYTES;
        write_raw_measurement(
            raw,
            &mut self.payload[offset..offset + RAW_MEASUREMENT_BYTES],
        );
        self.count += 1;

        if self.count == RECORDS_PER_PAGE {
            let payload = self.payload;
            *self = Self::new();
            Some(payload)
        } else {
            None
        }
    }
}

struct Rp2040Flash;

impl Flash for Rp2040Flash {
    fn read(&self, offset: usize, out: &mut [u8]) -> Result<(), FlashError> {
        let Some(end) = offset.checked_add(out.len()) else {
            return Err(FlashError::OutOfBounds);
        };
        if end > FLASH_TOTAL_BYTES {
            return Err(FlashError::OutOfBounds);
        }

        let src = (0x1000_0000usize + offset) as *const u8;
        unsafe {
            core::ptr::copy_nonoverlapping(src, out.as_mut_ptr(), out.len());
        }
        Ok(())
    }

    fn program(&mut self, offset: usize, data: &[u8]) -> Result<(), FlashError> {
        let Some(end) = offset.checked_add(data.len()) else {
            return Err(FlashError::OutOfBounds);
        };
        if end > FLASH_TOTAL_BYTES {
            return Err(FlashError::OutOfBounds);
        }

        let page_offset = offset & !0xFF;
        let page_end = (end + 0xFF) & !0xFF;
        let mut page = [0xFF; 256];
        let mut current = page_offset;
        while current < page_end {
            self.read(current, &mut page)?;
            let copy_start = cmp::max(offset, current);
            let copy_end = cmp::min(end, current + page.len());
            page[copy_start - current..copy_end - current]
                .copy_from_slice(&data[copy_start - offset..copy_end - offset]);

            for (old, new) in flash_slice(current, page.len()).iter().zip(page.iter()) {
                if (old & new) != *new {
                    return Err(FlashError::ProgramConflict);
                }
            }

            program_flash_page(current, &page);
            current += page.len();
        }

        Ok(())
    }

    fn erase_sector(&mut self, offset: usize) -> Result<(), FlashError> {
        if offset % 4096 != 0 || offset >= FLASH_TOTAL_BYTES {
            return Err(FlashError::OutOfBounds);
        }
        erase_flash_sector(offset);
        Ok(())
    }
}

struct ReadReplay {
    next_age: usize,
    remaining: usize,
    ok_pending: bool,
}

fn write_u64_decimal(mut value: u64, out: &mut [u8]) -> usize {
    let mut digits = [0u8; 20];
    let mut len = 0usize;

    loop {
        digits[len] = b'0' + (value % 10) as u8;
        len += 1;
        value /= 10;
        if value == 0 {
            break;
        }
    }

    for index in 0..len {
        out[index] = digits[len - 1 - index];
    }
    len
}

fn write_raw_measurement(raw: RawSensorData, out: &mut [u8]) {
    write_word_with_crc(raw.co2, &mut out[0..3]);
    write_word_with_crc(raw.temperature, &mut out[3..6]);
    write_word_with_crc(raw.humidity, &mut out[6..9]);
}

fn write_word_with_crc(word: u16, out: &mut [u8]) {
    let bytes = word.to_be_bytes();
    out[0] = bytes[0];
    out[1] = bytes[1];
    out[2] = sensirion_crc8(&bytes);
}

fn sensirion_crc8(bytes: &[u8]) -> u8 {
    let mut crc = 0xFFu8;
    for byte in bytes {
        crc ^= *byte;
        for _ in 0..8 {
            if (crc & 0x80) != 0 {
                crc = (crc << 1) ^ 0x31;
            } else {
                crc <<= 1;
            }
        }
    }
    crc
}

fn measurement_from_raw(raw: RawSensorData) -> SensorData {
    SensorData {
        co2: raw.co2,
        temperature: (raw.temperature as f32 * 175_f32) / (u16::MAX as f32) - 45_f32,
        humidity: (raw.humidity as f32 * 100_f32) / (u16::MAX as f32),
    }
}

fn flash_slice(offset: usize, len: usize) -> &'static [u8] {
    let src = (0x1000_0000usize + offset) as *const u8;
    unsafe { core::slice::from_raw_parts(src, len) }
}

fn program_flash_page(offset: usize, page: &[u8; 256]) {
    let connect_internal_flash = hal::rom_data::connect_internal_flash::ptr();
    let flash_exit_xip = hal::rom_data::flash_exit_xip::ptr();
    let flash_range_program = hal::rom_data::flash_range_program::ptr();
    let flash_flush_cache = hal::rom_data::flash_flush_cache::ptr();
    let flash_enter_cmd_xip = hal::rom_data::flash_enter_cmd_xip::ptr();

    cortex_m::interrupt::free(|_| unsafe {
        program_flash_page_from_ram(
            connect_internal_flash,
            flash_exit_xip,
            flash_range_program,
            flash_flush_cache,
            flash_enter_cmd_xip,
            offset as u32,
            page.as_ptr(),
            page.len(),
        );
    });
}

fn erase_flash_sector(offset: usize) {
    let connect_internal_flash = hal::rom_data::connect_internal_flash::ptr();
    let flash_exit_xip = hal::rom_data::flash_exit_xip::ptr();
    let flash_range_erase = hal::rom_data::flash_range_erase::ptr();
    let flash_flush_cache = hal::rom_data::flash_flush_cache::ptr();
    let flash_enter_cmd_xip = hal::rom_data::flash_enter_cmd_xip::ptr();

    cortex_m::interrupt::free(|_| unsafe {
        erase_flash_sector_from_ram(
            connect_internal_flash,
            flash_exit_xip,
            flash_range_erase,
            flash_flush_cache,
            flash_enter_cmd_xip,
            offset as u32,
        );
    });
}

#[inline(never)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".data.ram_func")]
unsafe fn program_flash_page_from_ram(
    connect_internal_flash: unsafe extern "C" fn(),
    flash_exit_xip: unsafe extern "C" fn(),
    flash_range_program: unsafe extern "C" fn(u32, *const u8, usize),
    flash_flush_cache: unsafe extern "C" fn(),
    flash_enter_cmd_xip: unsafe extern "C" fn(),
    offset: u32,
    data: *const u8,
    len: usize,
) {
    unsafe {
        connect_internal_flash();
        flash_exit_xip();
        flash_range_program(offset, data, len);
        flash_flush_cache();
        flash_enter_cmd_xip();
    }
}

#[inline(never)]
#[unsafe(no_mangle)]
#[unsafe(link_section = ".data.ram_func")]
unsafe fn erase_flash_sector_from_ram(
    connect_internal_flash: unsafe extern "C" fn(),
    flash_exit_xip: unsafe extern "C" fn(),
    flash_range_erase: unsafe extern "C" fn(u32, usize, u32, u8),
    flash_flush_cache: unsafe extern "C" fn(),
    flash_enter_cmd_xip: unsafe extern "C" fn(),
    offset: u32,
) {
    unsafe {
        connect_internal_flash();
        flash_exit_xip();
        flash_range_erase(offset, 4096, 4096, 0x20);
        flash_flush_cache();
        flash_enter_cmd_xip();
    }
}

async fn sensor_task<I2C>(
    sensor: &mut Scd4xAsync<I2C, DataplaneDelay<'_>>,
    timer: &Timer,
    logger: &RefCell<SerialLogger<'_>>,
    flash: &RefCell<Rp2040Flash>,
    log: &RefCell<RollingLog>,
) -> !
where
    I2C: embedded_hal_async::i2c::I2c,
{
    let mut delay = DataplaneDelay::new(timer);
    let mut average = Averager::new();
    let mut page_builder = MeasurementPageBuilder::new();
    let mut replay: Option<ReadReplay> = None;
    let mut deferred_live_page: Option<(u64, [u8; RECORD_PAYLOAD_BYTES])> = None;
    let mut next_log_us = now_us(timer).saturating_add(LOG_PERIOD_US);

    let _ = sensor.stop_periodic_measurement().await;
    let _ = delay.delay_ms(500).await;

    match sensor.start_periodic_measurement().await {
        Ok(()) => logger.borrow_mut().log_status("scd41 started"),
        Err(_) => logger.borrow_mut().log_status("scd41 start failed"),
    }

    loop {
        if let Ok(true) = sensor.data_ready_status().await {
            if let Ok(raw) = sensor.sensor_output().await {
                if let Some(payload) = page_builder.push(raw) {
                    let recovery = log.borrow().recovery();
                    let counter = recovery.next_counter;
                    logger
                        .borrow_mut()
                        .log_storage_append_begin(counter, recovery.next_slot);
                    match log
                        .borrow_mut()
                        .append_page(&mut *flash.borrow_mut(), &payload)
                    {
                        Ok(()) => {
                            if !logger.borrow_mut().log_page(counter, &payload) {
                                deferred_live_page = Some((counter, payload));
                            }
                        }
                        Err(_) => logger.borrow_mut().log_status("storage append failed"),
                    }
                }

                let measurement = measurement_from_raw(raw);
                average.push(
                    measurement.co2,
                    measurement.temperature,
                    measurement.humidity,
                );
            }
        }

        let current_us = now_us(timer);
        if current_us >= next_log_us {
            if let Some(values) = average.take() {
                logger.borrow_mut().log_average(values);
            }
            next_log_us = current_us.saturating_add(LOG_PERIOD_US);
        }

        if let Some(amount) = logger.borrow_mut().take_read_command() {
            let mut logger = logger.borrow_mut();
            let recovery = log.borrow().recovery();
            logger.log_read_begin(amount, recovery.valid_pages, recovery.newest_counter);
            let count = cmp::min(amount, recovery.valid_pages);
            replay = Some(ReadReplay {
                next_age: count.saturating_sub(1),
                remaining: count,
                ok_pending: count == 0,
            });
        }

        if deferred_live_page.is_some() && !logger.borrow().has_pending_line() {
            let (counter, payload) = deferred_live_page.take().unwrap();
            if !logger.borrow_mut().log_page(counter, &payload) {
                deferred_live_page = Some((counter, payload));
            }
        } else if let Some(state) = replay.as_mut() {
            if !logger.borrow().has_pending_line() {
                if state.ok_pending {
                    logger.borrow_mut().log_status("OK");
                    replay = None;
                    DelayFuture::new(timer, POLL_INTERVAL_US).await;
                    continue;
                }

                let result = log.borrow().read_age(&*flash.borrow(), state.next_age);
                let mut logger = logger.borrow_mut();
                match result {
                    Ok(Some(page)) if logger.log_page(page.counter, &page.payload) => {
                        state.remaining = state.remaining.saturating_sub(1);
                        if state.remaining == 0 {
                            state.ok_pending = true;
                        } else {
                            state.next_age = state.next_age.saturating_sub(1);
                        }
                    }
                    Ok(Some(_)) => {}
                    Ok(None) => {
                        state.remaining = state.remaining.saturating_sub(1);
                        if state.remaining == 0 {
                            state.ok_pending = true;
                        } else {
                            state.next_age = state.next_age.saturating_sub(1);
                        }
                    }
                    Err(_) => {
                        logger.log_status("READ failed");
                        replay = None;
                    }
                }
            }
        }

        DelayFuture::new(timer, POLL_INTERVAL_US).await;
    }
}

#[rp2040_hal::entry]
fn main() -> ! {
    let mut pac = pac::Peripherals::take().unwrap();
    let watchdog_reset = watchdog_reset_pending(&pac.WATCHDOG);
    let mut watchdog = Watchdog::new(pac.WATCHDOG);
    let mut hil_recovery = WatchdogBootselRecovery::check_or_enter_bootsel(
        watchdog_reset,
        &mut watchdog,
        WatchdogBootselConfig::default(),
    );
    let clocks = init_clocks_and_plls(
        XOSC_CRYSTAL_FREQ_HZ,
        pac.XOSC,
        pac.CLOCKS,
        pac.PLL_SYS,
        pac.PLL_USB,
        &mut pac.RESETS,
        &mut watchdog,
    )
    .ok()
    .unwrap();

    let sio = Sio::new(pac.SIO);
    let pins = hal::gpio::Pins::new(
        pac.IO_BANK0,
        pac.PADS_BANK0,
        sio.gpio_bank0,
        &mut pac.RESETS,
    );

    let sda_pin: GpioPin<_, FunctionI2c, _> = pins.gpio0.reconfigure();
    let scl_pin: GpioPin<_, FunctionI2c, _> = pins.gpio1.reconfigure();
    let i2c = I2C::i2c0(
        pac.I2C0,
        sda_pin,
        scl_pin,
        400.kHz(),
        &mut pac.RESETS,
        clocks.system_clock.freq(),
    );

    let timer = Timer::new(pac.TIMER, &mut pac.RESETS, &clocks);
    let boot_us = now_us(&timer);
    hil_recovery.start_watchdog(&mut watchdog);
    let delay = DataplaneDelay::new(&timer);
    let mut sensor = Scd4xAsync::new(i2c, delay);

    let usb_bus = UsbBusAllocator::new(UsbBus::new(
        pac.USBCTRL_REGS,
        pac.USBCTRL_DPRAM,
        clocks.usb_clock,
        true,
        &mut pac.RESETS,
    ));
    let serial = SerialPort::new(&usb_bus);
    let strings = [StringDescriptors::default()
        .manufacturer("dataplane")
        .product("RP2040 SCD41 smoke")
        .serial_number("SCD41")];
    let usb_dev = UsbDeviceBuilder::new(&usb_bus, UsbVidPid(0x1209, 0x0001))
        .strings(&strings)
        .unwrap()
        .device_class(USB_CLASS_CDC)
        .build();
    let logger = RefCell::new(SerialLogger {
        #[cfg(feature = "usb-auto-bootsel")]
        usb: UsbCdcAutoBootsel::new(serial, usb_dev),
        #[cfg(not(feature = "usb-auto-bootsel"))]
        serial,
        #[cfg(not(feature = "usb-auto-bootsel"))]
        usb_dev,
        read_command_pos: 0,
        read_amount: 0,
        read_saw_digit: false,
        pending_read: None,
        read_after_digits_ws: false,
        rx_polls: 0,
        rx_bytes: 0,
        rx_errors: 0,
        parsed_reads: 0,
        rx_recent: [0; 8],
        rx_recent_pos: 0,
        pending_line: [0; PAGE_LINE_MAX],
        pending_len: 0,
        pending_pos: 0,
    });
    let flash = RefCell::new(Rp2040Flash);
    let log = RefCell::new(
        RollingLog::recover_or_initialize(&mut *flash.borrow_mut(), STORAGE_GEOMETRY)
            .unwrap_or_else(|_| RollingLog::recover(&Rp2040Flash, STORAGE_GEOMETRY).unwrap()),
    );
    {
        let recovery = log.borrow().recovery();
        logger.borrow_mut().log_storage_recovery(
            recovery.valid_pages,
            recovery.next_counter,
            recovery.next_slot,
        );
    }

    let task = sensor_task(&mut sensor, &timer, &logger, &flash, &log);
    let mut task = core::pin::pin!(task);
    loop {
        logger.borrow_mut().poll();
        let _ = poll_once(task.as_mut());
        let elapsed_us = now_us(&timer).saturating_sub(boot_us);
        hil_recovery.mark_boot_stable_after(&mut watchdog, elapsed_us);
        watchdog.feed();
    }
}

fn poll_once<F>(mut future: Pin<&mut F>) -> Poll<F::Output>
where
    F: Future,
{
    let waker = core::task::Waker::noop();
    let mut cx = Context::from_waker(waker);
    future.as_mut().poll(&mut cx)
}

fn ns_to_us(ns: u32) -> u64 {
    (ns as u64).saturating_add(999) / 1_000
}

fn now_us(timer: &Timer) -> u64 {
    timer.get_counter().ticks()
}
