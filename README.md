# Dataplane

A sharded, thread-per-core async runtime framework for Rust, built around a
reactor abstraction with an io_uring backend (syscall fallback) on Linux and
deliberately portable down to `no_std`/no-alloc embedded targets.

One layered stack runs as:

- a **host runtime** with profiles (`Embedded` / `Balanced` / `Performance`),
- an **embedded host loop** on bare metal (RP2040, ESP32, custom kernels),
- an **embedded engine inside a foreign runtime** (Erlang NIF, Tokio) through a
  narrow batch boundary.

## Quickstart

Add the umbrella crate and implement a task:

```rust
use dataplane::{ProfileKind, Runtime};
use dataplane::task::{NativeTask, NativeTaskCx, StepResult};

struct Hello;

impl NativeTask for Hello {
    fn step(&mut self, _cx: &mut NativeTaskCx<Self>) -> StepResult {
        println!("hello from a dataplane task");
        StepResult::Complete
    }
}

fn main() {
    let mut runtime = Runtime::<Hello>::builder()
        .profile(ProfileKind::Balanced)
        .build()
        .expect("build runtime");
    runtime.spawn(Hello).expect("spawn");
    runtime.run().expect("run to completion");
}
```

Run the shipped example: `cargo run -p dataplane --example hello_tasks`.

## Crate map

| Crate | Role |
|---|---|
| `dataplane` | High-level entry point: profile builder + reactor wiring. Start here. |
| `dataplane-runtime` | Runtime integration surface: profiled runtimes, embedded host loop. |
| `dataplane-core-reactor` | The runtime core: host loop, schedulers, mailboxes, profiles (`no_std`-capable). |
| `dataplane-reactor` | Concrete reactors: io_uring, syscall, adaptive backend selection. |
| `dataplane-topology` | CPU topology and shard placement (hwloc / core affinity). |
| `dataplane-compat` / `dataplane-compat-tokio` | Runtime-agnostic batch boundary contract + Tokio adapter. |
| `dataplane-core-reactor-alloc` | `no_std + alloc` primitives shared by the core. |
| `dataplane-microkernel-core` | Bare vocabulary types for the microkernel scenario crates. |
| `dataplane-*-smoke` | Hardware/QEMU smoke targets (RP2040, Raspi3, x86_64 kernels). |
| `applications/erlang/ranch_uring` | Erlang NIF application embedding the runtime. |

## API stability

Ordinary consumers should use the `dataplane` crate. Naming types from
`dataplane_runtime::runtime_profiles` re-exports (the advanced/core API)
couples you to the matching `dataplane-core-reactor` revision — see
`docs/architecture-target-validation.md` for the boundary and compatibility
policy.

## Architecture gates

CI enforces the crate-boundary rules (compat must not depend on runtime,
reactor library builds without runtime, feature-combination checks, embedded
target checks). Run them locally before changing crate boundaries — the
commands are listed in `docs/architecture-target-validation.md`.
