# Raspi3B MMU Protected Shard Smoke

This crate is a bare-metal AArch64 smoke for QEMU's `raspi3b` machine. It is not
a new runtime implementation. It is a narrow proof that dataplane's no-alloc
scheduler surface can drive shard-local work while the ARM MMU enforces page
isolation between shards. It also proves the first Raspi3B network boundary:
QEMU attaches a VM-side `usb-net` device and a scheduler-owned task sends one
deterministic RNDIS-wrapped Ethernet probe frame through the Raspi3B DWC2 USB
host from its own protected page. The runner verifies that QEMU's net backend
captured the expected frame bytes.

## Architecture

- Boots at `0x80000`, parks secondary cores, drops from EL3 to EL2, installs an
  EL2 exception vector table, clears BSS, and calls Rust.
- Builds a small identity map with 4 KiB pages for the low 2 MiB and 2 MiB
  blocks for the rest of the Raspi3B address space.
- Wraps each 4 KiB shard page behind `ProtectedShard::with_access`, which is
  the only safe API that temporarily permits a shard page and then denies it
  again.
- Uses `FixedLocalExecCounts` from `dataplane-runtime`/core reactor no-alloc
  exports so the smoke still exercises dataplane-owned scheduling primitives.
- Runs `UsbEthernetTask` as the fourth scheduled slot. Its scratch/status page
  is the `ETHERNET_REGION`, so the task can only mutate driver-owned memory
  while `ProtectedShard::with_access` has temporarily enabled that page.
- The Ethernet task probes the DWC2 core id, powers and resets the host port,
  requires `HPRT_CONN`, enumerates QEMU's root hub, resets the downstream
  `usb-net` port, configures the RNDIS function, sends RNDIS initialize and
  packet-filter commands, then sends one RNDIS bulk OUT probe frame using
  setup/data DMA buffers inside `ETHERNET_REGION`.
- This is not yet a full Ethernet frame TX/RX driver. The current proof is one
  bounded VM-visible TX frame plus MMU isolation; RX and a request/response
  network protocol remain the next networking step.
- Deliberately writes to a denied shard page at the end. The expected EL2 data
  abort sets `FAULT_SEEN`, advances `ELR_EL2`, and exits through semihosting
  with process status zero.

## Validation

Run:

```sh
make raspi3b-mmu-smoke
```

That target builds for `aarch64-unknown-none`, checks the static contract with
`tools/check_raspi3b_mmu_smoke_contract.sh`, then runs the ELF on:

```sh
qemu-system-aarch64 -M raspi3b -cpu cortex-a53
```

The runner also attaches:

```sh
-netdev user,id=raspi_net
-object filter-dump,id=raspi_dump,netdev=raspi_net,file=$net_pcap
-device usb-net,netdev=raspi_net
```

Removing `usb-net` makes the smoke fail before the final MMU fault proof,
because the Ethernet task cannot complete the hub, RNDIS, and bulk transfer
sequence with the VM-side USB network function. Removing the filter-dump proof
or changing the deterministic Ethernet probe prefix also fails the runner.

The smoke is intentionally separate from `make embedded-ci` for now. The RP2040
and Cortex-M0 gates remain the main hosted embedded packet; this crate is the
AArch64/MMU frontier.
