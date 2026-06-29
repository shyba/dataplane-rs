.PHONY: all native erlang-compile test bench-core strict-five runtime-boundary runtime-boundary-dry-run native-feature-matrix native-feature-matrix-clippy embedded-validation embedded-ci embedded-noalloc-appliance-crosscheck-contract embedded-noalloc-appliance-crosscheck prd-check prd-schema smoke clean guard-scripts-executable untracked-generated qemu-cortexm0-status qemu-cortexm0-smoke-build qemu-cortexm0-smoke qemu-cortexm0-uart-sessions-build qemu-cortexm0-uart-sessions isolated-network-task-boundary-contract raspi3b-mmu-build raspi3b-mmu-smoke raspi3b-mmu-contract x86_64-virtio-smoke-build x86_64-virtio-smoke-contract x86_64-virtio-smoke x86_64-virtio-udp-bench-build x86_64-virtio-udp-bench-contract x86_64-virtio-udp-bench x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-write-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-tls-frontier-contract x86_64-microkernel-bounded-tcp-stream-contract x86_64-microkernel-validation-matrix-contract x86_64-microkernel-validation-matrix x86_64-microkernel-validation-failure-injection-contract x86_64-microkernel-validation-failure-injection x86_64-microkernel-runner-artifact-consolidation-contract x86_64-microkernel-runner-artifact-consolidation x86_64-microkernel-http-policy-matrix-contract x86_64-microkernel-http-policy-matrix x86_64-microkernel-bounded-stream-session-refresh-contract x86_64-microkernel-bounded-stream-session-refresh x86_64-microkernel-nontls-network-service-contract x86_64-microkernel-nontls-network-service-matrix x86_64-microkernel-network-control-plane-slice-contract x86_64-microkernel-network-control-plane-slice x86_64-microkernel-nontls-network-negative-matrix-contract x86_64-microkernel-nontls-network-negative-matrix x86_64-microkernel-operator-appliance-surface-contract x86_64-microkernel-operator-appliance-surface x86_64-microkernel-operator-appliance-consolidation-contract x86_64-microkernel-operator-appliance-consolidation x86_64-microkernel-appliance-consolidation-contract x86_64-microkernel-appliance-consolidation x86_64-microkernel-fault-and-recovery-preconditions-contract x86_64-microkernel-fault-and-recovery-preconditions x86_64-microkernel-service-timeout-stale-reply-policy-contract x86_64-microkernel-service-timeout-stale-reply-policy x86_64-microkernel-storage-integrity-frontier-contract x86_64-microkernel-storage-integrity-frontier x86_64-microkernel-storage-service-counters-contract x86_64-microkernel-storage-service-counters x86_64-microkernel-service-ipc-audit-contract x86_64-microkernel-service-ipc-audit x86_64-microkernel-service-route-table-contract x86_64-microkernel-scheduler-fairness-load-contract x86_64-microkernel-scheduler-fairness-load x86_64-microkernel-mixed-appliance-fairness-contract x86_64-microkernel-mixed-appliance-fairness x86_64-microkernel-appliance-load-fairness-pack-contract x86_64-microkernel-appliance-load-fairness-pack x86_64-microkernel-memory-isolation-map-contract x86_64-microkernel-memory-isolation-map x86_64-microkernel-operator-recovery-runbook-contract x86_64-microkernel-operator-recovery-runbook x86_64-microkernel-stream-session-state-contract x86_64-microkernel-stream-session-state x86_64-microkernel-status-snapshot-consistency-contract x86_64-microkernel-status-snapshot-consistency x86_64-microkernel-filesystem-read-matrix-contract x86_64-microkernel-filesystem-read-matrix x86_64-microkernel-fs-policy-and-directory-slice-contract x86_64-microkernel-fs-policy-and-directory-slice x86_64-microkernel-fat32-smoke x86_64-microkernel-fat32-fairness x86_64-microkernel-fat32-dhcp x86_64-microkernel-fat32-write x86_64-microkernel-fat32-http-backpressure x86_64-microkernel-fs-service-boundary x86_64-microkernel-capability-fault-policy x86_64-microkernel-protocol-input-bounds-ledger-contract x86_64-microkernel-protocol-input-bounds-ledger x86_64-microkernel-network-replay-corpus-contract x86_64-microkernel-network-replay-corpus x86_64-microkernel-cli-http-operator-parity-contract x86_64-microkernel-cli-http-operator-parity x86_64-microkernel-embedded-shared-surface-refresh-contract x86_64-microkernel-embedded-shared-surface-refresh rp2040-check rp2040-core-check rp2040-runtime-check rp2040-smoke-build rp2040-smoke-size rp2040-noalloc-check rp2040-noalloc-core-check rp2040-noalloc-runtime-check rp2040-noalloc-smoke-build rp2040-noalloc-smoke-size rp2040-hardware-status
.PHONY: architecture-guards
.PHONY: x86_64-microkernel-service-route-table
.PHONY: x86_64-microkernel-embedded-shared-surface-refresh-contract x86_64-microkernel-embedded-shared-surface-refresh
.PHONY: x86_64-microkernel-status-snapshot-contract-v2-contract x86_64-microkernel-status-snapshot-contract-v2
.PHONY: x86_64-microkernel-storage-durability-design-contract x86_64-microkernel-storage-durability-design
.PHONY: x86_64-microkernel-preallocated-journal-file-proof-build x86_64-microkernel-preallocated-journal-file-proof-contract x86_64-microkernel-preallocated-journal-file-proof
.PHONY: x86_64-microkernel-storage-prereq-contract-repair-contract
.PHONY: x86_64-microkernel-http-static-appliance-polish-contract x86_64-microkernel-http-static-appliance-polish
.PHONY: x86_64-microkernel-cli-operator-surface-polish-contract x86_64-microkernel-cli-operator-surface-polish
.PHONY: x86_64-microkernel-filesystem-service-hardening-contract x86_64-microkernel-filesystem-service-hardening
.PHONY: x86_64-microkernel-nontls-control-service-frontier-contract x86_64-microkernel-nontls-control-service-frontier
.PHONY: x86_64-microkernel-network-counter-audit-contract x86_64-microkernel-network-counter-audit
.PHONY: x86_64-microkernel-status-route-unification-contract x86_64-microkernel-status-route-unification
.PHONY: x86_64-microkernel-service-boundary-audit-contract x86_64-microkernel-service-boundary-audit
.PHONY: x86_64-microkernel-http-fat32-cache-policy-contract x86_64-microkernel-http-fat32-cache-policy
.PHONY: x86_64-microkernel-control-protocol-v1-contract x86_64-microkernel-control-protocol-v1
.PHONY: x86_64-microkernel-resource-budget-ledger-contract x86_64-microkernel-resource-budget-ledger
.PHONY: x86_64-microkernel-protocol-input-bounds-ledger-contract x86_64-microkernel-protocol-input-bounds-ledger
.PHONY: x86_64-microkernel-timer-timeout-service-contract x86_64-microkernel-timer-timeout-service
.PHONY: x86_64-microkernel-stream-timeout-retry-accounting-contract x86_64-microkernel-stream-timeout-retry-accounting
.PHONY: x86_64-microkernel-service-mailbox-envelope-contract x86_64-microkernel-service-mailbox-envelope
.PHONY: x86_64-microkernel-readonly-appliance-release-packet-contract x86_64-microkernel-readonly-appliance-release-packet
.PHONY: x86_64-microkernel-readonly-appliance-load-fairness-release-contract x86_64-microkernel-readonly-appliance-load-fairness-release
.PHONY: x86_64-microkernel-readonly-appliance-load-fairness-failure-fixture-contract x86_64-microkernel-readonly-appliance-load-fairness-failure-fixture
.PHONY: x86_64-microkernel-service-lifecycle-ledger-contract x86_64-microkernel-service-lifecycle-ledger
.PHONY: x86_64-microkernel-fat32-artifact-reproducibility-contract x86_64-microkernel-fat32-artifact-reproducibility
.PHONY: x86_64-microkernel-fat32-served-byte-reproducibility-contract x86_64-microkernel-fat32-served-byte-reproducibility
.PHONY: x86_64-microkernel-fault-policy-hardening-contract x86_64-microkernel-fault-policy-hardening-failure-fixture-contract x86_64-microkernel-fault-policy-hardening-failure-fixture x86_64-microkernel-fault-policy-hardening
.PHONY: x86_64-microkernel-fat32-integrity-readonly-contract x86_64-microkernel-fat32-integrity-readonly
.PHONY: x86_64-microkernel-qemu-negative-matrix-contract x86_64-microkernel-qemu-negative-matrix
.PHONY: x86_64-microkernel-appliance-contract-snapshot-contract x86_64-microkernel-appliance-contract-snapshot
.PHONY: x86_64-microkernel-combined-cap-degradation-contract x86_64-microkernel-combined-cap-degradation
.PHONY: x86_64-microkernel-network-replay-corpus-contract x86_64-microkernel-network-replay-corpus
.PHONY: x86_64-microkernel-fat32-service-errors-contract x86_64-microkernel-fat32-service-errors
.PHONY: x86_64-microkernel-fat32-directory-index-contract x86_64-microkernel-fat32-directory-index

