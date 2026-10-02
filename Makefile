.PHONY: all native erlang-compile test bench-core runtime-boundary runtime-boundary-dry-run native-feature-matrix native-feature-matrix-clippy embedded-validation embedded-ci smoke clean guard-scripts-executable untracked-generated qemu-cortexm0-status qemu-cortexm0-smoke-build qemu-cortexm0-smoke qemu-cortexm0-uart-sessions-build qemu-cortexm0-uart-sessions isolated-network-task-boundary-contract raspi3b-mmu-build raspi3b-mmu-smoke raspi3b-mmu-contract x86_64-virtio-smoke-build x86_64-virtio-smoke-contract x86_64-virtio-smoke x86_64-virtio-udp-bench-build x86_64-virtio-udp-bench-contract x86_64-virtio-udp-bench x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-write-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-nontls-network-service-contract x86_64-microkernel-nontls-network-service-matrix x86_64-microkernel-nontls-network-negative-matrix-contract x86_64-microkernel-nontls-network-negative-matrix x86_64-microkernel-operator-appliance-surface-contract x86_64-microkernel-operator-appliance-surface x86_64-microkernel-fat32-smoke x86_64-microkernel-fat32-fairness x86_64-microkernel-fat32-dhcp x86_64-microkernel-fat32-write x86_64-microkernel-fs-service-boundary rp2040-check rp2040-core-check rp2040-runtime-check rp2040-smoke-build rp2040-smoke-size rp2040-noalloc-check rp2040-noalloc-core-check rp2040-noalloc-runtime-check rp2040-noalloc-smoke-build rp2040-noalloc-smoke-size rp2040-scd41-smoke-build rp2040-hardware-status
.PHONY: architecture-guards
.PHONY: x86_64-microkernel-preallocated-journal-file-proof-build
all: native

native:
	CFLAGS="-fpermissive" cargo build --release --manifest-path applications/erlang/ranch_uring/native/Cargo.toml
	mkdir -p applications/erlang/ranch_uring/priv
	cp target/release/libranch_uring_nif.so applications/erlang/ranch_uring/priv/ranch_uring_nif.so

erlang-compile: native
	cd applications/erlang/ranch_uring && rebar3 compile

test:
	cd applications/erlang/ranch_uring && rebar3 ct

# Portable workloads only; real ring/network benchmarks are opt-in.
bench-core:
	cargo bench -p dataplane-core-reactor-alloc --bench local_exec
	cargo bench -p dataplane-runtime --bench scheduler_hot_path
	cargo bench -p dataplane-reactor --features runtime-tools --bench threadless_runtime_loop

# DP-NB-0008: Smoke target - runtime-boundary plus embedded-validation without benchmark work
smoke: runtime-boundary embedded-validation
	@echo "Smoke validation complete (no benchmarks run)"

# DP-NB-0006: Runtime-boundary dry-run target that performs only static guards
runtime-boundary-dry-run: guard-scripts-executable untracked-generated
	@echo "Static guards only - no runtime checks executed"

runtime-boundary:
	./tools/check_runtime_table_boundaries.sh
	./tools/check_runtime_extraction_wiring.sh
	./tools/check_runtime_api_async_guards.sh
	./tools/check_erlang_nif_exports.sh
	./tools/check_runtime_shard_wildcard_inventory.sh count
	./tools/check_placeholder_prevention.sh
	./tools/check_native_backend_dependency_hygiene.sh
	./tools/check_runtime_safety_frontier.sh

# DP-NB-0007: Guard that all check_*.sh scripts used by Makefile targets are executable
guard-scripts-executable:
	./tools/check_guard_scripts_executable.sh

architecture-guards: guard-scripts-executable
	./tools/check_noalloc_dependency_tree.sh
	./tools/check_noalloc_neutral_naming.sh
	./tools/check_embedded_host_neutral.sh
	./tools/check_rp2040_feature_separation.sh
	./tools/check_guard_scripts_executable.sh

# DP-NB-0005: Static guard for untracked generated Rust modules
untracked-generated:
	./tools/check_untracked_generated.sh

native-feature-matrix:
	./tools/check_native_feature_matrix.sh

native-feature-matrix-clippy:
	./tools/check_native_feature_matrix.sh clippy

