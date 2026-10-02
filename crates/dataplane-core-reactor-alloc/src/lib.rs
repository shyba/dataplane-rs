//! Heap-backed, `no_std` building blocks for local reactor execution.
//!
//! [`local_exec::LocalExec`] stores FIFO work per slot; [`local_exec_counts::LocalExecCounts`]
//! stores only counts when work items are interchangeable. Both rotate runnable
//! slots after a bounded visit and track total pending work in constant time.
//! This crate requires an allocator, but does not own threads, clocks, or I/O.
#![no_std]

extern crate alloc;

pub mod local_exec;
pub mod local_exec_counts;
pub mod replay_protocol;
pub mod settings;
pub mod wait_tag;
