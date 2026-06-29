use std::cmp::Reverse;

use quanta::Clock;

#[derive(Clone, Copy, Default)]
struct StepAgg {
    count: u64,
    total_ns: u64,
    max_ns: u64,
}

pub struct StepStats {
    enabled: bool,
    clock: Clock,
    rows: Vec<(&'static str, StepAgg)>,
}

impl StepStats {
    pub fn from_env(flag: &str) -> Self {
        let enabled = std::env::var(flag)
            .ok()
            .map(|v| {
                matches!(
                    v.as_str(),
                    "1" | "true" | "TRUE" | "yes" | "YES" | "on" | "ON"
                )
            })
            .unwrap_or(false);
        Self {
            enabled,
            clock: Clock::new(),
            rows: Vec::new(),
        }
    }

    #[inline]
    pub fn begin(&self) -> u64 {
        if self.enabled {
            self.clock.raw()
        } else {
            0
        }
    }

    #[inline]
    pub fn end(&mut self, step: &'static str, start_raw: u64) {
        if !self.enabled {
            return;
        }
        let elapsed = self.clock.delta_as_nanos(start_raw, self.clock.raw());
        let agg = self
            .rows
            .iter_mut()
            .find_map(|(name, agg)| (*name == step).then_some(agg));
        if let Some(agg) = agg {
            agg.count = agg.count.saturating_add(1);
            agg.total_ns = agg.total_ns.saturating_add(elapsed);
            agg.max_ns = agg.max_ns.max(elapsed);
            return;
        }
        self.rows.push((
            step,
            StepAgg {
                count: 1,
                total_ns: elapsed,
                max_ns: elapsed,
            },
        ));
    }

    pub fn print(&self, bench: &str) {
        if !self.enabled {
            return;
        }
        let total_ns: u64 = self.rows.iter().map(|(_, agg)| agg.total_ns).sum();
        println!(
            "step_stats bench={} enabled=1 steps={} total_ms={:.3}",
            bench,
            self.rows.len(),
            total_ns as f64 / 1_000_000.0
        );
        let mut rows = self.rows.clone();
        rows.sort_by_key(|row| Reverse(row.1.total_ns));
        for (step, agg) in rows {
            let pct = if total_ns == 0 {
                0.0
            } else {
                (agg.total_ns as f64 * 100.0) / total_ns as f64
            };
            let avg_ns = agg.total_ns.checked_div(agg.count).unwrap_or(0);
            println!(
                "step_stats bench={} step={} count={} total_ms={:.3} avg_ns={} max_ns={} pct={:.2}",
                bench,
                step,
                agg.count,
                agg.total_ns as f64 / 1_000_000.0,
                avg_ns,
                agg.max_ns,
                pct
            );
        }
    }
}
