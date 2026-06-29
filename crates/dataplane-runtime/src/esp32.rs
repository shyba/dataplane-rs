//! ESP32 integration slot for the `esp32-integration` feature gate.
//!
//! This module is intentionally a safe placeholder only.
//! The feature gate reserves an integration seam for future ESP32 wiring but
//! is not an ESP32 HAL binding at this stage.
//! No HAL crates, peripheral handles, target-specific APIs, or unsafe blocks
//! are introduced here until a concrete integration task is approved.
//! In particular, keep this module free of imports from `esp-hal`, `embassy`,
//! `embedded-hal`, and `esp-idf-hal` until a single first target crate is
//! selected and tracked by a dedicated integration story.
//! Integration decision (DP-EMB-0181):
//! - first lane is the local trait placeholder path (this module +
//!   `EmbeddedHostAdapter` seam), not direct `esp-hal` or `embassy` binding
//! - defer choosing one concrete HAL/runtime crate until the adapter contract,
//!   timer source requirements, and interrupt/wake semantics are documented and
//!   validated under the feature gate
//! - keep any required unsafe invariants outside this crate until that concrete
//!   integration point exists
//! M12C dependency-introduction guard:
//! - dependency additions in this lane are compile-time wiring only and must
//!   not change runtime host-loop behavior, scheduler policy, or I/O semantics
//! M12D runtime-core modification guard:
//! - keep ESP32 adapter implementation scoped to this feature-gated
//!   `dataplane-runtime` module and `EmbeddedHostAdapter` wiring only
//! - do not directly modify `dataplane-core-reactor` scheduler, host-loop, or
//!   ingress/task primitives while implementing this adapter lane
//! Any target-specific `unsafe` required by HAL call sites is out of scope
//! for this placeholder and may only be introduced at a concrete HAL
//! integration point.
//! Required unsafe-callsite documentation contract for that future HAL step:
//! - each `unsafe` call site must carry a short `SAFETY:` note at the point of
//!   use that states the exact HAL preconditions being relied on
//! - each call site must name the owned resource boundary
//!   (clock register block, interrupt source, wake token, or descriptor/handle)
//!   and why aliasing/lifetime rules remain valid
//! - each call site must state bounded-failure behavior
//!   (what happens if the HAL read/write fails or data is stale) and confirm no
//!   hidden unbounded retry/queue growth is introduced
//! - each call site must be adapter-local under `esp32-integration`; do not
//!   widen runtime-core traits or cross-crate ownership boundaries to carry
//!   target-specific unsafe assumptions
//! - this module currently forbids `unsafe` entirely (`#![forbid(unsafe_code)]`);
//!   these rules are predeclared documentation for the follow-up HAL packet
//! M9J guard:
//! - do not add ESP32 I/O operation surface area (socket, driver submit/poll,
//!   peripheral I/O traits, or async I/O wrappers) before the host-loop
//!   skeleton and driver trait boundary are proven stable by concrete callers.
//! Deferred follow-up task for later HAL-backed driver prototype:
//! - after a concrete embedded caller proves the current trait boundary is the
//!   next blocker, add a dedicated integration story to prototype one
//!   HAL-backed `ReactorDriver<NetEvent, OpToken> + ReactorDriverWait`
//!   implementation behind `esp32-integration`
//! - keep that prototype compile-gated, safe-Rust-first, and adapter-local;
//!   do not widen generic runtime traits during the first prototype step
//! - capture exact HAL/toolchain command(s), observed stderr signatures, and
//!   the smallest next unblocking step before any runtime-core changes
//! Deferred follow-up task for ESP32-specific example coverage:
//! - after the first concrete HAL integration lane is selected and compiled
//!   under `esp32-integration`, add one minimal rustdoc example in this module
//!   showing adapter-local `now_ns` wiring from the chosen monotonic source
//! - keep the example target-neutral at the runtime boundary (plain `u64`
//!   nanoseconds into `EmbeddedHostAdapter`) and avoid widening runtime traits
//! - include the exact feature-gated validation command used for the example
//!   when updating the execution ledger
//!
//! Compile-check sentinel:
//! - when `RUSTFLAGS="--cfg dataplane_verify_default_excludes_esp32"` is set,
//!   this module emits a compile error if it is compiled at all
//! - this allows a focused default-feature `cargo check` to prove the
//!   `esp32-integration` gate excludes ESP32 module code by default
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