all: native

native:
	CFLAGS="-fpermissive" cargo build --release --manifest-path applications/erlang/ranch_uring/native/Cargo.toml
	mkdir -p applications/erlang/ranch_uring/priv
	cp target/release/libranch_uring_nif.so applications/erlang/ranch_uring/priv/ranch_uring_nif.so

erlang-compile: native
	cd applications/erlang/ranch_uring && rebar3 compile

test:
	cd applications/erlang/ranch_uring && rebar3 ct

bench-core:
	cargo bench -p dataplane-runtime --bench scheduler_hot_path
	cargo bench -p dataplane-reactor --bench threadless_runtime_loop
	cargo bench -p dataplane-reactor --bench uring_reactor

strict-five:
	./tools/run_strict_five.sh

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
	./tools/check_prd_status.sh
	./tools/check_openspec_closeout.sh
	./tools/check_active_run_status_consistency.sh

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

embedded-noalloc-appliance-crosscheck-contract:
	./tools/check_embedded_noalloc_appliance_crosscheck.sh

embedded-noalloc-appliance-crosscheck: guard-scripts-executable embedded-noalloc-appliance-crosscheck-contract embedded-ci
	DP_EMBEDDED_NOALLOC_GATES_PASSED=1 ./tools/embedded_noalloc_appliance_crosscheck_summary.sh

