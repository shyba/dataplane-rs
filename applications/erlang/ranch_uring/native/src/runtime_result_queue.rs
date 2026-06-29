//! Result queue management for runtime shard.
//!
//! Extracts ResultEvent, ResultReduceState, ResultBatchSlot, and ResultReduceTrigger
//! from runtime.rs to enable independent evolution of result batching logic.
//!
//! Also provides `ResultFacet`, a private view struct that aggregates result queue
//! state and exposes a stable read-only interface to result queue predicates.

use std::collections::VecDeque;

use crate::runtime_adapter::{AsyncReplyPayload, ResultTarget};

/// Private view struct for result queue state on ShardState.
///
/// Aggregates the result-related fields from ShardState and exposes result queue
/// predicates without exposing the full ShardState struct. This is the first
/// ResultFacet slice — a narrow, owned interface over result-related state.
///
/// Constructed via `ResultFacet::new(...)` at the call site that needs to inspect
/// result queue state.
///
/// NOTE: This type is private to the native runtime crate.
/// Do not widen its visibility beyond what current call sites require.
pub(super) struct ResultFacet {
    result_direct_send: bool,
    pending_len: usize,
    reducer_scheduled: bool,
    result_batch_nonempty_slots: usize,
}

impl ResultFacet {
    /// Constructs a new ResultFacet from the given result queue fields.
    pub(super) fn new(
        result_direct_send: bool,
        pending_len: usize,
        reducer_scheduled: bool,
        result_batch_nonempty_slots: usize,
    ) -> Self {
        Self {
            result_direct_send,
            pending_len,
            reducer_scheduled,
            result_batch_nonempty_slots,
        }
    }

    /// Returns true when the result send path is ready for a direct handoff,
    /// meaning no batching, no pending reduce, and no reducer task is scheduled.
    pub(super) fn direct_send_ready(&self) -> bool {
        result_direct_send_ready(
            self.result_direct_send,
            self.pending_len,
            self.reducer_scheduled,
            self.result_batch_nonempty_slots,
        )
    }

    /// Returns true if result reduction should be scheduled based on the given trigger.
    ///
    /// This wraps `reduce_scheduling_predicate` using fields from this ResultFacet
    /// for the first three arguments; the caller provides trigger, CQE count,
    /// first_result_ns, and the current timestamp.
    #[inline]
    pub(super) fn should_reduce(
        &self,
        trigger: ResultReduceTrigger,
        result_reduce_cqes: usize,
        first_result_ns: Option<u64>,
        now_ns: u64,
    ) -> bool {
        reduce_scheduling_predicate(
            trigger,
            self.pending_len,
            self.reducer_scheduled,
            result_reduce_cqes,
            first_result_ns,
            now_ns,
        )
    }

    /// Computes the reduce scheduling outcome for the given trigger and current state.
    ///
    /// Returns `ResultCallbackOutcome::ReduceResults` if reduction should be scheduled,
    /// or `ResultCallbackOutcome::None` if not. This is a query — it does not mutate
    /// any state. The caller is responsible for applying `reducer_scheduled = true`
    /// and enqueueing the callback when `ReduceResults` is returned.
    #[inline]
    pub(super) fn schedule_outcome(
        &self,
        trigger: ResultReduceTrigger,
        result_reduce_cqes: usize,
        first_result_ns: Option<u64>,
        now_ns: u64,
    ) -> ResultCallbackOutcome {
        if self.should_reduce(trigger, result_reduce_cqes, first_result_ns, now_ns) {
            ResultCallbackOutcome::ReduceResults
        } else {
            ResultCallbackOutcome::None
        }
    }
}

pub(super) const RESULT_REDUCE_CQE_TRIGGER: usize = 128;
pub(super) const RESULT_REDUCE_AGE_NS: u64 = 20_000;
pub(super) const RESULT_SEND_BATCH_LIMIT: usize = 64;

