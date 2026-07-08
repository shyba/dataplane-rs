#[cfg(feature = "rp2040-integration")]
pub use dataplane_core_reactor::{future_task, local_exec, local_exec_counts};

#[cfg(feature = "rp2040-hil-recovery")]
pub mod hil_recovery {
    use crate::rp2040_recovery::{
        plan_recovery, RecoveryDecision, RecoverySnapshot, RECOVERY_MAGIC,
    };

    use rp2040_hal as hal;

    use hal::fugit::ExtU32;
    use hal::pac;
    use hal::watchdog::{ScratchRegister, Watchdog};

    pub const DEFAULT_BOOTSEL_AFTER_WATCHDOG_RESETS: u32 = 3;
    pub const DEFAULT_WATCHDOG_TIMEOUT_US: u32 = 2_000_000;
    pub const DEFAULT_STABLE_AFTER_US: u64 = 15_000_000;

    #[derive(Clone, Copy, Debug, Eq, PartialEq)]
    pub struct WatchdogBootselConfig {
        pub bootsel_after_watchdog_resets: u32,
        pub watchdog_timeout_us: u32,
        pub stable_after_us: u64,
    }

    impl WatchdogBootselConfig {
        pub const fn disabled() -> Self {
            Self {
                bootsel_after_watchdog_resets: 0,
                watchdog_timeout_us: 0,
                stable_after_us: 0,
            }
        }
    }

    impl Default for WatchdogBootselConfig {
        fn default() -> Self {
            Self {
                bootsel_after_watchdog_resets: DEFAULT_BOOTSEL_AFTER_WATCHDOG_RESETS,
                watchdog_timeout_us: DEFAULT_WATCHDOG_TIMEOUT_US,
                stable_after_us: DEFAULT_STABLE_AFTER_US,
            }
        }
    }

    pub struct WatchdogBootselRecovery {
        config: WatchdogBootselConfig,
        watchdog_resets: u32,
        boot_stable: bool,
    }

    impl WatchdogBootselRecovery {
        pub fn check_or_enter_bootsel(
            watchdog_reset: bool,
            watchdog: &mut Watchdog,
            config: WatchdogBootselConfig,
        ) -> Self {
            let snapshot = RecoverySnapshot {
                watchdog_reset,
                magic: watchdog.read_scratch(ScratchRegister::Scratch0),
                watchdog_resets: watchdog.read_scratch(ScratchRegister::Scratch1),
            };
            let plan = plan_recovery(snapshot, config.bootsel_after_watchdog_resets);

            watchdog.write_scratch(ScratchRegister::Scratch0, plan.next_magic);
            watchdog.write_scratch(ScratchRegister::Scratch1, plan.next_watchdog_resets);

            if plan.decision == RecoveryDecision::EnterBootsel {
                watchdog.write_scratch(ScratchRegister::Scratch0, 0);
                watchdog.write_scratch(ScratchRegister::Scratch1, 0);
                reset_to_usb_boot();
            }

            Self {
                config,
                watchdog_resets: plan.next_watchdog_resets,
                boot_stable: false,
            }
        }

        pub fn start_watchdog(&self, watchdog: &mut Watchdog) {
            if self.config.watchdog_timeout_us != 0 {
                watchdog.start(self.config.watchdog_timeout_us.micros());
            }
        }

        pub fn mark_boot_stable_after(&mut self, watchdog: &mut Watchdog, elapsed_us: u64) -> bool {
            if self.boot_stable || elapsed_us < self.config.stable_after_us {
                return false;
            }

            self.mark_boot_stable(watchdog);
            true
        }

        pub fn mark_boot_stable(&mut self, watchdog: &mut Watchdog) {
            watchdog.write_scratch(ScratchRegister::Scratch0, RECOVERY_MAGIC);
            watchdog.write_scratch(ScratchRegister::Scratch1, 0);
            self.watchdog_resets = 0;
            self.boot_stable = true;
        }

        pub const fn watchdog_resets(&self) -> u32 {
            self.watchdog_resets
        }

        pub const fn boot_stable(&self) -> bool {
            self.boot_stable
        }
    }

    pub fn watchdog_reset_pending(watchdog: &pac::WATCHDOG) -> bool {
        watchdog.reason().read().timer().bit_is_set()
    }

    pub fn reset_to_usb_boot() -> ! {
        hal::rom_data::reset_to_usb_boot(0, 0)
    }
}