prd-check:
	./tools/check_prd_status.sh

# DP-NB-0020: Fresh prd.json schema run after any PRD edit
prd-schema: prd-check
	@echo "PRD schema validated - prd.json is consistent"

clean:
	cd applications/erlang/ranch_uring && rebar3 clean
	cargo clean --manifest-path applications/erlang/ranch_uring/native/Cargo.toml

qemu-cortexm0-status:
	./tools/check_qemu_cortexm0_status.sh

qemu-cortexm0-smoke-build:
	cargo build -p dataplane-qemu-cortexm0-smoke --target thumbv6m-none-eabi --release

qemu-cortexm0-smoke: guard-scripts-executable qemu-cortexm0-status qemu-cortexm0-smoke-build
	./tools/check_qemu_cortexm0_smoke_contract.sh
	./tools/qemu_cortexm0_smoke_run.sh

qemu-cortexm0-uart-sessions-build:
	cargo build -p dataplane-qemu-cortexm0-smoke --target thumbv6m-none-eabi --release --features uart-sessions

qemu-cortexm0-uart-sessions: guard-scripts-executable qemu-cortexm0-status qemu-cortexm0-uart-sessions-build
	./tools/check_qemu_cortexm0_smoke_contract.sh
	./tools/qemu_cortexm0_uart_sessions_run.sh

isolated-network-task-boundary-contract:
	./tools/check_isolated_network_task_boundary_contract.sh

raspi3b-mmu-build:
	RUSTFLAGS="-C link-arg=-Tlayout.ld" cargo build -p dataplane-raspi3b-mmu-smoke --target aarch64-unknown-none --release

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

x86_64-microkernel-fat32-artifact-reproducibility-contract:
	./tools/check_x86_64_microkernel_fat32_artifact_reproducibility_contract.sh

x86_64-microkernel-fat32-artifact-reproducibility: guard-scripts-executable x86_64-microkernel-fat32-artifact-reproducibility-contract
	./tools/x86_64_microkernel_fat32_reproducibility.sh

x86_64-microkernel-fat32-served-byte-reproducibility-contract:
	./tools/check_x86_64_microkernel_fat32_served_byte_reproducibility_contract.sh

x86_64-microkernel-fat32-served-byte-reproducibility: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-artifact-reproducibility-contract x86_64-microkernel-fat32-served-byte-reproducibility-contract
	./tools/x86_64_microkernel_fat32_served_byte_reproducibility.sh

x86_64-microkernel-fat32-integrity-readonly-contract:
	./tools/check_x86_64_microkernel_fat32_integrity_readonly_contract.sh

x86_64-microkernel-fat32-integrity-readonly: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-fat32-artifact-reproducibility-contract x86_64-microkernel-fat32-integrity-readonly-contract
	./tools/x86_64_microkernel_fat32_integrity_readonly.sh

x86_64-microkernel-qemu-negative-matrix-contract:
	./tools/check_x86_64_microkernel_qemu_negative_matrix_contract.sh

x86_64-microkernel-qemu-negative-matrix: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-qemu-negative-matrix-contract
	./tools/x86_64_microkernel_qemu_negative_matrix.sh

x86_64-microkernel-appliance-contract-snapshot-contract:
	./tools/check_x86_64_microkernel_appliance_contract_snapshot.sh

x86_64-microkernel-appliance-contract-snapshot: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-appliance-contract-snapshot-contract
	./tools/x86_64_microkernel_appliance_contract_snapshot.sh

x86_64-microkernel-combined-cap-degradation-contract:
	./tools/check_x86_64_microkernel_combined_cap_degradation_contract.sh

x86_64-microkernel-combined-cap-degradation: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-combined-cap-degradation-contract
	./tools/x86_64_microkernel_combined_cap_degradation.sh

x86_64-microkernel-network-replay-corpus-contract:
	./tools/check_x86_64_microkernel_network_replay_corpus_contract.sh

x86_64-microkernel-network-replay-corpus: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-control-protocol-v1-contract x86_64-microkernel-network-replay-corpus-contract
	./tools/x86_64_microkernel_network_replay_corpus.sh

x86_64-microkernel-fat32-service-errors-contract:
	./tools/check_x86_64_microkernel_fat32_service_errors_contract.sh

x86_64-microkernel-fat32-service-errors: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-service-errors-contract
	./tools/x86_64_microkernel_fat32_service_errors.sh

x86_64-microkernel-fat32-directory-index-contract:
	./tools/check_x86_64_microkernel_fat32_directory_index_contract.sh

x86_64-microkernel-fat32-directory-index: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-directory-index-contract
	./tools/x86_64_microkernel_fat32_directory_index.sh

x86_64-microkernel-cli-http-operator-parity-contract:
	./tools/check_x86_64_microkernel_cli_http_operator_parity_contract.sh

