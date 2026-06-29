//! Runtime-agnostic boundary contract between dataplane runtime operations and
//! external adapter implementations.
//!
//! This crate is intentionally narrow:
//! - define batch request/reply types shared at the boundary;
//! - define the `BoundaryAdapter` trait used to submit batches and receive
//!   asynchronous replies.
//!
//! This crate does not own runtime scheduling, io_uring/reactor execution,
//! transport implementations, or Tokio-specific glue. Tokio adapters live in
//! `dataplane-compat-tokio`; runtime internals stay in `dataplane-runtime` and
//! core reactor crates.

#![forbid(unsafe_code)]

pub mod boundary;