/// Returns true when the result send path is ready for a direct handoff,
/// meaning no batching, no pending reduce, and no reducer task is scheduled.
///
/// This is a pure predicate on 4 scalars - no mutable state required.
/// Canonical implementation for ShardState::result_direct_send_ready.
#[inline]
pub(super) fn result_direct_send_ready(
    result_direct_send: bool,
    pending_len: usize,
    reducer_scheduled: bool,
    result_batch_nonempty_slots: usize,
) -> bool {
    result_direct_send && pending_len == 0 && !reducer_scheduled && result_batch_nonempty_slots == 0
}

/// Trigger for scheduling result reduction.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ResultReduceTrigger {
    Cqe,
    Age,
    Idle,
}

/// Outcome of a result callback scheduling decision.
/// Represents what callback, if any, should be enqueued as a result of a scheduling decision.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum ResultCallbackOutcome {
    /// No callback was scheduled.
    None,
    /// A ReduceResults callback should be enqueued.
    ReduceResults,
    /// A SendResults callback for the given slot should be enqueued.
    SendResults(usize),
}

impl From<ResultCallbackOutcome> for bool {
    /// Returns true if any callback was scheduled (ReduceResults or SendResults).
    fn from(outcome: ResultCallbackOutcome) -> bool {
        !matches!(outcome, ResultCallbackOutcome::None)
    }
}

/// A single result event pending reduction.
pub(super) struct ResultEvent {
    pub(super) target: ResultTarget,
    pub(super) request_id: u64,
    pub(super) payload: AsyncReplyPayload,
    pub(super) enqueued_raw: u64,
}

/// State machine for result reduction scheduling.
#[derive(Default)]
pub(super) struct ResultReduceState {
    pub(super) pending: VecDeque<ResultEvent>,
    pub(super) first_result_ns: Option<u64>,
    pub(super) reducer_scheduled: bool,
}

/// A batch slot for results destined for a specific target.
pub(super) struct ResultBatchSlot {
    pub(super) target: ResultTarget,
    pub(super) entries: VecDeque<(u64, AsyncReplyPayload, u64)>,
    pub(super) send_queued: bool,
}

impl ResultBatchSlot {
    /// Creates a new batch slot for the given result target.
    pub(super) fn new(target: ResultTarget) -> Self {
        Self {
            target,
            entries: VecDeque::new(),
            send_queued: false,
        }
    }

    /// Returns true if this batch slot has entries ready to be sent and is not already queued.
    /// This is the pure queue predicate for result batch slot readiness.
    #[inline]
    pub(super) fn queue_ready(&self) -> bool {
        !self.send_queued && !self.entries.is_empty()
    }
}

/// Determines whether result reduction should be scheduled based on the given trigger and state.
/// Returns true if the reducer should be scheduled; caller must still perform the mutation
/// (setting reducer_scheduled and enqueueing the callback).
///
/// Pure predicate - no mutable state modified.
#[inline]
pub(super) fn reduce_scheduling_predicate(
    trigger: ResultReduceTrigger,
    pending_len: usize,
    reducer_scheduled: bool,
    result_reduce_cqes: usize,
    first_result_ns: Option<u64>,
    now_ns: u64,
) -> bool {
    // No pending results or reducer already scheduled - nothing to do
    if pending_len == 0 || reducer_scheduled {
        return false;
    }

    match trigger {
        ResultReduceTrigger::Cqe => result_reduce_cqes >= RESULT_REDUCE_CQE_TRIGGER,
        ResultReduceTrigger::Age => first_result_ns
            .map(|first| now_ns.saturating_sub(first) >= RESULT_REDUCE_AGE_NS)
            .unwrap_or(false),
        ResultReduceTrigger::Idle => true,
    }
}

#[cfg(test)]
mod tests {
    use super::{
        reduce_scheduling_predicate, result_direct_send_ready, ResultBatchSlot,
        ResultCallbackOutcome, ResultReduceState, ResultReduceTrigger,
    };
    use crate::runtime_adapter::{AsyncReplyPayload, ResultTarget};
    use std::sync::mpsc;

    #[test]
    fn result_reduce_state_default() {
        let state = ResultReduceState::default();
        assert!(state.pending.is_empty());
        assert!(state.first_result_ns.is_none());
        assert!(!state.reducer_scheduled);
    }

