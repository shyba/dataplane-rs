//! Generic io_uring buffer, ring, and completion machinery used by the Ranch NIF.
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
