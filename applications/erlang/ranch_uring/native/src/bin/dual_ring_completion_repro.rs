use std::env;
use std::process::ExitCode;
use std::time::Instant;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum RecvRingMode {
    Main,
    Latency,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum WaitPolicy {
    BiasedMainFirst,
    FairAlternating,
}

#[derive(Debug, Clone, Copy)]
struct ReproConfig {
    iters: usize,
    ops_per_iter: usize,
    max_ticks: usize,
    main_complete_tick: usize,
    mode: RecvRingMode,
    policy: WaitPolicy,
}

#[derive(Debug, Default, Clone, Copy)]
struct ReproOutcome {
    pass_iters: usize,
    fail_iters: usize,
    expected_total: usize,
    completed_total: usize,
    max_completed: usize,
    main_chosen_while_latency_pending_total: usize,
    main_wait_ticks_total: usize,
    latency_wait_ticks_total: usize,
    latency_progress_while_main_pending_total: usize,
    max_main_wait_streak_while_latency_pending: usize,
    first_latency_tick_total: usize,
    first_latency_while_main_pending_iters: usize,
}

#[derive(Debug, Default, Clone, Copy)]
struct IterOutcome {
    completed_latency: usize,
    expected_latency: usize,
    main_chosen_while_latency_pending: usize,
    main_wait_ticks: usize,
    latency_wait_ticks: usize,
    latency_progress_while_main_pending: usize,
    max_main_wait_streak_while_latency_pending: usize,
    first_latency_tick: usize,
    first_latency_while_main_pending: bool,
}

fn env_usize(name: &str, default: usize) -> usize {
    env::var(name)
        .ok()
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(default)
}

fn env_mode() -> RecvRingMode {
    match env::var("DUAL_RING_MODE").ok().as_deref() {
        Some("latency") | Some("LATENCY") => RecvRingMode::Latency,
        _ => RecvRingMode::Main,
    }
}

fn env_policy() -> WaitPolicy {
    match env::var("DUAL_RING_POLICY").ok().as_deref() {
        Some("fair") | Some("FAIR") => WaitPolicy::FairAlternating,
        _ => WaitPolicy::BiasedMainFirst,
    }
}

fn run_one(cfg: ReproConfig) -> IterOutcome {
    let mut completed_latency = 0usize;
    let expected_latency = cfg.ops_per_iter;
    let mut main_chosen_while_latency_pending = 0usize;
    let mut main_wait_ticks = 0usize;
    let mut latency_wait_ticks = 0usize;
    let mut latency_progress_while_main_pending = 0usize;
    let mut current_main_wait_streak_while_latency_pending = 0usize;
    let mut max_main_wait_streak_while_latency_pending = 0usize;
    let mut first_latency_tick = cfg.max_ticks;
    let mut first_latency_while_main_pending = false;

    let mut main_pending = true;
    let mut prefer_main_turn = true;

    for tick in 0..cfg.max_ticks {
        let has_latency_io = completed_latency < expected_latency;
        let prefer_main_wait = main_pending && cfg.mode != RecvRingMode::Latency;

        let choose_main = match cfg.policy {
            WaitPolicy::BiasedMainFirst => prefer_main_wait,
            WaitPolicy::FairAlternating => {
                if !main_pending {
                    false
                } else if !has_latency_io {
                    true
                } else {
                    let pick_main = prefer_main_turn;
                    prefer_main_turn = !prefer_main_turn;
                    pick_main
                }
            }
        };

        if choose_main {
            main_wait_ticks += 1;
            if has_latency_io {
                main_chosen_while_latency_pending += 1;
                current_main_wait_streak_while_latency_pending += 1;
                max_main_wait_streak_while_latency_pending =
                    max_main_wait_streak_while_latency_pending
                        .max(current_main_wait_streak_while_latency_pending);
            } else {
                current_main_wait_streak_while_latency_pending = 0;
            }
            if tick >= cfg.main_complete_tick {
                main_pending = false;
            }
            continue;
        }

        current_main_wait_streak_while_latency_pending = 0;
        if has_latency_io {
            latency_wait_ticks += 1;
            if !first_latency_while_main_pending && main_pending {
                first_latency_while_main_pending = true;
                first_latency_tick = tick;
            }
            if main_pending {
                latency_progress_while_main_pending += 1;
            }
            completed_latency += 1;
        }
    }

    debug_assert!(completed_latency <= expected_latency);
    debug_assert!(main_wait_ticks + latency_wait_ticks <= cfg.max_ticks);

    IterOutcome {
        completed_latency,
        expected_latency,
        main_chosen_while_latency_pending,
        main_wait_ticks,
        latency_wait_ticks,
        latency_progress_while_main_pending,
        max_main_wait_streak_while_latency_pending,
        first_latency_tick,
        first_latency_while_main_pending,
    }
}

fn run(cfg: ReproConfig) -> ReproOutcome {
    let mut out = ReproOutcome::default();
    for _ in 0..cfg.iters {
        let iter = run_one(cfg);
        out.expected_total = out.expected_total.saturating_add(iter.expected_latency);
        out.completed_total = out.completed_total.saturating_add(iter.completed_latency);
        out.max_completed = out.max_completed.max(iter.completed_latency);
        out.main_chosen_while_latency_pending_total = out
            .main_chosen_while_latency_pending_total
            .saturating_add(iter.main_chosen_while_latency_pending);
        out.main_wait_ticks_total = out
            .main_wait_ticks_total
            .saturating_add(iter.main_wait_ticks);
        out.latency_wait_ticks_total = out
            .latency_wait_ticks_total
            .saturating_add(iter.latency_wait_ticks);
        out.latency_progress_while_main_pending_total = out
            .latency_progress_while_main_pending_total
            .saturating_add(iter.latency_progress_while_main_pending);
        out.max_main_wait_streak_while_latency_pending = out
            .max_main_wait_streak_while_latency_pending
            .max(iter.max_main_wait_streak_while_latency_pending);
        if iter.first_latency_while_main_pending {
            out.first_latency_while_main_pending_iters =
                out.first_latency_while_main_pending_iters.saturating_add(1);
            out.first_latency_tick_total = out
                .first_latency_tick_total
                .saturating_add(iter.first_latency_tick);
        }
        if iter.completed_latency == iter.expected_latency {
            out.pass_iters = out.pass_iters.saturating_add(1);
        } else {
            out.fail_iters = out.fail_iters.saturating_add(1);
        }
    }
    out
}

fn main() -> ExitCode {
    let cfg = ReproConfig {
        iters: env_usize("DUAL_RING_ITERS", 1000),
        ops_per_iter: env_usize("DUAL_RING_OPS", 1000),
        max_ticks: env_usize("DUAL_RING_MAX_TICKS", 2000),
        main_complete_tick: env_usize("DUAL_RING_MAIN_COMPLETE_TICK", usize::MAX),
        mode: env_mode(),
        policy: env_policy(),
    };
    let gate_mode = env::var("DUAL_RING_GATE_MODE")
        .ok()
        .as_deref()
        .map(|v| v == "1")
        .unwrap_or(true);

    let started = Instant::now();
    let out = run(cfg);
    let elapsed = started.elapsed();

    println!(
        "dual_ring_completion_repro: policy={:?} mode={:?} iters={} ops={} ticks={} main_complete_tick={}",
        cfg.policy,
        cfg.mode,
        cfg.iters,
        cfg.ops_per_iter,
        cfg.max_ticks,
        cfg.main_complete_tick
    );
    println!(
        "iterations: pass={} fail={} | completions: expected={} completed={} max_completed_per_iter={} | fairness: main_chosen_while_latency_pending_total={} main_wait_ticks_total={} latency_wait_ticks_total={} latency_progress_while_main_pending_total={} max_main_wait_streak_while_latency_pending={} first_latency_while_main_pending_iters={} avg_first_latency_tick={:.2} | elapsed_ms={}",
        out.pass_iters,
        out.fail_iters,
        out.expected_total,
        out.completed_total,
        out.max_completed,
        out.main_chosen_while_latency_pending_total,
        out.main_wait_ticks_total,
        out.latency_wait_ticks_total,
        out.latency_progress_while_main_pending_total,
        out.max_main_wait_streak_while_latency_pending,
        out.first_latency_while_main_pending_iters,
        if out.first_latency_while_main_pending_iters == 0 {
            cfg.max_ticks as f64
        } else {
            out.first_latency_tick_total as f64 / out.first_latency_while_main_pending_iters as f64
        },
        elapsed.as_millis()
    );

    if gate_mode {
        let bad_cfg = ReproConfig {
            policy: WaitPolicy::BiasedMainFirst,
            mode: RecvRingMode::Main,
            ..cfg
        };
        let fair_cfg = ReproConfig {
            policy: WaitPolicy::FairAlternating,
            mode: RecvRingMode::Main,
            ..cfg
        };
        let bad = run(bad_cfg);
        let fair = run(fair_cfg);
        let bad_avg_first_latency_tick = if bad.first_latency_while_main_pending_iters == 0 {
            cfg.max_ticks as f64
        } else {
            bad.first_latency_tick_total as f64 / bad.first_latency_while_main_pending_iters as f64
        };
        let fair_avg_first_latency_tick = if fair.first_latency_while_main_pending_iters == 0 {
            cfg.max_ticks as f64
        } else {
            fair.first_latency_tick_total as f64
                / fair.first_latency_while_main_pending_iters as f64
        };
        println!(
            "gate: biased_fail_iters={} fair_fail_iters={} biased_main_chosen_while_latency_pending_total={} fair_main_chosen_while_latency_pending_total={} biased_max_main_wait_streak_while_latency_pending={} fair_max_main_wait_streak_while_latency_pending={} biased_avg_first_latency_tick={:.2} fair_avg_first_latency_tick={:.2} fair_first_latency_while_main_pending_iters={}",
            bad.fail_iters,
            fair.fail_iters,
            bad.main_chosen_while_latency_pending_total,
            fair.main_chosen_while_latency_pending_total,
            bad.max_main_wait_streak_while_latency_pending,
            fair.max_main_wait_streak_while_latency_pending,
            bad_avg_first_latency_tick,
            fair_avg_first_latency_tick,
            fair.first_latency_while_main_pending_iters
        );
        if bad.fail_iters == 0
            || fair.fail_iters != 0
            || bad.main_chosen_while_latency_pending_total
                <= fair.main_chosen_while_latency_pending_total
            || bad.max_main_wait_streak_while_latency_pending
                <= fair.max_main_wait_streak_while_latency_pending
            || bad_avg_first_latency_tick <= fair_avg_first_latency_tick
            || fair.first_latency_while_main_pending_iters == 0
        {
            eprintln!("gate failed: reproducer did not show expected biased-vs-fair contrast");
            return ExitCode::from(1);
        }
        return ExitCode::SUCCESS;
    }

    if out.fail_iters > 0 {
        eprintln!(
            "reproducer detected partial completion envelope (completed < expected in {} iterations)",
            out.fail_iters
        );
        return ExitCode::from(1);
    }

    ExitCode::SUCCESS
}