x86_64-microkernel-cli-http-operator-parity: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-cli-http-operator-parity-contract
	DP_MICROKERNEL_CLI_HTTP_OPERATOR_PARITY_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --cli-http-operator-parity-proof

x86_64-microkernel-embedded-shared-surface-refresh-contract:
	./tools/check_x86_64_microkernel_embedded_shared_surface_refresh_contract.sh

x86_64-microkernel-embedded-shared-surface-refresh: guard-scripts-executable x86_64-microkernel-embedded-shared-surface-refresh-contract
	@echo "x86_64 microkernel embedded shared surface refresh packet complete"

x86_64-microkernel-readonly-appliance-release-packet-contract:
	./tools/check_x86_64_microkernel_readonly_appliance_release_contract.sh

x86_64-microkernel-readonly-appliance-release-packet: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-readonly-appliance-release-packet-contract
	./tools/x86_64_microkernel_readonly_appliance_release.sh

x86_64-microkernel-readonly-appliance-load-fairness-release-contract:
	./tools/check_x86_64_microkernel_readonly_appliance_load_fairness_release_contract.sh

x86_64-microkernel-readonly-appliance-load-fairness-release: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-readonly-appliance-release-packet-contract x86_64-microkernel-scheduler-fairness-load-contract x86_64-microkernel-readonly-appliance-load-fairness-release-contract
	./tools/x86_64_microkernel_readonly_appliance_load_fairness_release.sh

x86_64-microkernel-readonly-appliance-load-fairness-failure-fixture-contract:
	./tools/check_x86_64_microkernel_readonly_appliance_load_fairness_failure_fixture_contract.sh

x86_64-microkernel-readonly-appliance-load-fairness-failure-fixture: guard-scripts-executable x86_64-microkernel-readonly-appliance-load-fairness-release-contract x86_64-microkernel-readonly-appliance-load-fairness-failure-fixture-contract
	./tools/x86_64_microkernel_readonly_appliance_load_fairness_failure_fixture.sh

x86_64-microkernel-cli-operator-contract:
	./tools/check_x86_64_microkernel_cli_operator_contract.sh

x86_64-microkernel-tls-frontier-contract:
	./tools/check_x86_64_microkernel_tls_frontier_contract.sh

x86_64-microkernel-bounded-tcp-stream-contract:
	./tools/check_x86_64_microkernel_bounded_tcp_stream_contract.sh
	./tools/check_x86_64_microkernel_tls_frontier_contract.sh

x86_64-microkernel-validation-matrix-contract:
	./tools/check_x86_64_microkernel_validation_matrix_contract.sh

x86_64-microkernel-validation-matrix: guard-scripts-executable x86_64-microkernel-validation-matrix-contract
	./tools/x86_64_microkernel_validation_matrix_run.sh --quick

x86_64-microkernel-validation-failure-injection-contract:
	./tools/check_x86_64_microkernel_validation_failure_injection_contract.sh

x86_64-microkernel-validation-failure-injection: guard-scripts-executable x86_64-microkernel-validation-matrix-contract x86_64-microkernel-validation-failure-injection-contract
	./tools/x86_64_microkernel_validation_failure_injection.sh

x86_64-microkernel-runner-artifact-consolidation-contract:
	./tools/check_x86_64_microkernel_runner_artifact_consolidation_contract.sh

x86_64-microkernel-runner-artifact-consolidation: guard-scripts-executable x86_64-microkernel-validation-matrix-contract x86_64-microkernel-validation-failure-injection-contract x86_64-microkernel-runner-artifact-consolidation-contract
	./tools/x86_64_microkernel_validation_matrix_run.sh --scenario validation-failure-injection

x86_64-microkernel-runner-fail-closed-expansion-contract:
	./tools/check_x86_64_microkernel_runner_fail_closed_expansion_contract.sh

x86_64-microkernel-runner-fail-closed-expansion: guard-scripts-executable x86_64-microkernel-validation-matrix-contract x86_64-microkernel-validation-failure-injection-contract x86_64-microkernel-runner-artifact-consolidation-contract x86_64-microkernel-runner-fail-closed-expansion-contract
	./tools/x86_64_microkernel_validation_failure_injection.sh

x86_64-microkernel-http-policy-matrix-contract:
	./tools/check_x86_64_microkernel_http_policy_matrix_contract.sh

x86_64-microkernel-http-policy-matrix: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-validation-matrix-contract x86_64-microkernel-http-policy-matrix-contract
	./tools/x86_64_microkernel_http_policy_matrix_run.sh

x86_64-microkernel-http-static-appliance-polish-contract:
	./tools/check_x86_64_microkernel_http_static_appliance_polish_contract.sh

x86_64-microkernel-http-static-appliance-polish: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-http-policy-matrix-contract x86_64-microkernel-status-snapshot-contract-v2-contract x86_64-microkernel-http-static-appliance-polish-contract
	DP_MICROKERNEL_HTTP_STATIC_APPLIANCE_POLISH_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --http-static-appliance-polish-proof

x86_64-microkernel-http-fat32-cache-policy-contract:
	./tools/check_x86_64_microkernel_http_fat32_cache_policy_contract.sh

