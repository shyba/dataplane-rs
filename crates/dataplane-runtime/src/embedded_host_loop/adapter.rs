pub trait EmbeddedHostAdapter {
    /// Required monotonic time source consumed by runtime drive calls.
    ///
    /// Implementations should return a non-decreasing nanosecond timestamp.
    /// The value source is target-owned (for example, host clock or MCU timer).
    ///
    /// Target-adapter guidance:
    /// - Keep timer/counter access inside the adapter implementation.
    /// - Convert target timer units to nanoseconds in the adapter, then return `u64`.
    /// - Do not pass target-specific handles, peripheral state, or platform types
    ///   into runtime-core APIs; runtime only consumes this plain monotonic timestamp.
    ///
    /// This preserves the M9 host-neutral boundary: target integration stays at
    /// the adapter edge while runtime logic remains portable.
    fn now_ns(&mut self) -> u64;

    /// Optional idle callback for no-progress drive steps.
    ///
    /// This is policy only: the runtime does not require blocking sleep here.
    /// Making sleep mandatory would break the M9 host-neutral boundary by forcing
    /// one wait model into every target, including cooperative host loops that
    /// must stay responsive to other device work.
    ///
    /// The default implementation is a no-op, so drive calls remain non-blocking
    /// unless a host intentionally adds wait/backoff behavior. Targets that need
    /// sleep, yield, or low-power idle can still do so here as explicit host policy.
    ///
    /// # Example
    ///
    /// A host adapter can stay non-blocking and simply record idle notifications:
    ///
    /// ```rust
    /// use dataplane_runtime::embedded_host_loop::EmbeddedHostAdapter;
    ///
    /// #[derive(Default)]
    /// struct IdleRecordingHost {
    ///     now_ns: u64,
    ///     idle_notifications: usize,
    /// }
    ///
    /// impl EmbeddedHostAdapter for IdleRecordingHost {
    ///     fn now_ns(&mut self) -> u64 {
    ///         self.now_ns
    ///     }
    ///
    ///     fn on_idle(&mut self) {
    ///         self.idle_notifications += 1;
    ///     }
    /// }
    /// ```
    #[inline]
    fn on_idle(&mut self) {}
}
