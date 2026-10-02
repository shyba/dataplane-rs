# Dataplane

An experimental Rust runtime for explicit task scheduling and Linux I/O, with
io_uring and a syscall fallback. Use it to explore caller-driven execution and
thread-per-core designs—not as a drop-in Tokio replacement. APIs are evolving.

## Try it

On Linux, install a current stable Rust toolchain and a C compiler/linker, then
run from this checkout:

```sh
cargo run -p dataplane --example hello_tasks
```

The [example](crates/dataplane/examples/hello_tasks.rs) implements a small
`NativeTask::step` state machine. `Runtime<T>` schedules instances of one task
type; `run()` drives them to completion and `tick()` lets you own the loop.
This entrypoint is not a general-purpose `async`/`await` executor. Raw network
submission requires explicit unsafe resource-lifetime guarantees.

To try it from another local project:

```toml
[dependencies]
dataplane = { path = "../dataplane/crates/dataplane" }
```

Adjust the path to your checkout. Keep workspace crates at the same revision.

## Find your way

- [`dataplane`](crates/dataplane): start here for the host runtime.
- [`Architecture and validation`](docs/architecture-target-validation.md): crate
  boundaries and local checks.
- [`ranch_uring`](applications/erlang/ranch_uring): Erlang NIF integration.
- `dataplane-*-smoke`: experimental hardware/QEMU scenarios, not supported OS or
  network stacks. Portable core primitives support `no_std`; the umbrella crate
  is Linux-oriented. ESP32 currently supplies clock conversion, not a HAL runtime.

## Check and measure

```sh
cargo test --workspace
make bench-core
```

The benchmark target runs portable scheduler/executor workloads, without an
Erlang VM, io_uring privileges or embedded hardware. VM-dependent Rust tests stay
ignored; Erlang integration tests require Erlang/rebar3 and run separately.

## License

[GNU Affero General Public License v3.0 only](LICENSE) (`AGPL-3.0-only`).
