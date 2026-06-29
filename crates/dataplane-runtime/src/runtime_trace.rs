#[cfg(debug_assertions)]
fn configured_debug_phase_stats() -> bool {
    matches!(
        std::env::var("RANCH_URING_DEBUG_PHASE_STATS")
        .ok()
        .as_deref()
        .map(|s| s.trim().to_ascii_lowercase()),
        Some(ref v) if v == "1" || v == "true" || v == "yes" || v == "on"
    )
}

#[cfg(debug_assertions)]
fn configured_debug_phase_every() -> u64 {
    std::env::var("RANCH_URING_DEBUG_PHASE_EVERY")
        .ok()
        .and_then(|s| s.parse::<u64>().ok())
        .filter(|v| *v > 0)
        .unwrap_or(10_000)
}

#[cfg(debug_assertions)]
fn monotonic_ns() -> u64 {
    use std::sync::OnceLock;
    use std::time::Instant;

    static START: OnceLock<Instant> = OnceLock::new();
    let start = START.get_or_init(Instant::now);
    Instant::now()
        .duration_since(*start)
        .as_nanos()
        .try_into()
        .unwrap_or(u64::MAX)
}

#[cfg(debug_assertions)]
const PHASE_LABEL: &str = "ranch_uring phase";

#[cfg(debug_assertions)]
#[derive(Clone, Copy)]
pub enum LoopPhase {
    DrainCommands,
    CqeLatency,
    CqeMain,
    ReduceAge,
    DrainReady,
    SubmitQueued,
    WaitMain,
    WaitLatency,
    Spin,
}

#[cfg(debug_assertions)]
impl LoopPhase {
    const fn idx(self) -> usize {
        match self {
            LoopPhase::DrainCommands => 0,
            LoopPhase::CqeLatency => 1,
            LoopPhase::CqeMain => 2,
            LoopPhase::ReduceAge => 3,
            LoopPhase::DrainReady => 4,
            LoopPhase::SubmitQueued => 5,
            LoopPhase::WaitMain => 6,
            LoopPhase::WaitLatency => 7,
            LoopPhase::Spin => 8,
        }
    }
}

#[cfg(debug_assertions)]
const LOOP_PHASE_COUNT: usize = 9;

#[cfg(debug_assertions)]
#[derive(Clone, Copy)]
pub enum StallReason {
    IdleNoInflight,
    WaitMain,
    WaitLatency,
    Spin,
}

#[cfg(debug_assertions)]
#[derive(Default)]
pub struct DebugPhaseStats {
    enabled: bool,
    every: u64,
    tick: u64,
    phase_hits: [u64; LOOP_PHASE_COUNT],
    phase_ns: [u64; LOOP_PHASE_COUNT],
    progress_loops: u64,
    no_progress_loops: u64,
    stall_idle_no_inflight: u64,
    stall_wait_main: u64,
    stall_wait_latency: u64,
    stall_spin: u64,
}

#[cfg(debug_assertions)]
impl DebugPhaseStats {
    pub fn configured() -> Self {
        Self {
            enabled: configured_debug_phase_stats(),
            every: configured_debug_phase_every(),
            tick: 0,
            ..Self::default()
        }
    }

    pub fn record_phase(&mut self, phase: LoopPhase, start_ns: u64) {
        if !self.enabled {
            return;
        }
        let idx = phase.idx();
        self.phase_hits[idx] = self.phase_hits[idx].saturating_add(1);
        self.phase_ns[idx] =
            self.phase_ns[idx].saturating_add(monotonic_ns().saturating_sub(start_ns));
    }

    pub fn record_progress(&mut self, progressed: bool) {
        if !self.enabled {
            return;
        }
        if progressed {
            self.progress_loops = self.progress_loops.saturating_add(1);
        } else {
            self.no_progress_loops = self.no_progress_loops.saturating_add(1);
        }
    }

    pub fn record_stall(&mut self, reason: StallReason) {
        if !self.enabled {
            return;
        }
        match reason {
            StallReason::IdleNoInflight => {
                self.stall_idle_no_inflight = self.stall_idle_no_inflight.saturating_add(1)
            }
            StallReason::WaitMain => self.stall_wait_main = self.stall_wait_main.saturating_add(1),
            StallReason::WaitLatency => {
                self.stall_wait_latency = self.stall_wait_latency.saturating_add(1)
            }
            StallReason::Spin => self.stall_spin = self.stall_spin.saturating_add(1),
        }
    }

    #[allow(clippy::too_many_arguments)]
    pub fn maybe_log(
        &mut self,
        shard: usize,
        mode: &'static str,
        sqpoll: bool,
        pending_cmds: usize,
        ready_depth: usize,
        write_ready_depth: usize,
        queued_sqes: usize,
        latency_inflight: usize,
        main_inflight: usize,
    ) {
        if !self.enabled {
            return;
        }
        self.tick = self.tick.wrapping_add(1);
        if !self.tick.is_multiple_of(self.every) {
            return;
        }
        let total_ns: u64 = self.phase_ns.iter().copied().sum();
        let pct = |phase: LoopPhase| -> u64 {
            if total_ns == 0 {
                0
            } else {
                (self.phase_ns[phase.idx()] * 100) / total_ns
            }
        };
        eprintln!(
            "{} shard={} mode={} sqpoll={} pending_cmds={} ready={} write_ready={} queued_sqes={} latency_inflight={} main_inflight={} progress_loops={} no_progress_loops={} stall_idle={} stall_wait_main={} stall_wait_latency={} stall_spin={} pct(drain/cqe_l/cqe_m/reduce/ready/submit/wm/wl/spin)={}/{}/{}/{}/{}/{}/{}/{}/{}",
            PHASE_LABEL,
            shard,
            mode,
            sqpoll,
            pending_cmds,
            ready_depth,
            write_ready_depth,
            queued_sqes,
            latency_inflight,
            main_inflight,
            self.progress_loops,
            self.no_progress_loops,
            self.stall_idle_no_inflight,
            self.stall_wait_main,
            self.stall_wait_latency,
            self.stall_spin,
            pct(LoopPhase::DrainCommands),
            pct(LoopPhase::CqeLatency),
            pct(LoopPhase::CqeMain),
            pct(LoopPhase::ReduceAge),
            pct(LoopPhase::DrainReady),
            pct(LoopPhase::SubmitQueued),
            pct(LoopPhase::WaitMain),
            pct(LoopPhase::WaitLatency),
            pct(LoopPhase::Spin)
        );
        self.phase_hits = [0; LOOP_PHASE_COUNT];
        self.phase_ns = [0; LOOP_PHASE_COUNT];
        self.progress_loops = 0;
        self.no_progress_loops = 0;
        self.stall_idle_no_inflight = 0;
        self.stall_wait_main = 0;
        self.stall_wait_latency = 0;
        self.stall_spin = 0;
    }
}
