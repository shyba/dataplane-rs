//! Runtime budget and sizing limits.
//!
//! This module contains compile-time capacity constants used across the runtime.
//!
//! # Extracted from
//! - `runtime.rs` budget constants (DP-CS-0162)

/// Maximum number of completion queue events to process in one tick.
pub const CQE_BUDGET: usize = 256;

/// Maximum bytes per receive operation before yielding.
pub const RX_QUEUE_MAX_BYTES: usize = 512 * 1024;

/// Maximum bytes per transmit operation before yielding.
pub const TX_QUEUE_MAX_BYTES: usize = 512 * 1024;

/// Maximum number of writev operations to batch in one submit round.
pub const WRITEV_BATCH: usize = 32;

// SQPOLL arena capacity multipliers (extracted from runtime.rs DP-RB-0022)
/// Numerator for SQPOLL chunk arena capacity multiplier.
pub const SQPOLL_CHUNK_ARENA_MULTIPLIER_NUM: usize = 5;
/// Denominator for SQPOLL chunk arena capacity multiplier.
pub const SQPOLL_CHUNK_ARENA_MULTIPLIER_DEN: usize = 4;
/// SQPOLL subscribe arena capacity multiplier.
pub const SQPOLL_SUBSCRIBE_ARENA_MULTIPLIER: usize = 2;
