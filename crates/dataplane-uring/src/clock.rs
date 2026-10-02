//! Clock helpers for the runtime.
//!
//! All raw receive timestamps must come from [`recv_clock_raw`]; raw clock
//! ticks are not nanoseconds and must not be mixed with another clock's epoch.

use std::sync::OnceLock;

// Shared receive-side monotonic clock (for recv timeout comparisons).
static RECEIVE_CLOCK: OnceLock<quanta::Clock> = OnceLock::new();

/// Returns the shared receive-side monotonic clock.
pub fn recv_clock() -> &'static quanta::Clock {
    RECEIVE_CLOCK.get_or_init(quanta::Clock::new)
}

/// Returns the raw tick value of the receive clock.
pub fn recv_clock_raw() -> u64 {
    recv_clock().raw()
}

/// Returns the elapsed nanoseconds since the given enqueued timestamp.
pub fn recv_wait_ns_since(enqueued_raw: u64) -> u64 {
    recv_clock().delta_as_nanos(enqueued_raw, recv_clock_raw())
}