    #[test]
    fn result_reduce_trigger_variants() {
        let _ = ResultReduceTrigger::Cqe;
        let _ = ResultReduceTrigger::Age;
        let _ = ResultReduceTrigger::Idle;
        assert_eq!(ResultReduceTrigger::Cqe, ResultReduceTrigger::Cqe);
        assert_eq!(ResultReduceTrigger::Age, ResultReduceTrigger::Age);
        assert_eq!(ResultReduceTrigger::Idle, ResultReduceTrigger::Idle);
    }

    #[test]
    fn result_reduce_trigger_copy() {
        let trigger = ResultReduceTrigger::Cqe;
        let _ = trigger;
        let trigger2 = trigger;
        let _ = trigger2;
    }

    #[test]
    fn result_reduce_state_reducer_scheduled_default_false() {
        let state = ResultReduceState::default();
        assert!(!state.reducer_scheduled);
    }

    // === ResultCallbackOutcome tests (DP-RC-0011) ===

    #[test]
    fn result_callback_outcome_variants() {
        assert_eq!(ResultCallbackOutcome::None, ResultCallbackOutcome::None);
        assert_eq!(
            ResultCallbackOutcome::ReduceResults,
            ResultCallbackOutcome::ReduceResults
        );
        assert_eq!(
            ResultCallbackOutcome::SendResults(0),
            ResultCallbackOutcome::SendResults(0)
        );
        assert_eq!(
            ResultCallbackOutcome::SendResults(5),
            ResultCallbackOutcome::SendResults(5)
        );
        assert_ne!(
            ResultCallbackOutcome::SendResults(0),
            ResultCallbackOutcome::SendResults(5)
        );
    }

    #[test]
    fn result_callback_outcome_copy() {
        let outcome = ResultCallbackOutcome::ReduceResults;
        let _ = outcome;
        let outcome2 = outcome;
        let _ = outcome2;
        let outcome3 = ResultCallbackOutcome::SendResults(42);
        let _ = outcome3;
    }

    // === send-result slot outcome policy tests (DP-RC-0020) ===

    #[test]
    fn result_callback_outcome_send_results_slot_values() {
        // Verify SendResults carries the correct slot index via pattern match
        match ResultCallbackOutcome::SendResults(0) {
            ResultCallbackOutcome::SendResults(slot) => assert_eq!(slot, 0),
            _ => unreachable!(),
        }
        match ResultCallbackOutcome::SendResults(5) {
            ResultCallbackOutcome::SendResults(slot) => assert_eq!(slot, 5),
            _ => unreachable!(),
        }
        match ResultCallbackOutcome::SendResults(usize::MAX) {
            ResultCallbackOutcome::SendResults(slot) => assert_eq!(slot, usize::MAX),
            _ => unreachable!(),
        }
    }

    #[test]
    fn result_callback_outcome_from_send_results_is_true() {
        // bool::from should return true for SendResults
        let outcome = ResultCallbackOutcome::SendResults(0);
        assert!(bool::from(outcome));
    }

    #[test]
    fn result_callback_outcome_none_is_false() {
        // bool::from should return false for None
        let outcome = ResultCallbackOutcome::None;
        assert!(!bool::from(outcome));
    }

    #[test]
    fn result_callback_outcome_reduce_results_is_true() {
        // bool::from should return true for ReduceResults
        let outcome = ResultCallbackOutcome::ReduceResults;
        assert!(bool::from(outcome));
    }

    #[test]
    fn result_batch_slot_queue_ready_requires_entries_and_unqueued_state() {
        let (tx, _rx) = mpsc::sync_channel(1);
        let mut slot = ResultBatchSlot::new(ResultTarget::SyncUnit(tx));

        assert!(!slot.queue_ready());

        slot.entries.push_back((1, AsyncReplyPayload::Ok, 0));
        assert!(slot.queue_ready());

        slot.send_queued = true;
        assert!(!slot.queue_ready());
    }

    // === result_direct_send_ready tests (moved from runtime_shard_recv.rs) ===

    // === reduce_scheduling_predicate tests (DP-RC-0024) ===