x86_64-microkernel-http-fat32-cache-policy: guard-scripts-executable x86_64-microkernel-fat32-contract x86_64-microkernel-storage-service-counters-contract x86_64-microkernel-filesystem-read-matrix-contract x86_64-microkernel-http-static-appliance-polish-contract x86_64-microkernel-http-fat32-cache-policy-contract
	@echo "x86_64 microkernel HTTP/FAT32 cache policy proof passed."

x86_64-microkernel-cli-operator-surface-polish-contract:
	./tools/check_x86_64_microkernel_cli_operator_surface_polish_contract.sh

x86_64-microkernel-cli-operator-surface-polish: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-status-snapshot-contract-v2-contract x86_64-microkernel-storage-service-counters-contract x86_64-microkernel-cli-operator-surface-polish-contract
	DP_MICROKERNEL_CLI_OPERATOR_SURFACE_POLISH_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --cli-operator-surface-polish-proof

x86_64-microkernel-bounded-stream-session-refresh-contract:
	./tools/check_x86_64_microkernel_bounded_stream_session_refresh_contract.sh

x86_64-microkernel-bounded-stream-session-refresh: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-bounded-tcp-stream-contract x86_64-microkernel-stream-session-state-contract x86_64-microkernel-http-policy-matrix-contract x86_64-microkernel-bounded-stream-session-refresh-contract
	./tools/x86_64_microkernel_bounded_stream_session_refresh_run.sh

x86_64-microkernel-nontls-network-service-contract:
	./tools/check_x86_64_microkernel_nontls_network_service_contract.sh

x86_64-microkernel-nontls-network-service-matrix: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-nontls-network-service-contract
	./tools/x86_64_microkernel_fat32_run.sh --nontls-network-service-proof

x86_64-microkernel-network-control-plane-slice-contract:
	./tools/check_x86_64_microkernel_network_control_plane_slice_contract.sh

x86_64-microkernel-network-control-plane-slice: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-network-control-plane-slice-contract
	DP_MICROKERNEL_NETWORK_CONTROL_PLANE_SLICE_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --network-control-plane-slice-proof

x86_64-microkernel-nontls-control-service-frontier-contract:
	./tools/check_x86_64_microkernel_nontls_control_service_frontier_contract.sh

x86_64-microkernel-nontls-control-service-frontier: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-nontls-network-service-contract x86_64-microkernel-network-control-plane-slice-contract x86_64-microkernel-service-ipc-audit-contract x86_64-microkernel-service-route-table-contract x86_64-microkernel-status-snapshot-contract-v2-contract x86_64-microkernel-filesystem-service-hardening-contract x86_64-microkernel-nontls-control-service-frontier-contract
	DP_MICROKERNEL_NONTLS_CONTROL_SERVICE_FRONTIER_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --nontls-control-service-frontier-proof

x86_64-microkernel-control-protocol-v1-contract:
	./tools/check_x86_64_microkernel_control_protocol_v1_contract.sh

x86_64-microkernel-control-protocol-v1: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-control-protocol-v1-contract
	DP_MICROKERNEL_CONTROL_PROTOCOL_V1_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --control-protocol-v1-proof

x86_64-microkernel-resource-budget-ledger-contract:
	./tools/check_x86_64_microkernel_resource_budget_ledger_contract.sh

x86_64-microkernel-resource-budget-ledger: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-resource-budget-ledger-contract
	./tools/x86_64_microkernel_resource_budget_ledger.sh

x86_64-microkernel-protocol-input-bounds-ledger-contract:
	./tools/check_x86_64_microkernel_protocol_input_bounds_ledger_contract.sh

x86_64-microkernel-protocol-input-bounds-ledger: guard-scripts-executable x86_64-microkernel-protocol-input-bounds-ledger-contract
	./tools/x86_64_microkernel_protocol_input_bounds_ledger.sh

x86_64-microkernel-service-mailbox-envelope-contract:
	./tools/check_x86_64_microkernel_service_mailbox_envelope_contract.sh

x86_64-microkernel-service-mailbox-envelope: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-service-mailbox-envelope-contract
	./tools/x86_64_microkernel_fat32_run.sh

x86_64-microkernel-service-lifecycle-ledger-contract:
	./tools/check_x86_64_microkernel_service_lifecycle_ledger_contract.sh

x86_64-microkernel-service-lifecycle-ledger: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-service-lifecycle-ledger-contract
	./tools/x86_64_microkernel_fat32_run.sh

x86_64-microkernel-nontls-network-negative-matrix-contract:
	./tools/check_x86_64_microkernel_nontls_network_negative_matrix_contract.sh

x86_64-microkernel-nontls-network-negative-matrix: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-nontls-network-service-contract x86_64-microkernel-nontls-network-negative-matrix-contract
	DP_MICROKERNEL_NONTLS_NETWORK_NEGATIVE_MATRIX_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --nontls-network-negative-matrix-proof

x86_64-microkernel-operator-appliance-surface-contract:
	./tools/check_x86_64_microkernel_operator_appliance_surface_contract.sh

x86_64-microkernel-operator-appliance-surface: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-operator-appliance-surface-contract
	./tools/x86_64_microkernel_fat32_run.sh --operator-appliance-surface-proof