embedded-validation:
	./tools/check_current_thread_shard_feature.sh
	cargo test -p dataplane-runtime --test embedded_host_loop_api

embedded-ci: embedded-validation qemu-cortexm0-smoke qemu-cortexm0-uart-sessions rp2040-check
	@echo "Embedded CI packet complete"

clean:
	cd applications/erlang/ranch_uring && rebar3 clean
	cargo clean --manifest-path applications/erlang/ranch_uring/native/Cargo.toml

qemu-cortexm0-status:
	./tools/check_qemu_cortexm0_status.sh

qemu-cortexm0-smoke-build:
	cargo build -p dataplane-qemu-cortexm0-smoke --target thumbv6m-none-eabi --release --features bare-metal-bin

qemu-cortexm0-smoke: guard-scripts-executable qemu-cortexm0-status qemu-cortexm0-smoke-build
	./tools/check_qemu_cortexm0_smoke_contract.sh
	./tools/qemu_cortexm0_smoke_run.sh

qemu-cortexm0-uart-sessions-build:
	cargo build -p dataplane-qemu-cortexm0-smoke --target thumbv6m-none-eabi --release --features bare-metal-bin,uart-sessions

qemu-cortexm0-uart-sessions: guard-scripts-executable qemu-cortexm0-status qemu-cortexm0-uart-sessions-build
	./tools/check_qemu_cortexm0_smoke_contract.sh
	./tools/qemu_cortexm0_uart_sessions_run.sh

isolated-network-task-boundary-contract:
	./tools/check_isolated_network_task_boundary_contract.sh

raspi3b-mmu-build:
	RUSTFLAGS="-C link-arg=-Tlayout.ld" cargo build -p dataplane-raspi3b-mmu-smoke --target aarch64-unknown-none --release --features bare-metal-bin

raspi3b-mmu-contract:
	./tools/check_raspi3b_mmu_smoke_contract.sh

raspi3b-mmu-smoke: guard-scripts-executable raspi3b-mmu-build raspi3b-mmu-contract
	./tools/raspi3b_mmu_smoke_run.sh

x86_64-virtio-smoke-build:
	./tools/x86_64_virtio_smoke_build.sh

x86_64-virtio-smoke-contract:
	./tools/check_x86_64_virtio_smoke_contract.sh

x86_64-virtio-smoke: guard-scripts-executable x86_64-virtio-smoke-build x86_64-virtio-smoke-contract
	./tools/x86_64_virtio_smoke_run.sh

x86_64-virtio-udp-bench-build:
	X86_64_VIRTIO_FEATURES=udp-bench ./tools/x86_64_virtio_smoke_build.sh

x86_64-virtio-udp-bench-contract:
	./tools/check_x86_64_virtio_udp_bench_contract.sh

x86_64-virtio-udp-bench: guard-scripts-executable x86_64-virtio-udp-bench-build x86_64-virtio-udp-bench-contract
	./tools/x86_64_virtio_udp_bench_run.sh

x86_64-microkernel-fat32-smoke-build:
	./tools/x86_64_microkernel_smoke_build.sh

x86_64-microkernel-fat32-write-build:
	X86_64_MICROKERNEL_FEATURES=fat32-write-proof ./tools/x86_64_microkernel_smoke_build.sh

x86_64-microkernel-preallocated-journal-file-proof-build:
	X86_64_MICROKERNEL_FEATURES=preallocated-journal-file-proof ./tools/x86_64_microkernel_smoke_build.sh

x86_64-microkernel-fat32-contract:
	./tools/check_x86_64_microkernel_fat32_contract.sh

x86_64-microkernel-fs-service-boundary-contract:
	./tools/check_x86_64_microkernel_fs_service_boundary_contract.sh

x86_64-microkernel-cli-operator-contract:
	./tools/check_x86_64_microkernel_cli_operator_contract.sh

x86_64-microkernel-nontls-network-service-contract:
	./tools/check_x86_64_microkernel_nontls_network_service_contract.sh

x86_64-microkernel-nontls-network-service-matrix: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-nontls-network-service-contract
	./tools/x86_64_microkernel_fat32_run.sh --nontls-network-service-proof

