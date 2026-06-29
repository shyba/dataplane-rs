//! L3 Shard Close Extraction
//!
//! This module contains close-related helpers extracted from `runtime_shard.rs`.
//! Close helpers are migrated here when their dependencies are bounded and won't
//! cause circular import chains through the `runtime` module hierarchy.
//!
//! ## Extracted helpers (in order of migration):
//! - `shutdown_kind_from_how`: Converts libc shutdown how-to to NetShutdown

use std::net::Shutdown as NetShutdown;

/// Converts a libc shutdown(2) `how` argument to the corresponding `NetShutdown` variant.
///
/// Returns `None` if `how` is not one of `SHUT_RD`, `SHUT_WR`, or `SHUT_RDWR`.
#[inline]
pub(crate) fn shutdown_kind_from_how(how: libc::c_int) -> Option<NetShutdown> {
    match how {
        libc::SHUT_RD => Some(NetShutdown::Read),
        libc::SHUT_WR => Some(NetShutdown::Write),
        libc::SHUT_RDWR => Some(NetShutdown::Both),
        _ => None,
    }
}
