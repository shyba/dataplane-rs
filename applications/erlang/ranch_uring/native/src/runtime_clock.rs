//! Clock helpers for the runtime.
//!
//! This module extracts clock-related helpers from runtime_helpers.
//! Declared as a #[path] submodule of runtime, so it can be accessed
//! from runtime_helpers via `super::runtime_clock::*`.

use std::sync::OnceLock;

// Shared receive-side monotonic clock (for recv timeout comparisons).
static RECEIVE_CLOCK: OnceLock<quanta::Clock> = OnceLock::new();

/// Returns the shared receive-side monotonic clock.
pub(crate) fn recv_clock() -> &'static quanta::Clock {
    RECEIVE_CLOCK.get_or_init(quanta::Clock::new)
}

/// Returns the raw tick value of the receive clock.
pub(crate) fn recv_clock_raw() -> u64 {
    recv_clock().raw()
}

/// Returns the elapsed nanoseconds since the given enqueued timestamp.
pub(crate) fn recv_wait_ns_since(enqueued_raw: u64) -> u64 {
    recv_clock().delta_as_nanos(enqueued_raw, recv_clock_raw())
}
