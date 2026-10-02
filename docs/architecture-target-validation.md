# Architecture and validation

Start with `dataplane` for host use. Lower-level crates expose evolving APIs;
keep them at the same workspace revision.

## Crate boundaries

| Crate | Responsibility |
| --- | --- |
| `dataplane` | Host runtime builder and reactor wiring |
| `dataplane-runtime` | Runtime profiles and caller-driven embedded host loop |
| `dataplane-core-reactor` | Tasks, schedulers, mailboxes and driver contracts |
| `dataplane-core-reactor-alloc` | Portable allocation-backed execution primitives |
| `dataplane-reactor` | Linux io_uring/syscall drivers and backend selection |
| `dataplane-topology` | CPU placement and profile selection |
| `dataplane-compat` | Transport-neutral batch protocol; no runtime dependency |
| `dataplane-compat-tokio` | Depends on compat and adapts it to Tokio |
| `dataplane-uring` | Buffer pools, rings and completion machinery used by the NIF |
| `dataplane-nif` | Erlang term encoding and runtime adaptation |
| `dataplane-microkernel-core` | Types shared by experimental kernel scenarios |
| `dataplane-*-smoke` | Target-specific hardware/QEMU scenarios |

The umbrella depends on runtime and reactor; both use core-reactor. The reactor
library does not depend on runtime. Its optional `runtime-tools` feature enables
runtime-dependent binaries and benchmarks only. The NIF's `dataplane-uring`
machinery and the host reactor's io_uring driver are distinct implementations;
a change to one does not automatically affect the other.

## I/O and waits

Direct `Reactor::submit(NetOp)` is unsafe. Built-in driver submission uses
`RawNetOp`, whose unsafe constructor requires the caller to keep external buffers
and file descriptors valid through completion or confirmed teardown. The wrapper
does not own those resources. Generic runtime forwarding remains safe.

`ReactorDriverWait` requires an explicit `wait_deadline` implementation. The syscall
driver uses `epoll_pwait2`, with a millisecond-resolution `epoll_wait` fallback on
older kernels. Scheduling and OS latency can delay return; this is not a hard
real-time guarantee. Cross-thread wakeups are observed at wait exit.

## Local checks

Run host checks from the repository root on Linux with stable Rust:

```sh
cargo test --workspace
cargo clippy --workspace --lib -- -D warnings
RUSTDOCFLAGS='-D rustdoc::broken_intra_doc_links' cargo doc --workspace --no-deps
cargo check -p dataplane-reactor --features runtime-tools --all-targets
cargo test -p dataplane-runtime --features esp32-integration
cargo test -p dataplane-runtime --no-default-features --features host-runtime,scheduler-shares
```

Do not use `--all-features`: scheduler policies are mutually exclusive.
VM-dependent Rust tests are ignored, not passes. With Erlang/rebar3 installed,
run `rebar3 ct --suite ranch_uring_SUITE` in `applications/erlang/ranch_uring`.
The [CI workflow](../.github/workflows/architecture-gates.yml) also checks embedded
feature combinations. Hardware smoke checks need their target-specific tools;
see each scenario's README or build script.

## Benchmarks

`make bench-core` runs portable allocator, scheduler and mock-driver workloads.
Real io_uring/network benchmarks are opt-in; they are not part of that target.
Compare the same harness and machine before/after a change, and retain Criterion
baselines locally. Results from different machines or build settings are not
interchangeable.
