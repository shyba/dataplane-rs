// EXPLICIT IMPORTS — no wildcards in this file.
//
// Items from parent runtime module:
// - NifError: used by retry_eintr (from_errno)
// - recv_clock_raw: used by now_monotonic_ns
// - Result: dataplane_runtime::errors::Result type alias
// All other super::* imports removed; they were dead weight and the allow was masking
// unused-import warnings that could hide real coupling.
#[allow(unused_imports)]
use super::{recv_clock_raw, NifError, Result};
// LocalPid from rustler (used in non-SQPOLL helpers):
#[allow(unused_imports)]
use rustler::LocalPid;
// RawFd from std::os::fd (used in non-SQPOLL helpers):
#[allow(unused_imports)]
use std::os::fd::RawFd;

// Re-export helpers from split modules (DP-RP-0105–0130)
// These are conditionally re-exported depending on feature flags; allow unused.
#[allow(unused_imports)]
pub(super) use super::runtime_link_id::{
    caller_link_id, global_link_id, hash_pid, link_id, listener_link_id, session_link_id,
};
#[allow(unused_imports)]
pub(super) use super::runtime_routing::{shard_for_fd, shard_for_subscribe, shard_from_routed_id};
#[allow(unused_imports)]
pub(super) use super::runtime_send_path::{
    flush_staged_commands_all, send_many_to_shard, send_many_to_shard_linked, send_to_shard,
    send_to_shard_linked,
};

// =============================================================================
// Remaining helpers that stay in this file
// =============================================================================

pub(super) fn retry_eintr<T, F>(mut op: F) -> Result<T>
where
    F: FnMut() -> std::io::Result<T>,
{
    loop {
        match op() {
            Ok(value) => return Ok(value),
            Err(err) if err.raw_os_error() == Some(libc::EINTR) => continue,
            Err(err) => {
                return Err(NifError::from_errno(
                    err.raw_os_error().unwrap_or(libc::EIO),
                ));
            }
        }
    }
}

/// Returns the current monotonic timestamp in nanoseconds.
///
/// Uses `recv_clock_raw` from `runtime_clock` (re-exported via `super`).
pub(super) fn now_monotonic_ns() -> u64 {
    recv_clock_raw()
}
