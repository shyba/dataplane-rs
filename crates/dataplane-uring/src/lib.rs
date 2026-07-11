//! Generic io_uring runtime machinery, extracted from the ranch_uring
//! native crate (M4 step: aidocs/001_ranch_uring_runtime_migration.md).
//!
//! Nothing in this crate names an Erlang/rustler type. Delivery-facing
//! structures (`result_queue`) are generic over the reply target and
//! payload types; the NIF supplies its concrete enums.
#![deny(clippy::undocumented_unsafe_blocks)]

pub mod arena;
pub mod clock;
pub mod id_map;
pub mod limits;
pub mod reactor;
pub mod result_queue;
pub mod ring_pair;
