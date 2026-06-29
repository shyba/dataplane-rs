//! Shard receive decision classification types.
//!
//! Extracted from runtime_shard.rs as standalone types that ShardState
//! methods reference.  The enum and helper function live here as pub(super)
//! items used by runtime_shard.rs, so that the extraction boundary is real
//! and testable.
//!
//! ## Extraction boundary
//! This module owns the receive-decision classification seam for L4.

// === Session ID helpers (DP-CC-0096, DP-CC-0103) ===

/// Number of bits reserved for the shard portion of a session ID.
pub(super) const SESSION_ID_SHARD_BITS: u64 = 56;
/// Mask for the local (lower 56) bits of a session ID.
pub(super) const SESSION_ID_LOCAL_MASK: u64 = (1u64 << SESSION_ID_SHARD_BITS) - 1;
const SESSION_ID_COUNTER_EXHAUSTED: &str =
    "session_id local counter wrapped; refusing ambiguous token reuse";

/// Advances a local session ID counter by one, returning the next ID
/// if the local counter space has not wrapped into the reserved shard bits.
///
/// Returns `None` when the local counter is exhausted (wraps into the shard
/// bits). The shard bits occupy the upper 8 bits of the 64-bit session ID;
/// this function preserves them by filtering any result where the local
/// portion would be zero.
///
/// # Arguments
/// * `local_id` - current local portion of the session ID (lower 56 bits)
///
/// # Returns
/// `Some(next)` with the next local ID, or `None` if the local counter
/// wrapped and the caller must handle re-slotting.
#[inline]
pub(super) fn advance_session_id(local_id: u64) -> Option<u64> {
    local_id
        .checked_add(1)
        .filter(|next| (next & SESSION_ID_LOCAL_MASK) != 0)
        .or_else(|| {
            log::warn!("{SESSION_ID_COUNTER_EXHAUSTED}");
            None
        })
}

/// Classification of a receive operation's readiness state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) enum RecvDecision {
    /// Receive cannot proceed - shard is busy with existing work.
    Busy,
    /// Receive can proceed immediately with the given byte count.
    Immediate(usize),
    /// No receive can start yet - waiting for I/O or slot.
    Pending,
}

/// Pure cancellation plan derived from the connection's receive-in-flight state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(super) struct CancelRecvDecision {
    pub(super) canceled_recv_token: Option<u64>,
    pub(super) request_kernel_cancel: bool,
}

/// Classify a receive decision based on current shard state.
/// Returns the same result as ShardState::classify_recv_decision.
#[inline]
pub(super) fn classify_recv_decision(
    has_pending_batch: bool,
    has_pending_recv: bool,
    rx_bytes: usize,
    len: usize,
) -> RecvDecision {
    if has_pending_batch {
        RecvDecision::Busy
    } else if rx_bytes > 0 {
        RecvDecision::Immediate(if len == 0 {
            rx_bytes
        } else {
            len.min(rx_bytes)
        })
    } else if has_pending_recv {
        RecvDecision::Busy
    } else {
        RecvDecision::Pending
    }
}

/// Classify whether a receive cancel must submit a kernel cancel SQE.
#[inline]
pub(super) fn classify_cancel_recv_decision(
    read_in_flight: bool,
    read_in_flight_token: Option<u64>,
) -> CancelRecvDecision {
    let canceled_recv_token = if read_in_flight {
        read_in_flight_token
    } else {
        None
    };
    CancelRecvDecision {
        canceled_recv_token,
        request_kernel_cancel: canceled_recv_token.is_some(),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn classify_cancel_recv_decision_ignores_stale_token_when_read_not_in_flight() {
        assert_eq!(
            classify_cancel_recv_decision(false, Some(123)),
            CancelRecvDecision {
                canceled_recv_token: None,
                request_kernel_cancel: false,
            }
        );
    }

    #[test]
    fn classify_cancel_recv_decision_preserves_token_when_kernel_cancel_required() {
        assert_eq!(
            classify_cancel_recv_decision(true, Some(123)),
            CancelRecvDecision {
                canceled_recv_token: Some(123),
                request_kernel_cancel: true,
            }
        );
    }

    #[test]
    fn classify_cancel_recv_decision_does_not_request_cancel_without_token() {
        assert_eq!(
            classify_cancel_recv_decision(true, None),
            CancelRecvDecision {
                canceled_recv_token: None,
                request_kernel_cancel: false,
            }
        );
    }

    // === advance_session_id tests (DP-CC-0096) ===

    #[test]
    fn advance_session_id_increments_normal_case() {
        // Normal increment: 1 -> 2
        assert_eq!(advance_session_id(1), Some(2));
        // One before the local mask boundary: 0x00FF_FFFF_FFFF_FFFE -> 0x00FF_FFFF_FFFF_FFFF
        assert_eq!(
            advance_session_id(0x00FF_FFFF_FFFF_FFFE),
            Some(0x00FF_FFFF_FFFF_FFFF)
        );
    }

    #[test]
    fn advance_session_id_wraps_at_local_mask_boundary() {
        // The local mask is 2^56 - 1 = 0x00FF_FFFF_FFFF_FFFF
        // Adding 1 to this gives 0x0100_0000_0000_0000 where local bits == 0
        // This is the wrap case - returns None
        assert_eq!(advance_session_id(0x00FF_FFFF_FFFF_FFFF), None);
    }

    #[test]
    fn advance_session_id_zero_is_valid_input() {
        // Session IDs start at 1 (next_session_id init = 1), but 0 is valid input
        assert_eq!(advance_session_id(0), Some(1));
    }

    #[test]
    fn advance_session_id_max_value_wraps() {
        // u64::MAX wraps to None (the +1 overflows local bits to 0 after mask)
        assert_eq!(advance_session_id(u64::MAX), None);
    }
}
