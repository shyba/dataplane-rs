//! Feature-gated ESP32 clock conversion, without a HAL binding.
//!
//! Callers sample a monotonic SYSTIMER counter and feed it to [`Esp32ClockAdapter`].
//! The adapter exposes elapsed nanoseconds through [`EmbeddedHostAdapter`]. It
//! neither reads hardware nor owns interrupts, sleeps, or allocates per sample.
//! Counter wrap/reset is rejected as a backward sample; callers must extend a
//! wrapping hardware counter before passing ticks here.
//!
//! This host-runtime integration seam is not a bare-metal ESP32 runtime.
#![cfg(feature = "esp32-integration")]
#![forbid(unsafe_code)]

use crate::embedded_host_loop::EmbeddedHostAdapter;

#[cfg(dataplane_verify_default_excludes_esp32)]
compile_error!(
    "esp32 module compiled during default-feature verification; this module must remain excluded without `esp32-integration`"
);

/// Placeholder marker for future ESP32 runtime integration wiring.
///
/// This type exists only to make the feature-gated integration seam explicit.
/// It is not a HAL adapter and does not represent bound ESP32 peripherals.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Esp32IntegrationPlaceholder;

impl Esp32IntegrationPlaceholder {
    /// Returns the integration-slot marker.
    #[must_use]
    pub const fn new() -> Self {
        Self
    }
}

/// Monotonic source identifier for the feature-gated clock adapter.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Esp32ClockSource {
    /// ESP32 system timer counter (`SYSTIMER`) ticks.
    Systimer,
}

/// Explicit bounded failures for adapter-local clock conversion.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Esp32ClockAdapterError {
    /// The configured counter frequency was zero.
    ZeroCounterHz,
    /// A sampled counter value moved backward and would break monotonic output.
    CounterMovedBackward { last_tick: u64, observed_tick: u64 },
}

/// Allocation-free elapsed-time conversion for caller-sampled SYSTIMER ticks.
///
/// Output starts at zero, saturates at `u64::MAX`, and does not depend on sample
/// frequency. The frequency must remain constant. Backward samples return an
/// error without changing state; no wraparound or source fallback is inferred.
/// Hardware handles and any unsafe HAL access remain caller-owned.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Esp32ClockAdapter {
    source: Esp32ClockSource,
    counter_hz: u64,
    initial_tick: u64,
    last_tick: u64,
    now_ns: u64,
}

impl Esp32ClockAdapter {
    /// Creates an ESP32 clock adapter from the selected SYSTIMER tick source.
    ///
    /// `counter_hz` must be the monotonic counter frequency in hertz, and
    /// `initial_tick` must come from the same source that future samples use.
    pub const fn from_systimer(
        counter_hz: u64,
        initial_tick: u64,
    ) -> Result<Self, Esp32ClockAdapterError> {
        if counter_hz == 0 {
            return Err(Esp32ClockAdapterError::ZeroCounterHz);
        }

        Ok(Self {
            source: Esp32ClockSource::Systimer,
            counter_hz,
            initial_tick,
            last_tick: initial_tick,
            now_ns: 0,
        })
    }

    /// Returns the configured monotonic source tag.
    #[must_use]
    pub const fn source(&self) -> Esp32ClockSource {
        self.source
    }

    /// Samples the selected monotonic counter and updates `now_ns`.
    ///
    /// This conversion is adapter-local and keeps runtime boundaries on plain
    /// non-decreasing `u64` nanoseconds.
    pub fn observe_systimer_tick(
        &mut self,
        observed_tick: u64,
    ) -> Result<u64, Esp32ClockAdapterError> {
        if observed_tick < self.last_tick {
            return Err(Esp32ClockAdapterError::CounterMovedBackward {
                last_tick: self.last_tick,
                observed_tick,
            });
        }

        // Convert total elapsed ticks, not each sample's delta: rounding each
        // delta loses fractional nanoseconds and makes time depend on polling rate.
        let elapsed_ticks = observed_tick - self.initial_tick;
        self.last_tick = observed_tick;
        self.now_ns = ticks_to_ns_saturating(elapsed_ticks, self.counter_hz);
        Ok(self.now_ns)
    }

    /// Adapter-local safe boundary for reading one SYSTIMER sample then
    /// applying monotonic conversion.
    ///
    /// `read_tick` is called exactly once. Any unsafe HAL access belongs in the
    /// caller's hardware adapter, outside this safe-only crate.
    pub fn observe_systimer_with_read<F>(
        &mut self,
        mut read_tick: F,
    ) -> Result<u64, Esp32ClockAdapterError>
    where
        F: FnMut() -> u64,
    {
        let observed_tick = read_tick();
        self.observe_systimer_tick(observed_tick)
    }
}

