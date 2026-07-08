pub(crate) const RECOVERY_MAGIC: u32 = 0x4450_4849;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RecoverySnapshot {
    pub watchdog_reset: bool,
    pub magic: u32,
    pub watchdog_resets: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) struct RecoveryPlan {
    pub decision: RecoveryDecision,
    pub next_magic: u32,
    pub next_watchdog_resets: u32,
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub(crate) enum RecoveryDecision {
    Continue,
    EnterBootsel,
}

pub(crate) fn plan_recovery(
    snapshot: RecoverySnapshot,
    bootsel_after_watchdog_resets: u32,
) -> RecoveryPlan {
    let previous_resets = if snapshot.magic == RECOVERY_MAGIC {
        snapshot.watchdog_resets
    } else {
        0
    };
    let next_watchdog_resets = if snapshot.watchdog_reset {
        previous_resets.saturating_add(1)
    } else {
        0
    };
    let decision = if bootsel_after_watchdog_resets != 0
        && next_watchdog_resets >= bootsel_after_watchdog_resets
    {
        RecoveryDecision::EnterBootsel
    } else {
        RecoveryDecision::Continue
    };

    RecoveryPlan {
        decision,
        next_magic: RECOVERY_MAGIC,
        next_watchdog_resets,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn snapshot(watchdog_reset: bool, magic_valid: bool, watchdog_resets: u32) -> RecoverySnapshot {
        RecoverySnapshot {
            watchdog_reset,
            magic: if magic_valid { RECOVERY_MAGIC } else { 0 },
            watchdog_resets,
        }
    }

    #[test]
    fn clean_boot_records_magic_and_clears_resets() {
        let plan = plan_recovery(snapshot(false, false, 99), 3);

        assert_eq!(plan.decision, RecoveryDecision::Continue);
        assert_eq!(plan.next_magic, RECOVERY_MAGIC);
        assert_eq!(plan.next_watchdog_resets, 0);
    }

    #[test]
    fn watchdog_boot_counts_consecutive_unstable_resets() {
        let first = plan_recovery(snapshot(true, true, 0), 3);
        let second = plan_recovery(snapshot(true, true, first.next_watchdog_resets), 3);

        assert_eq!(first.decision, RecoveryDecision::Continue);
        assert_eq!(first.next_watchdog_resets, 1);
        assert_eq!(second.decision, RecoveryDecision::Continue);
        assert_eq!(second.next_watchdog_resets, 2);
    }

    #[test]
    fn threshold_watchdog_boot_enters_bootsel() {
        let plan = plan_recovery(snapshot(true, true, 2), 3);

        assert_eq!(plan.decision, RecoveryDecision::EnterBootsel);
        assert_eq!(plan.next_watchdog_resets, 3);
    }

    #[test]
    fn zero_threshold_disables_bootsel_escalation() {
        let plan = plan_recovery(snapshot(true, true, u32::MAX), 0);

        assert_eq!(plan.decision, RecoveryDecision::Continue);
        assert_eq!(plan.next_watchdog_resets, u32::MAX);
    }

    #[test]
    fn non_watchdog_reset_clears_stale_counter() {
        let plan = plan_recovery(snapshot(false, true, 2), 3);

        assert_eq!(plan.decision, RecoveryDecision::Continue);
        assert_eq!(plan.next_watchdog_resets, 0);
    }

    #[test]
    fn invalid_magic_starts_new_recovery_sequence() {
        let plan = plan_recovery(snapshot(true, false, 99), 3);

        assert_eq!(plan.decision, RecoveryDecision::Continue);
        assert_eq!(plan.next_watchdog_resets, 1);
    }

    #[test]
    fn small_state_space_obeys_escalation_threshold() {
        for threshold in 0..5 {
            for watchdog_resets in 0..5 {
                for watchdog_reset in [false, true] {
                    for magic_valid in [false, true] {
                        let plan = plan_recovery(
                            snapshot(watchdog_reset, magic_valid, watchdog_resets),
                            threshold,
                        );

                        assert_eq!(plan.next_magic, RECOVERY_MAGIC);
                        if threshold == 0 || plan.next_watchdog_resets < threshold {
                            assert_eq!(plan.decision, RecoveryDecision::Continue);
                        } else {
                            assert_eq!(plan.decision, RecoveryDecision::EnterBootsel);
                        }
                    }
                }
            }
        }
    }
}
