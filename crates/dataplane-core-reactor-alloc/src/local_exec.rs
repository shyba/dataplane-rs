use alloc::collections::VecDeque;
use alloc::vec;
use alloc::vec::Vec;

/// Error returned by fallible pushes, mirroring the no_std
/// `FixedLocalExec` contract (the item is handed back).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PushError<K> {
    /// `slot` is outside the configured slot count.
    BadSlot(K),
}

pub struct LocalExec<K> {
    queues: Vec<VecDeque<K>>,
    runnable: VecDeque<usize>,
    enqueued: Vec<bool>,
    pending: usize,
}

impl<K> LocalExec<K> {
    pub fn new(slot_count: usize) -> Self {
        Self {
            queues: (0..slot_count).map(|_| VecDeque::new()).collect(),
            runnable: VecDeque::with_capacity(slot_count),
            enqueued: vec![false; slot_count],
            pending: 0,
        }
    }

    pub fn push(&mut self, slot: usize, item: K) -> Result<(), PushError<K>> {
        let Some(queue) = self.queues.get_mut(slot) else {
            return Err(PushError::BadSlot(item));
        };
        let was_empty = queue.is_empty();
        queue.push_back(item);
        self.pending += 1;
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

    pub fn drain<F>(
        &mut self,
        runnable_budget: usize,
        session_run_budget: usize,
        drain_session: bool,
        mut f: F,
    ) -> usize
    where
        F: FnMut(K),
    {
        let mut progressed = 0usize;
        for _ in 0..runnable_budget {
            let Some(slot) = self.runnable.pop_front() else {
                break;
            };
            self.enqueued[slot] = false;
            let session_queue = &mut self.queues[slot];
            let mut ran = 0usize;
            while drain_session || ran < session_run_budget {
                let Some(op) = session_queue.pop_front() else {
                    break;
                };
                f(op);
                self.pending -= 1;
                progressed += 1;
                ran += 1;
            }
            if !session_queue.is_empty() && !self.enqueued[slot] {
                self.enqueued[slot] = true;
                self.runnable.push_back(slot);
            }
        }
        progressed
    }
}

#[cfg(test)]
mod tests {
    use super::LocalExec;
    use alloc::vec;
    use alloc::vec::Vec;
    use proptest::prelude::*;

    #[derive(Clone, Copy, Debug)]
    enum Op {
        Push {
            slot: usize,
            value: usize,
        },
        Drain {
            runnable_budget: usize,
            session_run_budget: usize,
            drain_session: bool,
        },
    }

    fn pending_sum<K>(exec: &LocalExec<K>) -> usize {
        exec.queues.iter().map(|queue| queue.len()).sum()
    }