/// Returns the last observed elapsed time; this trait call does not sample hardware.
impl EmbeddedHostAdapter for Esp32ClockAdapter {
    fn now_ns(&mut self) -> u64 {
        self.now_ns
    }
}

const NANOS_PER_SECOND: u128 = 1_000_000_000;

fn ticks_to_ns_saturating(delta_ticks: u64, counter_hz: u64) -> u64 {
    let nanos = (u128::from(delta_ticks) * NANOS_PER_SECOND) / u128::from(counter_hz);
    nanos.min(u128::from(u64::MAX)) as u64
}

#[cfg(all(test, not(target_os = "none")))]
mod tests {
    use super::{
        Esp32ClockAdapter, Esp32ClockAdapterError, Esp32ClockSource, Esp32IntegrationPlaceholder,
    };
    use crate::embedded_host_loop::EmbeddedHostAdapter;
    use core::mem::size_of;

    #[test]
    fn esp32_clock_adapter_systimer_ticks_convert_to_monotonic_ns() {
        let mut clock = Esp32ClockAdapter::from_systimer(1_000_000, 10).expect("valid config");

        assert_eq!(clock.source(), Esp32ClockSource::Systimer);
        assert_eq!(clock.now_ns(), 0);

        let now_ns = clock
            .observe_systimer_tick(510)
            .expect("tick conversion succeeds");
        assert_eq!(now_ns, 500_000);
        assert_eq!(clock.now_ns(), 500_000);

        let now_ns = clock
            .observe_systimer_tick(1_010)
            .expect("tick conversion succeeds");
        assert_eq!(now_ns, 1_000_000);
        assert_eq!(clock.now_ns(), 1_000_000);
    }

    #[test]
    fn sampling_frequency_does_not_accumulate_rounding_error() {
        let mut frequent = Esp32ClockAdapter::from_systimer(3, 10).unwrap();
        let mut once = frequent;
        for tick in 11..=13 {
            frequent.observe_systimer_tick(tick).unwrap();
        }
        assert_eq!(frequent.now_ns(), 1_000_000_000);
        assert_eq!(once.observe_systimer_tick(13).unwrap(), frequent.now_ns());
        assert!(frequent.observe_systimer_tick(12).is_err());
        assert_eq!(frequent.now_ns(), 1_000_000_000);
        assert_eq!(frequent.observe_systimer_tick(16).unwrap(), 2_000_000_000);
    }

    #[test]
    fn esp32_clock_adapter_rejects_backward_tick_samples() {
        let mut clock = Esp32ClockAdapter::from_systimer(1_000_000, 100).expect("valid config");
        let err = clock
            .observe_systimer_tick(99)
            .expect_err("backward sample must fail");

        assert_eq!(
            err,
            Esp32ClockAdapterError::CounterMovedBackward {
                last_tick: 100,
                observed_tick: 99
            }
        );
    }

    #[test]
    fn esp32_clock_adapter_rejects_zero_counter_frequency() {
        let err = Esp32ClockAdapter::from_systimer(0, 0).expect_err("must fail");
        assert_eq!(err, Esp32ClockAdapterError::ZeroCounterHz);
    }

    #[test]
    fn esp32_integration_placeholder_is_zero_sized_marker() {
        assert_eq!(size_of::<Esp32IntegrationPlaceholder>(), 0);
    }

    #[test]
    fn esp32_clock_adapter_runtime_surface_is_embedded_host_adapter_only() {
        fn runtime_reads_now_ns_only<T: EmbeddedHostAdapter>(adapter: &mut T) -> u64 {
            adapter.now_ns()
        }

        let mut clock = Esp32ClockAdapter::from_systimer(1_000_000, 0).expect("valid config");
        clock
            .observe_systimer_tick(250)
            .expect("tick conversion succeeds");

        assert_eq!(runtime_reads_now_ns_only(&mut clock), 250_000);
    }

    #[test]
    fn esp32_clock_adapter_tick_to_ns_conversion_saturates_without_wraparound() {
        let mut clock = Esp32ClockAdapter::from_systimer(1, 0).expect("valid config");

        let now_ns = clock
            .observe_systimer_tick(u64::MAX)
            .expect("tick conversion succeeds");

        assert_eq!(now_ns, u64::MAX);
        assert_eq!(clock.now_ns(), u64::MAX);
    }

    #[test]
    fn esp32_clock_adapter_observe_with_read_uses_safe_adapter_boundary() {
        let mut clock = Esp32ClockAdapter::from_systimer(1_000_000, 10).expect("valid config");
        let mut sampled = false;

        let now_ns = clock
            .observe_systimer_with_read(|| {
                sampled = true;
                1_010
            })
            .expect("tick conversion succeeds");

        assert!(sampled, "clock sample closure must be invoked once");
        assert_eq!(now_ns, 1_000_000);
        assert_eq!(clock.now_ns(), 1_000_000);
    }
}
