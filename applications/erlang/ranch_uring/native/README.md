# Ranch io_uring NIF

This crate is the Erlang/Ranch adapter, not a second copy of the reusable runtime.
`nif.rs` exposes VM calls; `runtime.rs` starts/stops shards; `runtime_shard.rs`
implements bounded pumping and completion handling. Session leases and registration
planning live in separate modules. Queue, arena, protocol, topology and error
vocabulary are re-exported from the `dataplane-*` crates.

## Ownership and configuration

- Each shard owns its rings and session state. Fixed read-buffer leases remain
  held until completion (including cancellation); generations reject stale ids.
- Senders and shards share `ShardControl`. It owns the wakeup descriptor, so both
  failed startup and final shutdown release it without closing it under a sender.
- `RANCH_URING_SHARDS` selects 1–256 shards. The effective default remains two;
  larger meshes allocate additional rings/arenas and must be explicitly requested.
  Reference policy layouts are dual-shard examples, not a cap on the hosted mesh.
  ENOMEM retries reduce the actual resolved width.
- Memlock planning budgets read and subscribe registration **per shard** against
  the process limit. Registration is setup work, not per-request allocation.
- `runtime_config.rs` snapshots environment settings; wait constants are expressed
  in nanoseconds. Do not infer units from historical comments or variable names
  containing a different suffix.

## Checks

```sh
cargo test -p ranch_uring_nif --lib
cargo test -p ranch_uring_nif --lib --features exec-strategy-sqpoll
```

Host tests exercise the pure adapter/state-machine paths. Tests marked as requiring
Rustler resource registration need a live Erlang VM; do not remove those ignores
or claim standalone Rust tests validate VM encoding. VM/network performance tests
are separate from the portable workloads run by `make bench-core` at the repository root.
The explicit ring-strategy variants and bounded pump remain separate to avoid
adding dynamic dispatch or synchronization to the execution path.
