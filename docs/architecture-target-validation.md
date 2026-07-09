# Architecture and Target Validation

This repo keeps platform contracts at the lowest crate that can state them
without importing a runtime implementation.

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

- Stable runtime callers should prefer profile builder functions,
  `ProfiledRuntime`, and `RuntimeLoopHandle`.
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