/// Placeholder clock adapter slot for future ESP32 host-time wiring.
///
/// This type exists only behind `esp32-integration` to reserve the runtime-facing
/// adapter seam. It intentionally carries no HAL imports or peripheral state.
///
/// Expected monotonic nanosecond semantics for the eventual adapter wiring:
/// - return a non-decreasing `u64` nanosecond timestamp on every `now_ns` read
/// - use a monotonic elapsed-time source (not wall-clock or timezone time)
/// - keep raw HAL counter reads and tick-to-nanosecond conversion inside the
///   adapter edge, then pass only plain `u64` nanoseconds into runtime APIs
/// - if conversion or accumulation reaches numeric limits, clamp with saturating
///   arithmetic so reported values do not move backward
///
/// Required timer-source contract for the selected first integration lane
/// (local trait placeholder + `EmbeddedHostAdapter` seam):
/// - select one primary monotonic hardware counter for `now_ns` and keep that
///   selection adapter-local behind `esp32-integration`
/// - current required direction for first bring-up: use the `SYSTIMER` counter
///   path as primary, with timer-group counters only as optional fallback if
///   deterministic preference order is documented
/// - record counter frequency assumptions and the exact tick-to-nanosecond
///   conversion formula used by `now_ns`
/// - record wraparound handling rules that preserve non-decreasing `u64` output
/// - if fallback is kept, record deterministic source preference so behavior is
///   stable across builds and boot modes
///
/// Required interrupt/wake semantics for the same selected first lane:
/// - keep wake signaling adapter-local under `esp32-integration`; runtime-core
///   boundaries continue to observe only host-loop progress (`step_with_host`,
///   `drive_steps_with_host`) and plain `u64` time from `now_ns`
/// - represent interrupts as edge notifications that request another bounded
///   host-loop step instead of introducing a blocking wait contract in runtime
/// - if multiple interrupt sources can fire for one wake epoch, coalesce to a
///   single pending wake signal so host progress remains bounded and deterministic
/// - define and document clear ack/clear ordering for wake sources so each
///   interrupt causes at least one subsequent host-loop step without duplicate
///   mandatory work
/// - if no interrupt arrives, caller policy may still drive periodic bounded
///   steps via `on_idle`; no mandatory sleep/wait behavior is added to runtime
///
/// Required allocation policy for the same selected first lane:
/// - keep this placeholder lane allocation-neutral at runtime boundaries:
///   no allocation exceptions, no allocator hooks, and no runtime-core API
///   widening for target-specific allocation behavior
/// - require adapter-local wake/time bookkeeping to use bounded state only;
///   do not depend on per-step heap growth in `now_ns`, wake signaling, or
///   host-loop drive paths
/// - if adapter state is needed, provision it once during setup and keep
///   step-time behavior deterministic under fixed capacity
/// - if capacity is exceeded, use explicit bounded behavior
///   (drop/coalesce/fail-fast with recorded reason) rather than hidden
///   retries or unbounded queues
/// - keep allocation-policy decisions behind `esp32-integration` until one
///   concrete HAL/runtime crate is selected and validated
///
/// Required minimum target triple and toolchain expectations for the same
/// selected first lane:
/// - use `xtensa-esp32-none-elf` as the minimum target triple for ESP32
///   feature-path compile checks in this lane
/// - require an ESP32-capable Rust toolchain + target sysroot able to build
///   `core` for `xtensa-esp32-none-elf` before treating target compile checks
///   as blocking gates
/// - keep default host builds/tests target-neutral; when local ESP32 toolchain
///   support is unavailable, record the exact attempted command, stderr
///   signature, and next unblocking step in the active execution ledger before
///   introducing runtime-core changes
///
/// Compile documentation for hosts without ESP32 toolchain support:
/// - preferred target compile-only check (when toolchain support exists):
///   `cargo check -p dataplane-runtime --features esp32-integration --target xtensa-esp32-none-elf --lib`
/// - expected unsupported-toolchain signatures include:
///   `can't find crate for 'core'`, `target may not be installed`,
///   `is not a recognized processor`, and cross-compilation `pkg-config`
///   setup failures
/// - if unsupported locally, run host fallback compile-only check instead:
///   `cargo check -p dataplane-runtime --features esp32-integration --lib`
/// - convenience wrapper for this policy:
///   `tools/check_esp32_feature_gate_compile.sh`
/// First selected monotonic source for the ESP32 feature-gated adapter lane.
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

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Esp32ClockAdapter {
    source: Esp32ClockSource,
    counter_hz: u64,
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

        let delta_ticks = observed_tick - self.last_tick;
        self.last_tick = observed_tick;
        self.now_ns = self
            .now_ns
            .saturating_add(ticks_to_ns_saturating(delta_ticks, self.counter_hz));
        Ok(self.now_ns)
    }

    /// Adapter-local safe boundary for reading one SYSTIMER sample then
    /// applying monotonic conversion.
    ///
    /// If a future HAL call requires `unsafe`, keep that `unsafe` in the
    /// smallest private helper used by `read_tick`, and keep this safe wrapper
    /// as the runtime-facing entry point.
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

/// Runtime-facing boundary for the ESP32 clock adapter remains the
/// [`EmbeddedHostAdapter`] trait only.
///
/// Adapter-local helpers such as [`Self::from_systimer`] and
/// [`Self::observe_systimer_tick`] exist to ingest raw hardware ticks, but
/// runtime-core integration continues to consume plain monotonic `u64`
/// nanoseconds via [`EmbeddedHostAdapter::now_ns`] without widening trait
/// coupling in this lane. Direct runtime-core behavior edits stay out of
/// scope for this adapter implementation packet.
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