x86_64-microkernel-nontls-network-negative-matrix-contract:
	./tools/check_x86_64_microkernel_nontls_network_negative_matrix_contract.sh

x86_64-microkernel-nontls-network-negative-matrix: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-nontls-network-service-contract x86_64-microkernel-nontls-network-negative-matrix-contract
	DP_MICROKERNEL_NONTLS_NETWORK_NEGATIVE_MATRIX_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --nontls-network-negative-matrix-proof

x86_64-microkernel-operator-appliance-surface-contract:
	./tools/check_x86_64_microkernel_operator_appliance_surface_contract.sh

x86_64-microkernel-operator-appliance-surface: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-operator-appliance-surface-contract
	./tools/x86_64_microkernel_fat32_run.sh --operator-appliance-surface-proof

x86_64-microkernel-fat32-smoke: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract
	./tools/x86_64_microkernel_fat32_run.sh

x86_64-microkernel-fat32-fairness: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract
	./tools/x86_64_microkernel_fat32_run.sh --fairness-proof

x86_64-microkernel-fat32-dhcp: guard-scripts-executable x86_64-microkernel-fat32-smoke x86_64-microkernel-fat32-contract
	./tools/x86_64_microkernel_fat32_run.sh --dhcp-proof

x86_64-microkernel-fat32-write: guard-scripts-executable x86_64-microkernel-fat32-smoke x86_64-microkernel-fat32-write-build x86_64-microkernel-fat32-contract
	./tools/x86_64_microkernel_fat32_run.sh --write-proof
	$(MAKE) x86_64-microkernel-fat32-smoke-build

x86_64-microkernel-fs-service-boundary: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-fs-service-boundary-contract
	DP_MICROKERNEL_FS_SERVICE_MIN_BODY=513 ./tools/x86_64_microkernel_fat32_run.sh --fs-service-boundary-proof

rp2040-hardware-status:
	./tools/check_rp2040_hardware_status.sh

rp2040-core-check:
	cargo check -p dataplane-core-reactor --no-default-features --features rp2040-compile --target thumbv6m-none-eabi --lib

rp2040-runtime-check:
	cargo check -p dataplane-runtime --no-default-features --features rp2040-integration --target thumbv6m-none-eabi --lib

rp2040-smoke-build:
	./tools/rp2040_smoke_build.sh

rp2040-smoke-size: rp2040-smoke-build
	./tools/rp2040_smoke_size.sh

rp2040-scd41-smoke-build: guard-scripts-executable
	./tools/rp2040_scd41_smoke_build.sh

rp2040-noalloc-core-check:
	cargo check -p dataplane-core-reactor --no-default-features --features rp2040-noalloc --target thumbv6m-none-eabi --lib

rp2040-noalloc-runtime-check:
	cargo check -p dataplane-runtime --no-default-features --features rp2040-noalloc --target thumbv6m-none-eabi --lib

rp2040-noalloc-smoke-build:
	./tools/rp2040_noalloc_smoke_build.sh

rp2040-noalloc-smoke-size: rp2040-noalloc-smoke-build
	./tools/rp2040_noalloc_smoke_size.sh
	./tools/check_rp2040_noalloc_elf_sections.sh

# RP2040 no-alloc frontier: real compile + smoke gates, not static-only grep.
rp2040-noalloc-check: guard-scripts-executable
	./tools/check_noalloc_neutral_naming.sh
	./tools/check_rp2040_toolchain.sh
	$(MAKE) rp2040-noalloc-core-check
	$(MAKE) rp2040-noalloc-runtime-check
	./tools/check_rp2040_noalloc_static.sh
	$(MAKE) rp2040-noalloc-smoke-build
	$(MAKE) rp2040-noalloc-smoke-size
	./tools/check_noalloc_dependency_tree.sh
	./tools/check_rp2040_noalloc_smoke_contract.sh

rp2040-check: guard-scripts-executable
	./tools/check_rp2040_toolchain.sh
	./tools/check_rp2040_alloc_check.sh
	$(MAKE) rp2040-core-check
	$(MAKE) rp2040-runtime-check
	$(MAKE) rp2040-smoke-build
	$(MAKE) rp2040-smoke-size
	./tools/check_rp2040_feature_separation.sh
	$(MAKE) rp2040-noalloc-check