x86_64-microkernel-operator-appliance-consolidation-contract:
	./tools/check_x86_64_microkernel_operator_appliance_consolidation_contract.sh

x86_64-microkernel-operator-appliance-consolidation: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-operator-appliance-consolidation-contract
	DP_MICROKERNEL_OPERATOR_APPLIANCE_CONSOLIDATION_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --operator-appliance-consolidation-proof

x86_64-microkernel-appliance-consolidation-contract:
	./tools/check_x86_64_microkernel_appliance_consolidation_contract.sh

x86_64-microkernel-appliance-consolidation: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-appliance-consolidation-contract
	DP_MICROKERNEL_STORAGE_SERVICE_COUNTERS_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --storage-service-counters-proof

x86_64-microkernel-fault-and-recovery-preconditions-contract:
	./tools/check_x86_64_microkernel_fault_and_recovery_preconditions_contract.sh

x86_64-microkernel-fault-and-recovery-preconditions: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-service-ipc-audit-contract x86_64-microkernel-memory-isolation-map-contract x86_64-microkernel-operator-recovery-runbook-contract x86_64-microkernel-fault-and-recovery-preconditions-contract
	DP_MICROKERNEL_FAULT_AND_RECOVERY_PRECONDITIONS_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --fault-and-recovery-preconditions-proof

x86_64-microkernel-fault-policy-hardening-contract:
	$(MAKE) x86_64-microkernel-fat32-smoke-build
	./tools/check_x86_64_microkernel_fault_policy_hardening_contract.sh

x86_64-microkernel-fault-policy-hardening-failure-fixture-contract:
	./tools/check_x86_64_microkernel_fault_policy_hardening_failure_fixture_contract.sh

x86_64-microkernel-fault-policy-hardening-failure-fixture: guard-scripts-executable x86_64-microkernel-fault-policy-hardening-failure-fixture-contract
	@true

x86_64-microkernel-fault-policy-hardening: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-fault-policy-hardening-contract
	@true

x86_64-microkernel-service-timeout-stale-reply-policy-contract:
	./tools/check_x86_64_microkernel_service_timeout_stale_reply_policy_contract.sh

x86_64-microkernel-service-timeout-stale-reply-policy: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-service-timeout-stale-reply-policy-contract
	DP_MICROKERNEL_SERVICE_TIMEOUT_STALE_REPLY_POLICY_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --service-timeout-stale-reply-policy-proof

x86_64-microkernel-timer-timeout-service-contract:
	./tools/check_x86_64_microkernel_timer_timeout_service_contract.sh

x86_64-microkernel-timer-timeout-service: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-timer-timeout-service-contract
	DP_MICROKERNEL_TIMER_TIMEOUT_SERVICE_PROOF=1 bash ./tools/x86_64_microkernel_timer_timeout_service.sh

x86_64-microkernel-stream-timeout-retry-accounting-contract:
	./tools/check_x86_64_microkernel_stream_timeout_retry_accounting_contract.sh

x86_64-microkernel-stream-timeout-retry-accounting: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-bounded-tcp-stream-contract x86_64-microkernel-stream-session-state-contract x86_64-microkernel-bounded-stream-session-refresh-contract x86_64-microkernel-timer-timeout-service-contract x86_64-microkernel-stream-timeout-retry-accounting-contract
	./tools/x86_64_microkernel_stream_timeout_retry_accounting.sh

x86_64-microkernel-storage-integrity-frontier-contract:
	./tools/check_x86_64_microkernel_storage_integrity_contract.sh

x86_64-microkernel-storage-integrity-frontier: guard-scripts-executable x86_64-microkernel-fat32-smoke x86_64-microkernel-fat32-write x86_64-microkernel-storage-integrity-frontier-contract
	echo 'x86_64 microkernel storage integrity proof passed.'

x86_64-microkernel-storage-service-counters-contract:
	./tools/check_x86_64_microkernel_storage_service_counters_contract.sh

x86_64-microkernel-storage-service-counters: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-storage-integrity-frontier-contract x86_64-microkernel-storage-service-counters-contract
	DP_MICROKERNEL_STORAGE_SERVICE_COUNTERS_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --storage-service-counters-proof

x86_64-microkernel-storage-durability-design-contract:
	./tools/check_x86_64_microkernel_storage_durability_design_contract.sh

x86_64-microkernel-storage-durability-design: guard-scripts-executable x86_64-microkernel-storage-integrity-frontier-contract x86_64-microkernel-storage-service-counters-contract x86_64-microkernel-filesystem-read-matrix-contract x86_64-microkernel-fs-policy-and-directory-slice-contract x86_64-microkernel-fault-and-recovery-preconditions-contract x86_64-microkernel-storage-durability-design-contract
	@echo "x86_64 microkernel storage durability design guard/contract passed."

x86_64-microkernel-preallocated-journal-file-proof-contract:
	./tools/check_x86_64_microkernel_preallocated_journal_file_contract.sh

x86_64-microkernel-preallocated-journal-file-proof: guard-scripts-executable x86_64-microkernel-preallocated-journal-file-proof-build x86_64-microkernel-storage-durability-design-contract x86_64-microkernel-storage-service-counters-contract x86_64-microkernel-preallocated-journal-file-proof-contract
	DP_MICROKERNEL_PREALLOCATED_JOURNAL_FILE_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --preallocated-journal-file-proof

