#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DriverBackendKind {
    IoUring,
    Syscall,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Driver feature flags surfaced to runtime policy layers.
///
/// Embedded M9J contract (DP-EMB-0091):
/// - An ESP32 embedded driver is expected to report conservative capabilities:
///   `supports_accept_multi = false`, `supports_multishot = false`,
///   `supports_fixed_buffers = false`, and `supports_sqpoll = false`
///   unless a concrete target integration proves otherwise.
/// - `backend` is informational for runtime diagnostics/policy and does not
///   authorize OS-specific behavior in host-neutral embedded paths.
pub struct DriverCapabilities {
    pub backend: DriverBackendKind,
    pub supports_accept_multi: bool,
    pub supports_multishot: bool,
    pub supports_fixed_buffers: bool,
    pub supports_sqpoll: bool,
}

/// Minimal reactor-driver contract consumed by the runtime core.
///
/// Embedded M9J contract (DP-EMB-0091):
/// - Current embedded host-loop paths require a driver that implements
///   `ReactorDriver` and `ReactorDriverWait`.
/// - For the current network runtime shape, associated types must align with
///   runtime usage (`Event = NetEvent`, `Token = OpToken` at call sites).
/// - Required driver capabilities are expressed by this trait surface:
///   `submit`, `flush`, `drain`, `outstanding`, and `capabilities`.
/// - Keep implementations safe-Rust-first and host-neutral at this layer
///   (no HAL handles or target-specific wait policy in runtime-core APIs).
pub trait ReactorDriver {
    type Error;
    type Token: Copy + Eq;
    type Submit;
    type Event;

    /// Submits one operation to the backend with an associated token.
    ///
    /// The token is passed through unchanged to the runtime's completion path;
    /// the driver does not retain or consume the token value. The caller is
    /// responsible for ensuring the token remains valid until the completion
    /// is observed via `drain`.
    ///
    /// Ownership contract:
    /// - The operation (`op`) may be consumed or moved by the implementation.
    /// - The token is not consumed: it remains owned by the caller and is
    ///   only presented to `on_event` callbacks as associated metadata.
    /// - After a successful `submit`, the caller may drop `op` and reuse or
    ///   drop the token; the driver guarantees the token value will be
    ///   delivered to `on_event` at most once.
    ///
    /// Error contract:
    /// - `Err(e)` means the operation was not queued and no completion will
    ///   be emitted for this `op`/`token` pair.
    /// - Implementations must not emit a completion for a failed `submit`.
    fn submit(&mut self, op: Self::Submit, token: Self::Token) -> Result<(), Self::Error>;
    /// Pushes queued submissions to the backend and returns how many were
    /// submitted in this call.
    ///
    /// Return-count contract:
    /// - `0` means no new submissions were flushed by this call.
    /// - `n > 0` means exactly `n` submissions moved from queued to submitted.
    fn flush(&mut self) -> Result<usize, Self::Error>;
    /// Drains up to `max_events` backend events and invokes `on_event` once per
    /// emitted event.
    ///
    /// Return-count contract:
    /// - `0` means this call emitted no events.
    /// - `n > 0` means exactly `n` events were emitted to `on_event`.
    /// - Implementations must keep the return value consistent with callback
    ///   invocations.
    fn drain<F>(&mut self, max_events: usize, on_event: F) -> Result<usize, Self::Error>
    where
        F: FnMut(Self::Event);
    /// Returns the number of operations submitted through `submit` that have
    /// not yet been delivered as completions via `drain`.
    ///
    /// Count semantics:
    /// - `0` means no outstanding operations: all prior submissions have
    ///   either been completed via `drain` or the implementation has
    ///   guaranteed delivery will never occur.
    /// - `n > 0` means exactly `n` operation/token pairs are pending
    ///   completion. The value is monotonically non-increasing after each
    ///   call to `drain` that returns `n_drained > 0`, assuming no new
    ///   `submit` calls occur between drain invocations.
    ///
    /// Consistency rule:
    /// - If `submit` returned `Ok(())` for an operation, that operation
    ///   must eventually either:
    ///   a) appear in an `on_event` callback from a subsequent `drain`, or
    ///   b) be reflected in `outstanding` until such a callback occurs.
    /// - Implementations must not silently discard a successfully submitted
    ///   operation without reflecting it in `outstanding` until the operation
    ///   is accounted for.
    ///
    /// This value is advisory: callers use it to estimate backend backlog
    /// depth. It is not used to derive correctness guarantees in the runtime.
    fn outstanding(&self) -> usize;
    fn capabilities(&self) -> DriverCapabilities;
}

/// Optional blocking wait contract used by host-loop wait paths.
///
/// Implement this trait for drivers that are used with runtime paths calling
/// `ReactorRuntime::drain_or_wait` / host-loop wait helpers. Drivers that are
/// only used in pure poll/tick mode may omit this trait.
pub trait ReactorDriverWait {
    type Error;
    type Readiness;

    /// Returns backend readiness metadata when available.
    ///
    /// Returning `None` is valid for drivers without explicit readiness handles.
    fn readiness(&self) -> Option<Self::Readiness>;
    /// Waits until at least `min_events` may be available and returns how many
    /// events became ready because of this wait operation.
    ///
    /// Return-count contract:
    /// - `0` means the wait returned without making events ready.
    /// - `n > 0` means this wait made `n` events ready for subsequent drains.
    fn wait(&mut self, min_events: usize) -> Result<usize, Self::Error>;
}

#[cfg(test)]
mod tests {
    use super::{DriverBackendKind, DriverCapabilities, ReactorDriver, ReactorDriverWait};

    struct SurfaceGuardDriver;

    impl ReactorDriver for SurfaceGuardDriver {
        type Error = core::convert::Infallible;
        type Token = u64;
        type Submit = ();
        type Event = ();

        fn submit(&mut self, _op: Self::Submit, _token: Self::Token) -> Result<(), Self::Error> {
            Ok(())
        }

        fn flush(&mut self) -> Result<usize, Self::Error> {
            Ok(0)
        }

        fn drain<F>(&mut self, _max_events: usize, _on_event: F) -> Result<usize, Self::Error>
        where
            F: FnMut(Self::Event),
        {
            Ok(0)
        }

        fn outstanding(&self) -> usize {
            0
        }

        fn capabilities(&self) -> DriverCapabilities {
            DriverCapabilities {
                backend: DriverBackendKind::Syscall,
                supports_accept_multi: false,
                supports_multishot: false,
                supports_fixed_buffers: false,
                supports_sqpoll: false,
            }
        }
    }

    impl ReactorDriverWait for SurfaceGuardDriver {
        type Error = core::convert::Infallible;
        type Readiness = ();

        fn readiness(&self) -> Option<Self::Readiness> {
            None
        }

        fn wait(&mut self, _min_events: usize) -> Result<usize, Self::Error> {
            Ok(0)
        }
    }

    #[test]
    fn reactor_driver_trait_surface_guard_compiles_without_extensions() {
        fn assert_surface<D: ReactorDriver + ReactorDriverWait>(_driver: &D) {}

        let driver = SurfaceGuardDriver;
        assert_surface(&driver);
    }
}
