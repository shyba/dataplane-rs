# Architecture and Target Validation

This repo keeps platform contracts at the lowest crate that can state them
without importing a runtime implementation.

## Layering

Dependencies point strictly downward; the one upward edge
(`dataplane-reactor -> dataplane-runtime`) is feature-gated to bin/bench
targets only (`runtime-tools`) and CI asserts it stays out of library code.

```
dataplane                      high-level entry point (stable consumer surface)
  ├─ dataplane-runtime         profiled runtimes, embedded host loop
  │    ├─ dataplane-core-reactor   runtime core: host loop, schedulers, mailboxes
  │    │    ├─ dataplane-core-reactor-alloc   no_std+alloc primitives
  │    │    └─ dataplane-topology             CPU placement, ProfileKind
  │    └─ dataplane-compat     batch boundary contract (leaf; no runtime dep)
  │         └─ dataplane-compat-tokio         Tokio adapter
  └─ dataplane-reactor         concrete reactors: io_uring, syscall, adaptive
       └─ dataplane-core-reactor
```

## Boundary Ownership

- `dataplane-compat` owns the adapter-facing batch protocol and boundary error
  contract. It must not depend on `dataplane-runtime`.
- `dataplane-runtime::runtime_protocol` re-exports the compat protocol for
  source compatibility with existing runtime callers.
- `dataplane-compat-tokio` may adapt the compat contract to Tokio channels, but
  it should not force `dataplane-compat` to know about Tokio or runtime
  internals.
- `dataplane-reactor` keeps runtime-dependent command-line tools behind the
  `runtime-tools` feature so library builds do not pull `dataplane-runtime`.

## Runtime Profile Facade

`dataplane-runtime::runtime_profiles` is the intentional advanced/core facade.
It publicly re-exports selected `dataplane-core-reactor::balanced_profile`
types because `ProfiledRuntime`, profile builders, and profile dispatch are
generic over the concrete core runtime, timer-store, park-store, and policy
types. This is not the adapter boundary and should not be treated as the stable
surface for ordinary consumers.

Compatibility policy:

- Ordinary consumers should use the `dataplane` umbrella crate, which wires
  the adaptive reactor backend into `build_profiled_runtime` and hides driver
  and store generics behind `Runtime`/`RuntimeBuilder`.
- Stable runtime callers below the umbrella should prefer profile builder
  functions, `ProfiledRuntime`, and `RuntimeLoopHandle`. `ProfiledRuntime`
  implements the `RuntimeLoop` trait, so callers can stay generic over the
  loop rather than naming store types.
- Callers that name re-exported core types are opting into the advanced/core API
  and must version `dataplane-runtime` with the matching `dataplane-core-reactor`
  workspace revision.
- Future narrowing is allowed by adding smaller facade types first, then
  deprecating direct re-exports only after an in-repo migration path exists.
- `dataplane-compat` remains below this facade; it must not import
  `runtime_profiles` or any core-reactor profile type.

## Target Recovery

The RP2040 SCD41 smoke firmware treats unrecoverable storage recovery failure as
a firmware recovery condition. It reports the failure over USB and enters
BOOTSEL instead of panicking through nested unwraps. The watchdog still covers
unstable boots; explicit BOOTSEL keeps failed storage initialization actionable
for reflashing.

## Cheap Validation Gates

Run these before changing crate boundaries:

```sh
cargo check -p dataplane-compat
cargo test -p dataplane-compat
cargo test -p dataplane-compat-tokio
cargo tree -p dataplane-compat --edges normal --invert dataplane-runtime
cargo tree -p dataplane-compat-tokio --edges normal
cargo tree -p dataplane-reactor --edges normal
```

The CI architecture gate mirrors those checks and also greps for the two
regressions that caused this review packet: a direct runtime dependency from
`dataplane-compat`, and nested storage recovery unwraps in the SCD41 firmware.
CI additionally builds and tests every core crate, checks the meaningful
feature combinations (no-default, scheduler backends, `erlang-nif`,
`runtime-tools`), and cargo-checks the `no_std` combinations on
`thumbv6m-none-eabi`.

## Decision Log

- **Umbrella crate (`dataplane`)** — Accepted. A high-level `Runtime` builder
  wires the adaptive reactor into the profiled runtime so consumers never name
  `ReactorDriver`/store generics. Chosen over widening `dataplane-runtime`
  because runtime must not depend on the concrete reactors; the umbrella sits
  above both. Consequence: the umbrella is the documented stable surface;
  `runtime_profiles` remains the advanced/core API.
- **Profile-axis collapse** — Accepted. `QueueProfile`/`TimerProfile`/
  `ParkingProfile` were a lockstep fan-out of `ProfileKind` with no
  independent production reader, so `TopologyProfile` now carries only
  `profile_kind` and consumers derive subsystem tuning from it. Consequence:
  a future need for divergent axes must reintroduce them behind constructors,
  not public fields.
- **`io_fairness` rename** — Accepted. Core-reactor's `runtime_scheduler`
  (read/write burst fairness) collided with `local_scheduler` (task mesh).
  The module is now `io_fairness`; a deprecated `runtime_scheduler` alias
  remains for one revision. `dataplane-runtime::runtime_scheduler` (the
  feature-selection shim) keeps its name — it is the scheduler-backend
  selector, not the fairness policy.
- **`dataplane-uring` extraction** — Accepted (first slice). The rustler-free
  io_uring machinery from the ranch_uring NIF (buffer pools/ring config,
  arenas, id maps, clock, limits, ring pair, result batching) now lives in
  `crates/dataplane-uring`; the result queue is generic over the delivery
  target/payload and the NIF instantiates it with its `ResultTarget`/
  `AsyncReplyPayload` enums. The dispatch/driver/shard-loop extraction is
  staged behind `ShardState` decomposition — see
  `aidocs/001_ranch_uring_runtime_migration.md`. Consequence: two uring
  stacks (`dataplane-uring::reactor` pools vs
  `dataplane-reactor::reactor::uring`) coexist until reconciliation.
- **Facade consolidation** — Accepted. `RuntimeLoop` (in
  `runtime_profiles::loop_handle`) is the single loop abstraction; it is
  implemented once over `BalancedRuntime` (all store shapes) and by
  `ProfiledRuntime`. `EmbeddedHostLoop` is a thin newtype over
  `RuntimeLoopHandle` preserving the host-neutral embedded boundary contract.
