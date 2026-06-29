//! Error term encoding helpers for the NIF boundary.
//!
//! Encodes `dataplane_runtime::errors::NifError` as an Erlang error tuple
//! `{error, Atom}`. EOPNOTSUPP is special-cased to return the `enotsup` atom
//! per the original err_atom behavior.

use crate::atoms;
use dataplane_runtime::errors::NifError;
use rustler::{Encoder, Env, Term};

/// Encodes a `NifError` as an Erlang error tuple `{error, Atom}`.
///
/// - `EOPNOTSUPP` → `{error, enotsup}`
/// - Other errors → `{error, <atom_name>}`
pub fn encode_error<'a>(env: Env<'a>, e: &NifError) -> Term<'a> {
    let atom = match e {
        NifError::Errno(libc::EOPNOTSUPP) => atoms::enotsup(),
        _ => rustler::types::atom::Atom::from_str(env, e.atom_name())
            .unwrap_or_else(|_| atoms::error()),
    };
    (atoms::error(), atom).encode(env)
}
