# x86_64 virtio network smoke

A single-CPU, bare-metal QEMU probe using legacy PCI virtio-net. This is not a
production NIC stack. `main.rs` owns region access and orchestration;
`network.rs` validates fixed diagnostic packets; `virtio.rs` owns device queues;
`mmu.rs` and `fault.rs` prove the denied-page fault path. Optional `shard-bench`
modules exercise bounded cross-task mailboxes.

```sh
bash tools/x86_64_virtio_smoke_build.sh
bash tools/check_x86_64_virtio_smoke_contract.sh
bash tools/x86_64_virtio_smoke_run.sh

X86_64_VIRTIO_FEATURES=udp-bench bash tools/x86_64_virtio_smoke_build.sh
bash tools/check_x86_64_virtio_udp_bench_contract.sh
bash tools/x86_64_virtio_udp_bench_run.sh
```

Builds require the `x86_64-unknown-none` Rust target and GNU binutils; runs require
QEMU. UDP timings include the host harness and are not a portable core benchmark.
The default exchange and UDP runner both check receive, response, and fault
containment, rather than treating successful compilation as hardware evidence.

Device assumptions: identity-mapped, page-aligned queue memory below 4 GiB,
coherent x86 DMA, one outstanding descriptor per queue, and no concurrent or
reentrant task-region access. Ring indices use aligned volatile word accesses;
compiler fences preserve publication order. The RX descriptor is bounded by the
posted 2048-byte receive buffer, not the entire task region. IPv4/UDP validation
is intentionally limited to the deterministic, fixed-format smoke protocol.