#[cfg(feature = "rp2040-usb-auto-bootsel")]
pub mod usb_auto_bootsel {
    use core::borrow::BorrowMut;

    use usb_device::bus::UsbBus;
    use usb_device::device::UsbDevice;
    use usb_device::{Result, UsbError};
    use usbd_serial::{DefaultBufferStore, SerialPort};

    use super::hil_recovery::reset_to_usb_boot;

    pub const USB_BOOT_BAUD: u32 = 1200;
    pub const USB_BOOT_COMMAND: &[u8] = b"BOOTSEL";

    pub struct UsbCdcAutoBootsel<'a, B, RS = DefaultBufferStore, WS = DefaultBufferStore>
    where
        B: UsbBus,
        RS: BorrowMut<[u8]>,
        WS: BorrowMut<[u8]>,
    {
        serial: SerialPort<'a, B, RS, WS>,
        usb_dev: UsbDevice<'a, B>,
        command_pos: usize,
        enabled: bool,
    }

    impl<'a, B, RS, WS> UsbCdcAutoBootsel<'a, B, RS, WS>
    where
        B: UsbBus,
        RS: BorrowMut<[u8]>,
        WS: BorrowMut<[u8]>,
    {
        pub const fn new(serial: SerialPort<'a, B, RS, WS>, usb_dev: UsbDevice<'a, B>) -> Self {
            Self {
                serial,
                usb_dev,
                command_pos: 0,
                enabled: true,
            }
        }

        pub const fn disabled(
            serial: SerialPort<'a, B, RS, WS>,
            usb_dev: UsbDevice<'a, B>,
        ) -> Self {
            Self {
                serial,
                usb_dev,
                command_pos: 0,
                enabled: false,
            }
        }

        pub fn set_auto_bootsel_enabled(&mut self, enabled: bool) {
            self.enabled = enabled;
            if !enabled {
                self.command_pos = 0;
            }
        }

        pub const fn auto_bootsel_enabled(&self) -> bool {
            self.enabled
        }

        pub fn poll(&mut self) -> bool {
            let active = self.usb_dev.poll(&mut [&mut self.serial]);
            self.reset_to_bootsel_if_requested();
            active
        }

        pub fn read(&mut self, data: &mut [u8]) -> Result<usize> {
            self.reset_to_bootsel_if_requested();
            match self.serial.read(data) {
                Ok(count) => {
                    self.accept_rx_bytes(&data[..count]);
                    Ok(count)
                }
                Err(err) => Err(err),
            }
        }

        pub fn write(&mut self, data: &[u8]) -> Result<usize> {
            self.reset_to_bootsel_if_requested();
            self.serial.write(data)
        }

        pub fn flush(&mut self) -> Result<()> {
            self.reset_to_bootsel_if_requested();
            self.serial.flush()
        }

        pub fn serial(&self) -> &SerialPort<'a, B, RS, WS> {
            &self.serial
        }

        pub fn usb_device(&self) -> &UsbDevice<'a, B> {
            &self.usb_dev
        }

        pub fn accept_rx_bytes(&mut self, bytes: &[u8]) {
            if !self.enabled {
                return;
            }

            for byte in bytes {
                if *byte == USB_BOOT_COMMAND[self.command_pos] {
                    self.command_pos += 1;
                    if self.command_pos == USB_BOOT_COMMAND.len() {
                        reset_to_usb_boot();
                    }
                    continue;
                }

                self.command_pos = usize::from(*byte == USB_BOOT_COMMAND[0]);
            }
        }

        fn reset_to_bootsel_if_requested(&mut self) {
            if self.enabled && self.serial.line_coding().data_rate() == USB_BOOT_BAUD {
                reset_to_usb_boot();
            }
        }
    }

    impl<'a, B, RS, WS> UsbCdcAutoBootsel<'a, B, RS, WS>
    where
        B: UsbBus,
        RS: BorrowMut<[u8]>,
        WS: BorrowMut<[u8]>,
    {
        pub fn drain_readable(
            &mut self,
            buf: &mut [u8],
            mut on_bytes: impl FnMut(&[u8]),
        ) -> Result<usize> {
            let mut total = 0usize;
            loop {
                match self.read(buf) {
                    Ok(0) => return Ok(total),
                    Ok(count) => {
                        total = total.saturating_add(count);
                        on_bytes(&buf[..count]);
                    }
                    Err(UsbError::WouldBlock) => return Ok(total),
                    Err(err) => return Err(err),
                }
            }
        }
    }
}
