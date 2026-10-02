//! Portable flash-log and formatting components of the RP2040 SCD41 firmware.
//! Host tests validate codecs/recovery without touching devices. The feature-gated
//! binary owns USB, I2C, watchdog, and flash programming; it is not a host service.
#![cfg_attr(not(test), no_std)]

pub mod formatting;
pub mod rolling_log;