x86_64-microkernel-storage-prereq-contract-repair-contract:
	./tools/check_x86_64_microkernel_storage_prereq_contract_repair.sh

x86_64-microkernel-service-ipc-audit-contract:
	./tools/check_x86_64_microkernel_service_ipc_audit_contract.sh

x86_64-microkernel-service-ipc-audit: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-service-mailbox-envelope-contract x86_64-microkernel-service-route-table-contract x86_64-microkernel-service-ipc-audit-contract
	./tools/x86_64_microkernel_service_ipc_audit.sh

x86_64-microkernel-service-route-table-contract:
	./tools/check_x86_64_microkernel_service_route_table_contract.sh

x86_64-microkernel-service-route-table: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-service-mailbox-envelope-contract x86_64-microkernel-service-route-table-contract
	./tools/x86_64_microkernel_service_route_table.sh

x86_64-microkernel-service-boundary-audit-contract:
	./tools/check_x86_64_microkernel_service_boundary_audit_contract.sh

x86_64-microkernel-service-boundary-audit: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-service-mailbox-envelope-contract x86_64-microkernel-service-route-table-contract x86_64-microkernel-service-ipc-audit-contract x86_64-microkernel-service-lifecycle-ledger-contract x86_64-microkernel-service-boundary-audit-contract
	./tools/x86_64_microkernel_service_boundary_audit.sh

x86_64-microkernel-scheduler-fairness-load-contract:
	./tools/check_x86_64_microkernel_scheduler_fairness_load_contract.sh

x86_64-microkernel-scheduler-fairness-load: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-nontls-network-service-contract x86_64-microkernel-service-ipc-audit-contract x86_64-microkernel-scheduler-fairness-load-contract
	DP_MICROKERNEL_SCHEDULER_FAIRNESS_LOAD_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --scheduler-fairness-load-proof

x86_64-microkernel-mixed-appliance-fairness-contract:
	./tools/check_x86_64_microkernel_mixed_appliance_fairness_contract.sh

x86_64-microkernel-mixed-appliance-fairness: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-nontls-network-service-contract x86_64-microkernel-service-ipc-audit-contract x86_64-microkernel-scheduler-fairness-load-contract x86_64-microkernel-filesystem-read-matrix-contract x86_64-microkernel-network-control-plane-slice-contract x86_64-microkernel-fault-and-recovery-preconditions-contract x86_64-microkernel-mixed-appliance-fairness-contract
	DP_MICROKERNEL_MIXED_APPLIANCE_FAIRNESS_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --mixed-appliance-fairness-proof

x86_64-microkernel-mixed-io-fairness-frontier-contract:
	./tools/check_x86_64_microkernel_mixed_io_fairness_frontier_contract.sh

x86_64-microkernel-mixed-io-fairness-frontier: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-nontls-network-service-contract x86_64-microkernel-service-ipc-audit-contract x86_64-microkernel-scheduler-fairness-load-contract x86_64-microkernel-filesystem-read-matrix-contract x86_64-microkernel-network-control-plane-slice-contract x86_64-microkernel-fault-and-recovery-preconditions-contract x86_64-microkernel-mixed-appliance-fairness-contract x86_64-microkernel-mixed-io-fairness-frontier-contract
	DP_MICROKERNEL_MIXED_IO_FAIRNESS_FRONTIER_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --mixed-io-fairness-frontier-proof

x86_64-microkernel-appliance-load-fairness-pack-contract:
	./tools/check_x86_64_microkernel_appliance_load_fairness_pack_contract.sh

x86_64-microkernel-appliance-load-fairness-pack: guard-scripts-executable x86_64-microkernel-appliance-load-fairness-pack-contract
	./tools/x86_64_microkernel_appliance_load_fairness_pack.sh --appliance-load-fairness-pack-proof

x86_64-microkernel-memory-isolation-map-contract:
	./tools/check_x86_64_microkernel_memory_isolation_map_contract.sh

x86_64-microkernel-memory-isolation-map: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-nontls-network-service-contract x86_64-microkernel-service-ipc-audit-contract x86_64-microkernel-scheduler-fairness-load-contract x86_64-microkernel-memory-isolation-map-contract
	DP_MICROKERNEL_MEMORY_ISOLATION_MAP_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --memory-isolation-map-proof

x86_64-microkernel-operator-recovery-runbook-contract:
	./tools/check_x86_64_microkernel_operator_recovery_runbook_contract.sh

x86_64-microkernel-operator-recovery-runbook: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-service-ipc-audit-contract x86_64-microkernel-memory-isolation-map-contract x86_64-microkernel-operator-recovery-runbook-contract
	DP_MICROKERNEL_OPERATOR_RECOVERY_RUNBOOK_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --operator-recovery-runbook-proof

x86_64-microkernel-stream-session-state-contract:
	./tools/check_x86_64_microkernel_stream_session_state_contract.sh