    proptest! {
        #[test]
        fn pending_matches_total_queue_lengths_after_operation_sequences(
            slot_count in 1usize..8,
            ops in proptest::collection::vec(
                prop_oneof![
                    (0usize..8, 0usize..64).prop_map(|(slot, value)| Op::Push { slot, value }),
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
            let mut exec = LocalExec::new(slot_count);

            for op in ops {
                match op {
                    Op::Push { slot, value } => exec.push(slot % slot_count, value).expect("valid slot"),
                    Op::Drain { runnable_budget, session_run_budget, drain_session } => {
                        let before_pending = exec.pending();
                        let mut drained = Vec::new();
                        let progressed = exec.drain(
                            runnable_budget,
                            session_run_budget,
                            drain_session,
                            |value| drained.push(value),
                        );
                        prop_assert_eq!(progressed, drained.len());
                        prop_assert_eq!(before_pending - exec.pending(), progressed);
                    }
                }

                prop_assert_eq!(exec.pending(), pending_sum(&exec));
                prop_assert_eq!(exec.has_work(), exec.pending() != 0);
                for slot in 0..slot_count {
                    prop_assert_eq!(exec.enqueued[slot], exec.runnable.iter().any(|&queued| queued == slot));
                }
            }
        }
    }

    #[test]
    fn pending_tracks_total_work_and_fifo_order() {
        let mut exec = LocalExec::new(2);
        exec.push(0, 10).expect("push");
        exec.push(0, 11).expect("push");
        exec.push(1, 20).expect("push");
        exec.push(0, 12).expect("push");

        let mut drained = Vec::new();
        let progressed = exec.drain(usize::MAX, 1, false, |value| drained.push(value));

        assert_eq!(progressed, 4);
        assert_eq!(drained, vec![10, 20, 11, 12]);
        assert_eq!(exec.pending(), 0);
        assert!(!exec.has_work());
    }

    #[test]
    fn slot_is_enqueued_only_once_while_non_empty() {
        let mut exec = LocalExec::new(2);

        exec.push(1, 10).expect("push");
        exec.push(1, 11).expect("push");
        exec.push(1, 12).expect("push");

        assert_eq!(exec.pending(), 3);
        assert_eq!(exec.runnable.iter().copied().collect::<Vec<_>>(), vec![1]);
        assert!(exec.enqueued[1]);
    }

    #[test]
    fn partial_drain_requeues_slot_exactly_once() {
        let mut exec = LocalExec::new(1);
        exec.push(0, 10).expect("push");
        exec.push(0, 11).expect("push");
        exec.push(0, 12).expect("push");

        let mut drained = Vec::new();
        let progressed = exec.drain(1, 1, false, |value| drained.push(value));

        assert_eq!(progressed, 1);
        assert_eq!(drained, vec![10]);
        assert_eq!(exec.pending(), 2);
        assert_eq!(exec.runnable.iter().copied().collect::<Vec<_>>(), vec![0]);
        assert!(exec.enqueued[0]);
    }

    #[test]
    fn drain_session_true_drains_current_slot_fully() {
        let mut exec = LocalExec::new(2);
        exec.push(0, 10).expect("push");
        exec.push(0, 11).expect("push");
        exec.push(0, 12).expect("push");
        exec.push(1, 20).expect("push");
        exec.push(1, 21).expect("push");

        let mut drained = Vec::new();
        let progressed = exec.drain(1, 1, true, |value| drained.push(value));

        assert_eq!(progressed, 3);
        assert_eq!(drained, vec![10, 11, 12]);
        assert_eq!(exec.pending(), 2);
        assert_eq!(exec.runnable.iter().copied().collect::<Vec<_>>(), vec![1]);
        assert_eq!(exec.enqueued, vec![false, true]);
    }

    #[test]
    fn drain_session_false_respects_session_budget() {
        let mut exec = LocalExec::new(2);
        exec.push(0, 10).expect("push");
        exec.push(0, 11).expect("push");
        exec.push(0, 12).expect("push");
        exec.push(1, 20).expect("push");
        exec.push(1, 21).expect("push");

        let mut drained = Vec::new();
        let progressed = exec.drain(1, 1, false, |value| drained.push(value));

        assert_eq!(progressed, 1);
        assert_eq!(drained, vec![10]);
        assert_eq!(exec.pending(), 4);
        assert_eq!(
            exec.runnable.iter().copied().collect::<Vec<_>>(),
            vec![1, 0]
        );
        assert_eq!(exec.enqueued, vec![true, true]);
    }

    #[test]
    fn zero_runnable_budget_preserves_state() {
        let mut exec = LocalExec::new(1);
        exec.push(0, 10).expect("push");
        exec.push(0, 11).expect("push");

        let mut drained = Vec::new();
        let progressed = exec.drain(0, 0, false, |value| drained.push(value));

        assert_eq!(progressed, 0);
        assert!(drained.is_empty());
        assert_eq!(exec.pending(), 2);
        assert_eq!(exec.runnable.iter().copied().collect::<Vec<_>>(), vec![0]);
        assert!(exec.enqueued[0]);
    }

    #[test]
    fn zero_session_budget_requeues_without_progress() {
        let mut exec = LocalExec::new(1);
        exec.push(0, 10).expect("push");
        exec.push(0, 11).expect("push");

        let mut drained = Vec::new();
        let progressed = exec.drain(1, 0, false, |value| drained.push(value));

        assert_eq!(progressed, 0);
        assert!(drained.is_empty());
        assert_eq!(exec.pending(), 2);
        assert_eq!(exec.runnable.iter().copied().collect::<Vec<_>>(), vec![0]);
        assert!(exec.enqueued[0]);
    }
}
