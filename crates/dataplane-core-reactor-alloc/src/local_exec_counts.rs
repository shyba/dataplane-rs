use alloc::collections::VecDeque;
use alloc::vec;
use alloc::vec::Vec;

/// Errors from fallible count pushes, mirroring the no_std
/// `FixedLocalExecCounts` contract.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PushCountError {
    /// `slot` is outside the configured slot count.
    BadSlot,
    /// The per-slot or total pending count would overflow.
    Overflow,
}

pub struct LocalExecCounts {
    pending_per_slot: Vec<usize>,
    runnable: VecDeque<usize>,
    enqueued: Vec<bool>,
    pending: usize,
}

impl LocalExecCounts {
    pub fn new(slot_count: usize) -> Self {
        Self {
            pending_per_slot: vec![0; slot_count],
            runnable: VecDeque::with_capacity(slot_count),
            enqueued: vec![false; slot_count],
            pending: 0,
        }
    }

    pub fn push_count(&mut self, slot: usize, count: usize) -> Result<(), PushCountError> {
        if count == 0 {
            return Ok(());
        }
        let Some(per_slot) = self.pending_per_slot.get_mut(slot) else {
            return Err(PushCountError::BadSlot);
        };
        let was_empty = *per_slot == 0;
        let next_slot = per_slot
            .checked_add(count)
            .ok_or(PushCountError::Overflow)?;
        let next_pending = self
            .pending
            .checked_add(count)
            .ok_or(PushCountError::Overflow)?;
        // Commit only after both checks succeed: errors must preserve all state.
        *per_slot = next_slot;
        self.pending = next_pending;
        if was_empty && !self.enqueued[slot] {
            self.enqueued[slot] = true;
            self.runnable.push_back(slot);
        }
        Ok(())
    }

    pub fn has_work(&self) -> bool {
        self.pending != 0
    }

    pub fn pending(&self) -> usize {
        self.pending
    }

    /// Visit at most `runnable_budget` slots, rotating unfinished slots to the back.
    /// A slot may be revisited in the same call. `drain_session` ignores the
    /// per-visit item budget and drains each selected slot completely.
    ///
    /// Callbacks must not unwind: after a callback panic, discard this executor.
    /// Pending work in the current slot may no longer be scheduled.
    pub fn drain<F>(
        &mut self,
        runnable_budget: usize,
        session_run_budget: usize,
        drain_session: bool,
        mut f: F,
    ) -> usize
    where
        F: FnMut(),
    {
        let mut progressed = 0usize;
        for _ in 0..runnable_budget {
            let Some(slot) = self.runnable.pop_front() else {
                break;
            };
            self.enqueued[slot] = false;
            let mut ran = 0usize;
            while drain_session || ran < session_run_budget {
                if self.pending_per_slot[slot] == 0 {
                    break;
                }
                self.pending_per_slot[slot] -= 1;
                self.pending -= 1;
                f();
                progressed += 1;
                ran += 1;
            }
            if self.pending_per_slot[slot] != 0 && !self.enqueued[slot] {
                self.enqueued[slot] = true;
                self.runnable.push_back(slot);
            }
        }
        progressed
    }
}

#[cfg(test)]
mod tests {
    use super::LocalExecCounts;
    use alloc::vec;
    use alloc::vec::Vec;
    use proptest::prelude::*;

    #[derive(Clone, Copy, Debug)]
    enum Op {
        Push {
            slot: usize,
            count: usize,
        },
        Drain {
            runnable_budget: usize,
            session_run_budget: usize,
            drain_session: bool,
        },
    }

    fn pending_sum(exec: &LocalExecCounts) -> usize {
        exec.pending_per_slot.iter().sum()
    }

    proptest! {
        #[test]
        fn pending_matches_slot_sum_after_operation_sequences(
            slot_count in 1usize..8,
            ops in proptest::collection::vec(
                prop_oneof![
                    (0usize..8, 0usize..8).prop_map(|(slot, count)| Op::Push { slot, count }),
                    (0usize..8, 0usize..8, any::<bool>()).prop_map(
                        |(runnable_budget, session_run_budget, drain_session)| Op::Drain {
                            runnable_budget,
                            session_run_budget,
                            drain_session,
                        }
                    ),
                ],
                0..128
            )
        ) {
            let mut exec = LocalExecCounts::new(slot_count);

            for op in ops {
                match op {
                    Op::Push { slot, count } => exec.push_count(slot % slot_count, count).expect("push_count"),
                    Op::Drain { runnable_budget, session_run_budget, drain_session } => {
                        let before_pending = exec.pending;
                        let mut callback_count = 0usize;
                        let progressed = exec.drain(
                            runnable_budget,
                            session_run_budget,
                            drain_session,
                            || callback_count += 1,
                        );
                        prop_assert_eq!(progressed, callback_count);
                        prop_assert_eq!(before_pending - exec.pending, progressed);
                    }
                }

                prop_assert_eq!(exec.pending, pending_sum(&exec));
                prop_assert_eq!(exec.has_work(), exec.pending != 0);
                for slot in 0..slot_count {
                    prop_assert_eq!(exec.enqueued[slot], exec.runnable.iter().any(|&queued| queued == slot));
                }
            }
        }
    }