x86_64-microkernel-stream-session-state: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-bounded-tcp-stream-contract x86_64-microkernel-stream-session-state-contract
	DP_MICROKERNEL_STREAM_SESSION_STATE_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --stream-session-state-proof

x86_64-microkernel-status-snapshot-consistency-contract:
	./tools/check_x86_64_microkernel_status_snapshot_consistency_contract.sh

x86_64-microkernel-status-snapshot-consistency: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-status-snapshot-consistency-contract
	DP_MICROKERNEL_STATUS_SNAPSHOT_CONSISTENCY_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --status-snapshot-consistency-proof

x86_64-microkernel-status-snapshot-contract-v2-contract:
	./tools/check_x86_64_microkernel_status_snapshot_contract_v2.sh

x86_64-microkernel-status-snapshot-contract-v2: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-status-snapshot-consistency-contract x86_64-microkernel-status-snapshot-contract-v2-contract
	DP_MICROKERNEL_STATUS_SNAPSHOT_V2_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --status-snapshot-v2-proof

x86_64-microkernel-status-route-unification-contract:
	./tools/check_x86_64_microkernel_status_route_unification_contract.sh

x86_64-microkernel-status-route-unification: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-service-route-table-contract x86_64-microkernel-service-ipc-audit-contract x86_64-microkernel-service-boundary-audit-contract x86_64-microkernel-status-route-unification-contract
	./tools/x86_64_microkernel_status_route_unification.sh

x86_64-microkernel-filesystem-read-matrix-contract:
	./tools/check_x86_64_microkernel_filesystem_read_matrix_contract.sh

x86_64-microkernel-filesystem-read-matrix: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-filesystem-read-matrix-contract
	DP_MICROKERNEL_FILESYSTEM_READ_MATRIX_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --filesystem-read-matrix-proof

x86_64-microkernel-fs-policy-and-directory-slice-contract:
	./tools/check_x86_64_microkernel_fs_policy_and_directory_slice_contract.sh

x86_64-microkernel-fs-policy-and-directory-slice: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-filesystem-read-matrix-contract x86_64-microkernel-fs-policy-and-directory-slice-contract
	DP_MICROKERNEL_FS_POLICY_AND_DIRECTORY_SLICE_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --fs-policy-and-directory-slice-proof

x86_64-microkernel-filesystem-service-hardening-contract:
	./tools/check_x86_64_microkernel_filesystem_service_hardening_contract.sh

x86_64-microkernel-filesystem-service-hardening: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-cli-operator-surface-polish-contract x86_64-microkernel-storage-service-counters-contract x86_64-microkernel-filesystem-service-hardening-contract
	DP_MICROKERNEL_FILESYSTEM_SERVICE_HARDENING_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --filesystem-service-hardening-proof

x86_64-microkernel-network-counter-audit-contract:
	./tools/check_x86_64_microkernel_network_counter_audit_contract.sh

x86_64-microkernel-network-counter-audit: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-nontls-network-service-contract x86_64-microkernel-network-control-plane-slice-contract x86_64-microkernel-nontls-control-service-frontier-contract x86_64-microkernel-network-counter-audit-contract
	DP_MICROKERNEL_NETWORK_COUNTER_AUDIT_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --network-counter-audit-proof

x86_64-microkernel-fat32-smoke: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract
	./tools/x86_64_microkernel_fat32_run.sh

x86_64-microkernel-fat32-fairness: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract
	./tools/x86_64_microkernel_fat32_run.sh --fairness-proof

x86_64-microkernel-fat32-dhcp: guard-scripts-executable x86_64-microkernel-fat32-smoke x86_64-microkernel-fat32-contract
	./tools/x86_64_microkernel_fat32_run.sh --dhcp-proof

x86_64-microkernel-fat32-write: guard-scripts-executable x86_64-microkernel-fat32-smoke x86_64-microkernel-fat32-write-build x86_64-microkernel-fat32-contract
	./tools/x86_64_microkernel_fat32_run.sh --write-proof
	$(MAKE) x86_64-microkernel-fat32-smoke-build

x86_64-microkernel-http-backpressure-contract: guard-scripts-executable
	./tools/check_x86_64_microkernel_http_backpressure_contract.sh

x86_64-microkernel-fat32-http-backpressure: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-http-backpressure-contract
	./tools/x86_64_microkernel_fat32_run.sh --http-backpressure-proof

x86_64-microkernel-http-backpressure: x86_64-microkernel-fat32-http-backpressure

x86_64-microkernel-fs-service-boundary: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-fs-service-boundary-contract
	DP_MICROKERNEL_FS_SERVICE_MIN_BODY=513 ./tools/x86_64_microkernel_fat32_run.sh --fs-service-boundary-proof

x86_64-microkernel-capability-fault-policy: guard-scripts-executable x86_64-microkernel-fat32-smoke-build x86_64-microkernel-fat32-contract x86_64-microkernel-cli-operator-contract x86_64-microkernel-fault-policy-hardening-contract
	DP_MICROKERNEL_CAPABILITY_FAULT_POLICY_PROOF=1 ./tools/x86_64_microkernel_fat32_run.sh --fault-policy-hardening-proof

# Non-fatal hardware status reporter (flash/debug tools + board detection)
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