    #[test]
    fn reduce_scheduling_predicate_empty_pending_returns_false() {
        // No pending results - never schedule
        assert!(!reduce_scheduling_predicate(
            ResultReduceTrigger::Cqe,
            0,
            false,
            200,
            None,
            0
        ));
        assert!(!reduce_scheduling_predicate(
            ResultReduceTrigger::Age,
            0,
            false,
            0,
            Some(0),
            30_000
        ));
        assert!(!reduce_scheduling_predicate(
            ResultReduceTrigger::Idle,
            0,
            false,
            0,
            None,
            0
        ));
    }

    #[test]
    fn reduce_scheduling_predicate_reducer_already_scheduled_returns_false() {
        // Reducer already scheduled - don't schedule again
        assert!(!reduce_scheduling_predicate(
            ResultReduceTrigger::Cqe,
            10,
            true,
            200,
            None,
            0
        ));
        assert!(!reduce_scheduling_predicate(
            ResultReduceTrigger::Age,
            10,
            true,
            0,
            Some(0),
            30_000
        ));
        assert!(!reduce_scheduling_predicate(
            ResultReduceTrigger::Idle,
            10,
            true,
            0,
            None,
            0
        ));
    }

    #[test]
    fn reduce_scheduling_predicate_cqe_trigger() {
        // CQE trigger: needs >= 128 cqes
        assert!(!reduce_scheduling_predicate(
            ResultReduceTrigger::Cqe,
            5,
            false,
            127,
            None,
            0
        ));
        assert!(reduce_scheduling_predicate(
            ResultReduceTrigger::Cqe,
            5,
            false,
            128,
            None,
            0
        ));
        assert!(reduce_scheduling_predicate(
            ResultReduceTrigger::Cqe,
            5,
            false,
            1000,
            None,
            0
        ));
    }

    #[test]
    fn reduce_scheduling_predicate_age_trigger() {
        // Age trigger: needs first_result_ns and age >= 20_000ns
        let first_ns = 100_000;
        let below_threshold = first_ns + 19_999;
        let at_threshold = first_ns + 20_000;
        let above_threshold = first_ns + 100_000;

        assert!(!reduce_scheduling_predicate(
            ResultReduceTrigger::Age,
            5,
            false,
            0,
            Some(first_ns),
            below_threshold
        ));
        assert!(reduce_scheduling_predicate(
            ResultReduceTrigger::Age,
            5,
            false,
            0,
            Some(first_ns),
            at_threshold
        ));
        assert!(reduce_scheduling_predicate(
            ResultReduceTrigger::Age,
            5,
            false,
            0,
            Some(first_ns),
            above_threshold
        ));
        // No first_result_ns = false
        assert!(!reduce_scheduling_predicate(
            ResultReduceTrigger::Age,
            5,
            false,
            0,
            None,
            1_000_000
        ));
    }

    #[test]
    fn reduce_scheduling_predicate_idle_trigger() {
        // Idle trigger: always true when called with pending results
        assert!(reduce_scheduling_predicate(
            ResultReduceTrigger::Idle,
            1,
            false,
            0,
            None,
            0
        ));
        assert!(reduce_scheduling_predicate(
            ResultReduceTrigger::Idle,
            100,
            false,
            0,
            None,
            0
        ));
    }

    #[test]
    fn result_direct_send_ready_requires_all_conditions() {
        // result_direct_send must be true AND pending empty AND reducer idle AND no batch slots
        assert!(result_direct_send_ready(true, 0, false, 0));
        assert!(!result_direct_send_ready(false, 0, false, 0)); // flag off
        assert!(!result_direct_send_ready(true, 1, false, 0)); // pending not empty
        assert!(!result_direct_send_ready(true, 0, true, 0)); // reducer scheduled
        assert!(!result_direct_send_ready(true, 0, false, 1)); // batch slots occupied
    }

    #[test]
    fn result_direct_send_ready_all_false_inputs() {
        assert!(!result_direct_send_ready(false, 0, false, 0));
        assert!(!result_direct_send_ready(false, 1, true, 1));
    }

    #[test]
    fn result_direct_send_ready_true_flag_with_no_other_conditions() {
        // When the flag is true but everything else is clear, ready
        assert!(result_direct_send_ready(true, 0, false, 0));
    }
}
