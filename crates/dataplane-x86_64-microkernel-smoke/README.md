# x86_64 microkernel smoke

A bounded, single-CPU QEMU proof of dataplane task/message isolation, virtio block
and network I/O, fixed-layout FAT32 reads, and a small non-TLS HTTP/CLI appliance.
It is not a general filesystem, TCP stack, or security-hardened production kernel.

## Code map

- `kernel.rs`, `services.rs`, `task_mailbox.rs`, `kernel_ledgers.rs`: task admission,
  routing, bounded service work and observable counters.
- `arch.rs`, `task_memory.rs`: identity mappings, temporary region access and DMA
  storage. Access is single-threaded/non-reentrant; rings are page-aligned. Device
  ring fields use single volatile word accesses; fences remain at publication.
- `virtio_block.rs`, `virtio_net.rs`: device mechanics, not protocol parsing.
- `fat32.rs`, `block_runtime.rs`: known image geometry, allowlisted file handles and
  bounded reads. Do not treat the fixed fixture validator as a general FAT parser.
- `network_task/`, `tcp_stream.rs`, `http.rs`: fixed-capacity protocol state and
  responses. `scenarios/` contains positive/negative proof workloads, not drivers.
- `cli.rs`, `control_protocol.rs`: bounded operator commands and capability checks.

The explicit state machines are intentionally separate from raw hardware access.
Keep changes small: moving register or cancellation sequencing merely to reduce
line counts makes this code harder to audit.

## Validation

```sh
cargo test -p dataplane-x86_64-microkernel-smoke --test portable_boundaries
bash tools/x86_64_microkernel_smoke_build.sh
bash tools/check_x86_64_microkernel_fat32_contract.sh
bash tools/check_x86_64_microkernel_fs_service_boundary_contract.sh
bash tools/check_x86_64_microkernel_nontls_network_service_contract.sh
bash tools/check_x86_64_microkernel_nontls_network_negative_matrix_contract.sh
bash tools/x86_64_microkernel_fat32_run.sh
bash tools/x86_64_microkernel_fat32_run.sh --nontls-network-service-proof
```

The first command tests actual pure production modules without port I/O. Build/run
commands require the `x86_64-unknown-none` Rust target, GNU binutils and QEMU. The
runner creates a dedicated fixture image and retains serial/network evidence.
Emulator wall-clock timings are not portable core performance measurements.
Optional write-proof and historical milestone scripts are separate from the
read-only smoke contract; see the workspace quality ledger for audited scope.