    #[test]
    fn overflow_preserves_all_state() {
        for slot in [0, 1] {
            let mut exec = LocalExecCounts::new(2);
            exec.push_count(0, usize::MAX).unwrap();
            let before = (
                exec.pending_per_slot.clone(),
                exec.runnable.clone(),
                exec.enqueued.clone(),
                exec.pending,
            );
            assert_eq!(
                exec.push_count(slot, 1),
                Err(super::PushCountError::Overflow)
            );
            assert_eq!(
                (
                    exec.pending_per_slot,
                    exec.runnable,
                    exec.enqueued,
                    exec.pending
                ),
                before
            );
        }
    }

    #[test]
    fn invalid_slot_preserves_state_and_zero_remains_a_noop() {
        let mut exec = LocalExecCounts::new(0);
        assert_eq!(exec.push_count(0, 1), Err(super::PushCountError::BadSlot));
        assert_eq!(exec.push_count(usize::MAX, 0), Ok(()));
        assert_eq!(exec.pending(), 0);
        assert!(exec.runnable.is_empty());
    }

    #[test]
    fn zero_count_is_a_noop() {
        let mut exec = LocalExecCounts::new(2);
        exec.push_count(0, 0).expect("push_count");
        exec.push_count(1, 0).expect("push_count");

        assert!(!exec.has_work());
        assert_eq!(exec.drain(4, 4, false, || panic!("should not run")), 0);
    }

    #[test]
    fn pending_count_tracks_total_work_and_preserves_slot_fairness() {
        let mut exec = LocalExecCounts::new(2);
        exec.push_count(0, 2).expect("push_count");
        exec.push_count(1, 1).expect("push_count");
        exec.push_count(0, 1).expect("push_count");

        assert!(exec.has_work());

        let mut seen = Vec::new();
        let progressed = exec.drain(usize::MAX, 1, false, || seen.push(()));

        assert_eq!(progressed, 4);
        assert_eq!(seen.len(), 4);
        assert!(!exec.has_work());

        let progressed = exec.drain(usize::MAX, 1, false, || seen.push(()));
        assert_eq!(progressed, 0);
        assert_eq!(seen.len(), 4);
        assert!(!exec.has_work());
    }

    #[test]
    fn push_count_zero_is_noop() {
        let mut exec = LocalExecCounts::new(3);

        exec.push_count(1, 0).expect("push_count");

        assert_eq!(exec.pending, 0);
        assert_eq!(exec.pending_per_slot, vec![0, 0, 0]);
        assert!(exec.runnable.is_empty());
        assert_eq!(exec.enqueued, vec![false, false, false]);
    }

    #[test]
    fn slot_is_enqueued_only_once_while_non_empty() {
        let mut exec = LocalExecCounts::new(2);

        exec.push_count(1, 2).expect("push_count");
        exec.push_count(1, 3).expect("push_count");

        assert_eq!(exec.pending, 5);
        assert_eq!(exec.pending_per_slot[1], 5);
        assert_eq!(exec.runnable.iter().copied().collect::<Vec<_>>(), vec![1]);
        assert!(exec.enqueued[1]);
    }

    #[test]
    fn drain_session_consumes_entire_slot_when_requested() {
        let mut exec = LocalExecCounts::new(1);
        exec.push_count(0, 3).expect("push_count");

        let mut ran = 0usize;
        let progressed = exec.drain(1, 1, true, || ran += 1);

        assert_eq!(progressed, 3);
        assert_eq!(ran, 3);
        assert!(!exec.has_work());
    }

    #[test]
    fn partial_drain_requeues_remaining_work_once() {
        let mut exec = LocalExecCounts::new(1);
        exec.push_count(0, 3).expect("push_count");

        let mut ran = 0usize;
        let progressed = exec.drain(1, 2, false, || ran += 1);
        assert_eq!(progressed, 2);
        assert_eq!(ran, 2);
        assert!(exec.has_work());

        let progressed = exec.drain(1, 2, false, || ran += 1);
        assert_eq!(progressed, 1);
        assert_eq!(ran, 3);
        assert!(!exec.has_work());
    }

    #[test]
    fn zero_runnable_budget_preserves_state() {
        let mut exec = LocalExecCounts::new(1);
        let mut calls = 0usize;
        exec.push_count(0, 2).expect("push_count");

        let progressed = exec.drain(0, 0, false, || calls += 1);

        assert_eq!(progressed, 0);
        assert_eq!(calls, 0);
        assert_eq!(exec.pending, 2);
        assert_eq!(exec.pending_per_slot[0], 2);
        assert_eq!(exec.runnable.iter().copied().collect::<Vec<_>>(), vec![0]);
        assert!(exec.enqueued[0]);
    }

    #[test]
    fn zero_session_budget_requeues_without_progress() {
        let mut exec = LocalExecCounts::new(1);
        let mut calls = 0usize;
        exec.push_count(0, 2).expect("push_count");

        let progressed = exec.drain(1, 0, false, || calls += 1);

        assert_eq!(progressed, 0);
        assert_eq!(calls, 0);
        assert_eq!(exec.pending, 2);
        assert_eq!(exec.pending_per_slot[0], 2);
        assert_eq!(exec.runnable.iter().copied().collect::<Vec<_>>(), vec![0]);
        assert!(exec.enqueued[0]);
    }
}
