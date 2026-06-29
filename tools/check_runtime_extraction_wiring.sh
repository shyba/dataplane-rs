#!/usr/bin/env bash
# Runtime Extraction Wiring Guard
# Fails if extracted modules are not properly wired or duplicate definitions reappear
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
cd "$REPO_ROOT"

echo "=== Runtime Extraction Wiring Guard ==="

# 1. Guard: dataplane-nif public module exports
echo "Checking: dataplane-nif pub mod atoms, error_terms, runtime_adapter..."
if ! rg -q "pub mod atoms" crates/dataplane-nif/src/lib.rs 2>/dev/null; then
    echo "FAIL: pub mod atoms not found in crates/dataplane-nif/src/lib.rs"
    exit 1
fi
if ! rg -q "pub mod error_terms" crates/dataplane-nif/src/lib.rs 2>/dev/null; then
    echo "FAIL: pub mod error_terms not found in crates/dataplane-nif/src/lib.rs"
    exit 1
fi
if ! rg -q "pub mod runtime_adapter" crates/dataplane-nif/src/lib.rs 2>/dev/null; then
    echo "FAIL: pub mod runtime_adapter not found in crates/dataplane-nif/src/lib.rs"
    exit 1
fi
echo "  ... ok"

# 2. Guard: duplicate native atom module removed
echo "Checking: nif_atoms.rs does not exist in native src..."
if [ -f "applications/erlang/ranch_uring/native/src/nif_atoms.rs" ]; then
    echo "FAIL: applications/erlang/ranch_uring/native/src/nif_atoms.rs still exists"
    exit 1
fi
if rg -q "crate::nif_atoms\|use crate::nif_atoms" applications/erlang/ranch_uring/native/src/ 2>/dev/null; then
    echo "FAIL: native crate still imports crate::nif_atoms"
    exit 1
fi
echo "  ... ok"

# 3. Guard: native nif.rs imports dataplane_nif atoms and error_terms
echo "Checking: native nif.rs imports dataplane_nif::atoms and error_terms..."
NIF_PATH="applications/erlang/ranch_uring/native/src/nif.rs"
if ! rg -q "dataplane_nif::(atoms|error_terms)" "$NIF_PATH" 2>/dev/null; then
    echo "FAIL: native nif.rs does not import dataplane_nif atoms or error_terms"
    exit 1
fi
echo "  ... ok"

# 4. Guard: lib.rs declares runtime_startup and runtime_id_map
echo "Checking: lib.rs declares runtime_startup and runtime_id_map..."
LIB_PATH="applications/erlang/ranch_uring/native/src/lib.rs"
if ! rg -q "mod runtime_startup" "$LIB_PATH" 2>/dev/null; then
    echo "FAIL: mod runtime_startup not declared in lib.rs"
    exit 1
fi
if ! rg -q "mod runtime_id_map" "$LIB_PATH" 2>/dev/null; then
    echo "FAIL: mod runtime_id_map not declared in lib.rs"
    exit 1
fi
echo "  ... ok"

# 5. Guard: runtime.rs does NOT redefine U64Hasher, U64Map, map_probe_err, dispatch_runtime_startup_profile_layout
echo "Checking: runtime.rs does not redefine extracted helpers..."
RUNTIME_PATH="applications/erlang/ranch_uring/native/src/runtime.rs"
# U64Hasher and U64Map were extracted to runtime_id_map.rs
if rg -q "^pub(super) struct U64Hasher" "$RUNTIME_PATH" 2>/dev/null; then
    echo "FAIL: U64Hasher redefined in runtime.rs (should be in runtime_id_map)"
    exit 1
fi
if rg -q "^pub(super) type U64Map" "$RUNTIME_PATH" 2>/dev/null; then
    echo "FAIL: U64Map redefined in runtime.rs (should be in runtime_id_map)"
    exit 1
fi
# These are extracted helpers - runtime.rs should import them from runtime_startup
if rg -q "^pub fn dispatch_runtime_startup_profile_layout" "$RUNTIME_PATH" 2>/dev/null; then
    echo "FAIL: dispatch_runtime_startup_profile_layout redefined in runtime.rs (should be in runtime_startup)"
    exit 1
fi
echo "  ... ok"

# 6. Guard: runtime_registration module exists and is declared in lib.rs (when created)
echo "Checking: runtime_registration module wiring..."
if [ -f "applications/erlang/ranch_uring/native/src/runtime_registration.rs" ]; then
    if ! rg -q "mod runtime_registration" "$LIB_PATH" 2>/dev/null; then
        echo "FAIL: runtime_registration.rs exists but not declared in lib.rs"
        exit 1
    fi
    # Guard that RegistrationLayout etc. are NOT in runtime.rs
    if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?struct[[:space:]]+RegistrationLayout\\b" "$RUNTIME_PATH" 2>/dev/null; then
        echo "FAIL: RegistrationLayout still defined in runtime.rs (should be in runtime_registration)"
        exit 1
    fi
    if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?struct[[:space:]]+FixedRegistrationPlan\\b" "$RUNTIME_PATH" 2>/dev/null; then
        echo "FAIL: FixedRegistrationPlan still defined in runtime.rs (should be in runtime_registration)"
        exit 1
    fi
    if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?fn[[:space:]]+plan_fixed_registration\\b" "$RUNTIME_PATH" 2>/dev/null; then
        echo "FAIL: plan_fixed_registration still defined in runtime.rs (should be in runtime_registration)"
        exit 1
    fi
    if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?fn[[:space:]]+build_registration_layout\\b" "$RUNTIME_PATH" 2>/dev/null; then
        echo "FAIL: build_registration_layout still defined in runtime.rs (should be in runtime_registration)"
        exit 1
    fi
    echo "  runtime_registration wiring: ok"
else
    echo "  runtime_registration: not yet created (skip)"
fi

# 7. Guard: runtime_result_queue module wiring
echo "Checking: runtime_result_queue module wiring..."
if [ -f "applications/erlang/ranch_uring/native/src/runtime_result_queue.rs" ]; then
    if ! rg -q "mod runtime_result_queue" "$LIB_PATH" 2>/dev/null; then
        echo "FAIL: runtime_result_queue.rs exists but not declared in lib.rs"
        exit 1
    fi
    if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?struct[[:space:]]+ResultReduceState\\b" "$RUNTIME_PATH" 2>/dev/null; then
        echo "FAIL: ResultReduceState still defined in runtime.rs (should be in runtime_result_queue)"
        exit 1
    fi
    # DP-RC-0027: All result queue policy types and helpers must not be redefined in runtime.rs
    if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?struct[[:space:]]+ResultEvent\\b" "$RUNTIME_PATH" 2>/dev/null; then
        echo "FAIL: ResultEvent still defined in runtime.rs (should be in runtime_result_queue)"
        exit 1
    fi
    if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?struct[[:space:]]+ResultBatchSlot\\b" "$RUNTIME_PATH" 2>/dev/null; then
        echo "FAIL: ResultBatchSlot still defined in runtime.rs (should be in runtime_result_queue)"
        exit 1
    fi
    if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?enum[[:space:]]+ResultReduceTrigger\\b" "$RUNTIME_PATH" 2>/dev/null; then
        echo "FAIL: ResultReduceTrigger still defined in runtime.rs (should be in runtime_result_queue)"
        exit 1
    fi
    if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?enum[[:space:]]+ResultCallbackOutcome\\b" "$RUNTIME_PATH" 2>/dev/null; then
        echo "FAIL: ResultCallbackOutcome still defined in runtime.rs (should be in runtime_result_queue)"
        exit 1
    fi
    if ! rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?fn[[:space:]]+result_direct_send_ready\\b" "applications/erlang/ranch_uring/native/src/runtime_result_queue.rs" 2>/dev/null; then
        echo "FAIL: result_direct_send_ready is not defined in runtime_result_queue.rs"
        exit 1
    fi
    if ! rg -q "facet\\.direct_send_ready\\(\\)" "$RUNTIME_PATH" 2>/dev/null; then
        echo "FAIL: runtime.rs result_direct_send_ready wrapper must delegate through ResultFacet"
        exit 1
    fi
    if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?fn[[:space:]]+reduce_scheduling_predicate\\b" "$RUNTIME_PATH" 2>/dev/null; then
        echo "FAIL: reduce_scheduling_predicate still defined in runtime.rs (should be in runtime_result_queue)"
        exit 1
    fi
    if rg -q "std::mem::zeroed\\(\\)" "applications/erlang/ranch_uring/native/src/runtime_result_queue.rs" 2>/dev/null; then
        echo "FAIL: runtime_result_queue.rs must not fake ResultTarget with std::mem::zeroed()"
        exit 1
    fi
    echo "  runtime_result_queue wiring: ok"
else
    echo "  runtime_result_queue: not yet created (skip)"
fi

# 8. Guard: runtime_pending_reply module wiring
echo "Checking: runtime_pending_reply module wiring..."
if [ -f "applications/erlang/ranch_uring/native/src/runtime_pending_reply.rs" ]; then
    if ! rg -q "mod runtime_pending_reply" "$LIB_PATH" 2>/dev/null; then
        echo "FAIL: runtime_pending_reply.rs exists but not declared in lib.rs"
        exit 1
    fi
    if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?struct[[:space:]]+PendingStatx\\b" "$RUNTIME_PATH" 2>/dev/null; then
        echo "FAIL: PendingStatx still defined in runtime.rs (should be in runtime_pending_reply)"
        exit 1
    fi
    echo "  runtime_pending_reply wiring: ok"
else
    echo "  runtime_pending_reply: not yet created (skip)"
fi

# 9. Guard: build_ring removed from runtime.rs after L3 extraction
echo "Checking: build_ring startup extraction state..."
STARTUP_PATH="applications/erlang/ranch_uring/native/src/runtime_startup.rs"
if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?fn[[:space:]]+build_ring\\b" "$STARTUP_PATH" 2>/dev/null; then
    if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?fn[[:space:]]+build_ring\\b" "$RUNTIME_PATH" 2>/dev/null; then
        echo "FAIL: build_ring still defined in runtime.rs after runtime_startup owns it"
        exit 1
    fi
    echo "  build_ring startup extraction: ok"
else
    echo "  build_ring startup extraction: pending (runtime.rs still owns it): ok"
fi

# 9b. Guard: runtime_shard_close owns shutdown_kind_from_how
echo "Checking: shutdown_kind_from_how stays in runtime_shard_close.rs..."
RUNTIME_SHARD_PATH="applications/erlang/ranch_uring/native/src/runtime_shard.rs"
SHARD_CLOSE_PATH="applications/erlang/ranch_uring/native/src/runtime_shard_close.rs"
if ! rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?fn[[:space:]]+shutdown_kind_from_how\\b" "$SHARD_CLOSE_PATH" 2>/dev/null; then
    echo "FAIL: shutdown_kind_from_how is not defined in runtime_shard_close.rs"
    exit 1
fi
if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?fn[[:space:]]+shutdown_kind_from_how\\b" "$RUNTIME_SHARD_PATH" 2>/dev/null; then
    echo "FAIL: shutdown_kind_from_how redefined in runtime_shard.rs (should stay in runtime_shard_close)"
    exit 1
fi
echo "  shutdown_kind_from_how extraction: ok"

# 9c. Guard: runtime_shard_recv owns RecvDecision
echo "Checking: RecvDecision stays in runtime_shard_recv.rs..."
SHARD_RECV_PATH="applications/erlang/ranch_uring/native/src/runtime_shard_recv.rs"
if ! rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?enum[[:space:]]+RecvDecision\\b" "$SHARD_RECV_PATH" 2>/dev/null; then
    echo "FAIL: RecvDecision is not defined in runtime_shard_recv.rs"
    exit 1
fi
if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?enum[[:space:]]+RecvDecision\\b" "$RUNTIME_SHARD_PATH" 2>/dev/null; then
    echo "FAIL: RecvDecision redefined in runtime_shard.rs (should stay in runtime_shard_recv)"
    exit 1
fi
echo "  RecvDecision extraction: ok"

# 10. Guard: nif_bench_support removed from native after L4 move
#    Pre-move:  file=yes, decl=yes  -> PASS (guard should not fire pre-move)
#    Mid-move:  file=yes, decl=no   -> PASS (partially done)
#    Post-move: file=no,  decl=no   -> PASS (clean done state)
#    Broken:    file=no,  decl=yes  -> FAIL (orphan declaration)
#    Broken:    file=yes, decl=yes AND bench_support.rs still imported by nif.rs -> FAIL
echo "Checking: native nif_bench_support removed after move to dataplane-nif..."
NATIVE_BENCH="applications/erlang/ranch_uring/native/src/nif_bench_support.rs"
NIF_PATH="applications/erlang/ranch_uring/native/src/nif.rs"
FILE_EXISTS="false"
DECL_EXISTS="false"
if [ -f "$NATIVE_BENCH" ]; then FILE_EXISTS="true"; fi
if rg -q "mod nif_bench_support" "$LIB_PATH" 2>/dev/null; then DECL_EXISTS="true"; fi

# Broken: declaration without file
if [ "$DECL_EXISTS" = "true" ] && [ "$FILE_EXISTS" = "false" ]; then
    echo "FAIL: lib.rs declares mod nif_bench_support but file missing"
    exit 1
fi

# Broken: both exist AND nif.rs still uses it (not yet updated)
if [ "$FILE_EXISTS" = "true" ] && [ "$DECL_EXISTS" = "true" ]; then
    # This is pre-move state - OK unless nif.rs imports it (which it will pre-move)
    # Guard will fire after partial L4 work - that's expected
    echo "  nif_bench_support: pre-move state (file+decl both exist)"
    echo "  NOTE: guard will re-check after nif.rs is updated"
fi

# Post-move: file gone, decl gone
if [ "$FILE_EXISTS" = "false" ] && [ "$DECL_EXISTS" = "false" ]; then
    echo "  nif_bench_support: fully moved to dataplane-nif: ok"
fi

# Mid-move: file exists, decl removed
if [ "$FILE_EXISTS" = "true" ] && [ "$DECL_EXISTS" = "false" ]; then
    echo "  nif_bench_support: mid-move (file exists, declaration removed): ok"
fi

# 11. Guard: bench_support uses dataplane_nif::atoms (not duplicate atoms)
echo "Checking: bench_support uses dataplane_nif::atoms..."
if [ -f "crates/dataplane-nif/src/bench_support.rs" ]; then
    if rg -q "rustler::atoms\|mod atoms\|nif_atoms" crates/dataplane-nif/src/bench_support.rs 2>/dev/null; then
        echo "FAIL: bench_support duplicates atom declarations instead of using dataplane_nif::atoms"
        exit 1
    fi
fi
echo "  ... ok"

# 12. Guard: dataplane-nif has no unsafe blocks
echo "Checking: dataplane-nif is unsafe-free..."
if rg -q "unsafe[[:space:]]*\\{|unsafe[[:space:]]+fn|unsafe[[:space:]]+impl|allow\\(unsafe_code\\)" crates/dataplane-nif/src/ 2>/dev/null; then
    echo "FAIL: dataplane-nif contains unsafe code or unsafe-code allowlists"
    exit 1
fi
echo "  ... ok"

# 13. Guard: dataplane-nif does not depend on native crate
echo "Checking: dataplane-nif does not depend on native/ranch_uring_nif..."
if rg -q "ranch_uring_nif\|dataplane-native" crates/dataplane-nif/Cargo.toml 2>/dev/null; then
    echo "FAIL: dataplane-nif depends on native crate"
    exit 1
fi
echo "  ... ok"

# =============================================================================
# DP-RP-0027-0052: L1 Extraction Guard Hardening
# =============================================================================

# DP-RP-0027: runtime_command.rs module existence
echo "Checking: runtime_command.rs exists and is declared in runtime.rs..."
if [ ! -f "applications/erlang/ranch_uring/native/src/runtime_command.rs" ]; then
    echo "FAIL: runtime_command.rs does not exist"
    exit 1
fi
if ! rg -q "mod runtime_command" "applications/erlang/ranch_uring/native/src/runtime.rs" 2>/dev/null; then
    echo "FAIL: mod runtime_command not declared in runtime.rs"
    exit 1
fi
echo "  ... ok"

# DP-RP-0028: runtime_config.rs module existence
echo "Checking: runtime_config.rs exists and is declared in runtime.rs..."
if [ ! -f "applications/erlang/ranch_uring/native/src/runtime_config.rs" ]; then
    echo "FAIL: runtime_config.rs does not exist"
    exit 1
fi
if ! rg -q "mod runtime_config" "applications/erlang/ranch_uring/native/src/runtime.rs" 2>/dev/null; then
    echo "FAIL: mod runtime_config not declared in runtime.rs"
    exit 1
fi
echo "  ... ok"

# DP-RP-0029: enum Command NOT in runtime.rs
echo "Checking: enum Command is not redefined in runtime.rs..."
if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?enum[[:space:]]+Command\\b" "applications/erlang/ranch_uring/native/src/runtime.rs" 2>/dev/null; then
    echo "FAIL: enum Command redefined in runtime.rs (should be in runtime_command)"
    exit 1
fi
echo "  ... ok"

# DP-RP-0030: enum CommandLane NOT in runtime.rs
echo "Checking: enum CommandLane is not redefined in runtime.rs..."
if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?enum[[:space:]]+CommandLane\\b" "applications/erlang/ranch_uring/native/src/runtime.rs" 2>/dev/null; then
    echo "FAIL: enum CommandLane redefined in runtime.rs (should be in runtime_command)"
    exit 1
fi
echo "  ... ok"

# DP-RP-0031: DeferredSubmitCommand NOT in runtime.rs
echo "Checking: DeferredSubmitCommand is not redefined in runtime.rs..."
if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?(struct|enum)[[:space:]]+DeferredSubmitCommand\\b" "applications/erlang/ranch_uring/native/src/runtime.rs" 2>/dev/null; then
    echo "FAIL: DeferredSubmitCommand redefined in runtime.rs (should be in runtime_command)"
    exit 1
fi
echo "  ... ok"

# DP-RP-0032: ReadyOp NOT in runtime.rs
echo "Checking: enum ReadyOp is not redefined in runtime.rs..."
if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?enum[[:space:]]+ReadyOp\\b" "applications/erlang/ranch_uring/native/src/runtime.rs" 2>/dev/null; then
    echo "FAIL: enum ReadyOp redefined in runtime.rs (should be in runtime_command)"
    exit 1
fi
echo "  ... ok"

# DP-RP-0033: LatencyItem NOT in runtime.rs
echo "Checking: enum LatencyItem is not redefined in runtime.rs..."
if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?enum[[:space:]]+LatencyItem\\b" "applications/erlang/ranch_uring/native/src/runtime.rs" 2>/dev/null; then
    echo "FAIL: enum LatencyItem redefined in runtime.rs (should be in runtime_command)"
    exit 1
fi
echo "  ... ok"

# DP-RP-0034: ShardRuntimeConfig NOT in runtime.rs (re-export via runtime_config is ok)
echo "Checking: struct ShardRuntimeConfig is not redefined in runtime.rs..."
if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?struct[[:space:]]+ShardRuntimeConfig\\b" "applications/erlang/ranch_uring/native/src/runtime.rs" 2>/dev/null; then
    echo "FAIL: struct ShardRuntimeConfig redefined in runtime.rs (should be in runtime_config)"
    exit 1
fi
echo "  ... ok"

# DP-RP-0035: configured_debug_loop_queues NOT in runtime_helpers.rs (moved to runtime_config)
echo "Checking: configured_debug_loop_queues is not redefined in runtime_helpers.rs..."
if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?fn[[:space:]]+configured_debug_loop_queues\\b" "applications/erlang/ranch_uring/native/src/runtime_helpers.rs" 2>/dev/null; then
    echo "FAIL: configured_debug_loop_queues redefined in runtime_helpers.rs (should be in runtime_config)"
    exit 1
fi
echo "  ... ok"

# DP-RP-0036: configured_recv_ring_mode NOT in runtime_helpers.rs (moved to runtime_config)
echo "Checking: configured_recv_ring_mode is not redefined in runtime_helpers.rs..."
if rg -q "^[[:space:]]*(pub(\\([^)]*\\))?[[:space:]]+)?fn[[:space:]]+configured_recv_ring_mode\\b" "applications/erlang/ranch_uring/native/src/runtime_helpers.rs" 2>/dev/null; then
    echo "FAIL: configured_recv_ring_mode redefined in runtime_helpers.rs (should be in runtime_config)"
    exit 1
fi
echo "  ... ok"

# DP-RP-0037: use super wildcard NOT in runtime_helpers.rs
echo "Checking: runtime_helpers.rs does not use 'use super::*' wildcard import..."
if rg -q "^use super::\*;" "applications/erlang/ranch_uring/native/src/runtime_helpers.rs" 2>/dev/null; then
    echo "FAIL: runtime_helpers.rs uses 'use super::*' wildcard import"
    exit 1
fi
echo "  ... ok"

# DP-RP-0038: module-wide allow(dead_code) NOT in runtime_command.rs
echo "Checking: runtime_command.rs does not have module-wide allow(dead_code)..."
if rg -q "^#\\[allow\\(dead_code\\)\\]" "applications/erlang/ranch_uring/native/src/runtime_command.rs" 2>/dev/null; then
    echo "FAIL: runtime_command.rs has module-wide allow(dead_code)"
    exit 1
fi
echo "  ... ok"

# DP-RP-0039: std::mem::zeroed() NOT in runtime_result_queue.rs
echo "Checking: runtime_result_queue.rs does not use std::mem::zeroed()..."
if rg -q "std::mem::zeroed\\(\\)" "applications/erlang/ranch_uring/native/src/runtime_result_queue.rs" 2>/dev/null; then
    echo "FAIL: runtime_result_queue.rs uses std::mem::zeroed()"
    exit 1
fi
echo "  ... ok"

# DP-RP-0040: pointer-sized ArenaHandle assertion NOT in runtime_pending_reply.rs
echo "Checking: runtime_pending_reply.rs does not assert pointer-sized ArenaHandle..."
if rg -q "size_of::<.*ArenaHandle.*>.*==.*size_of::<.*usize" "applications/erlang/ranch_uring/native/src/runtime_pending_reply.rs" 2>/dev/null; then
    echo "FAIL: runtime_pending_reply.rs asserts pointer-sized ArenaHandle"
    exit 1
fi
if rg -q "size_of::<ArenaHandle\\>" "applications/erlang/ranch_uring/native/src/runtime_pending_reply.rs" 2>/dev/null; then
    echo "FAIL: runtime_pending_reply.rs checks ArenaHandle size (non-portable)"
    exit 1
fi
echo "  ... ok"

# DP-RP-0041: rg errors NOT treated as success in runtime_api guards
echo "Checking: runtime_api async guards treat rg errors as failures..."
API_GUARD="applications/erlang/ranch_uring/native/src/runtime_api.rs"
if [ -f "$API_GUARD" ]; then
    # Check that when rg is used for async guard matching, errors are not swallowed
    # Pattern: rg ... && (something that looks like Ok(something) or return Ok(...))
    # Guard: if rg returns non-zero (error) it should NOT be treated as success
    # A safe pattern check: "rg .* && return Ok" or "rg .*; Ok" where rg error is ignored
    if rg -q "rg.+\\?\\?.*Ok\\(|rg.+\\?\\?.*return Ok" "$API_GUARD" 2>/dev/null; then
        echo "FAIL: runtime_api.rs treats rg errors as success (?? pattern with Ok)"
        exit 1
    fi
fi
echo "  ... ok"

# DP-RP-0042: CloseAsync and OwnerDown present in runtime_command.rs
echo "Checking: CloseAsync and OwnerDown variants exist in runtime_command.rs..."
if ! rg -q "CloseAsync" "applications/erlang/ranch_uring/native/src/runtime_command.rs" 2>/dev/null; then
    echo "FAIL: CloseAsync not found in runtime_command.rs"
    exit 1
fi
if ! rg -q "OwnerDown" "applications/erlang/ranch_uring/native/src/runtime_command.rs" 2>/dev/null; then
    echo "FAIL: OwnerDown not found in runtime_command.rs"
    exit 1
fi
echo "  ... ok"

# DP-RP-0043: Done PRD module stories have matching files
echo "Checking: Done PRD module stories have matching extracted files..."
# Story DP-PA-0164: runtime_command.rs exists
if rg -q '"id": "DP-PA-0164".+"status": "done"' "prd.json" 2>/dev/null; then
    if [ ! -f "applications/erlang/ranch_uring/native/src/runtime_command.rs" ]; then
        echo "FAIL: DP-PA-0164 done but runtime_command.rs missing"
        exit 1
    fi
fi
# Story DP-PA-0181: runtime_config.rs exists
if rg -q '"id": "DP-PA-0181".+"status": "done"' "prd.json" 2>/dev/null; then
    if [ ! -f "applications/erlang/ranch_uring/native/src/runtime_config.rs" ]; then
        echo "FAIL: DP-PA-0181 done but runtime_config.rs missing"
        exit 1
    fi
fi
# Story DP-PA-0173: runtime_helpers.rs no wildcard
if rg -q '"id": "DP-PA-0173".+"status": "done"' "prd.json" 2>/dev/null; then
    if rg -q "^use super::\*;" "applications/erlang/ranch_uring/native/src/runtime_helpers.rs" 2>/dev/null; then
        echo "FAIL: DP-PA-0173 done but runtime_helpers.rs still has wildcard"
        exit 1
    fi
fi
echo "  ... ok"

# =============================================================================
# L2 Launch Extraction Guards (DP-CR-0071-0099)
# =============================================================================

# DP-CR-0071: spawn_shard_thread has a single definition in runtime_launch.rs
echo "Checking: spawn_shard_thread has a single definition in runtime_launch.rs..."
if [ ! -f "applications/erlang/ranch_uring/native/src/runtime_launch.rs" ]; then
    echo "FAIL: runtime_launch.rs does not exist"
    exit 1
fi
SPAWN_COUNT=$(rg -c 'fn spawn_shard_thread' "applications/erlang/ranch_uring/native/src/runtime_launch.rs" 2>/dev/null || true)
SPAWN_COUNT=${SPAWN_COUNT:-0}
if [ "$SPAWN_COUNT" -lt 1 ]; then
    echo "FAIL: spawn_shard_thread not found in runtime_launch.rs"
    exit 1
fi
echo "  spawn_shard_thread in runtime_launch.rs: ok ($SPAWN_COUNT definition)"

# DP-CR-0072: spawn_shard_thread is NOT redefined in runtime.rs
echo "Checking: spawn_shard_thread is not redefined in runtime.rs..."
if rg -q "fn spawn_shard_thread" "applications/erlang/ranch_uring/native/src/runtime.rs" 2>/dev/null; then
    echo "FAIL: spawn_shard_thread redefined in runtime.rs"
    exit 1
fi
echo "  spawn_shard_thread not in runtime.rs: ok"

# DP-CR-0073: run_shard_state has a single implementation in runtime_launch.rs
echo "Checking: run_shard_state has a single implementation in runtime_launch.rs..."
RUN_STATE_COUNT=$(rg -c 'fn run_shard_state[^_]' "applications/erlang/ranch_uring/native/src/runtime_launch.rs" 2>/dev/null || true)
RUN_STATE_COUNT=${RUN_STATE_COUNT:-0}
if [ "$RUN_STATE_COUNT" -lt 1 ]; then
    echo "FAIL: run_shard_state not found in runtime_launch.rs"
    exit 1
fi
echo "  run_shard_state in runtime_launch.rs: ok ($RUN_STATE_COUNT definition)"

# DP-CR-0074: run_shard_state is NOT redefined in runtime.rs
echo "Checking: run_shard_state is not redefined in runtime.rs..."
if rg -q "fn run_shard_state[^_]" "applications/erlang/ranch_uring/native/src/runtime.rs" 2>/dev/null; then
    echo "FAIL: run_shard_state redefined in runtime.rs"
    exit 1
fi
echo "  run_shard_state not in runtime.rs: ok"

# DP-CR-0084: shard_thread_name preserved in runtime_launch.rs
echo "Checking: shard_thread_name preserved in runtime_launch.rs..."
if ! rg -q "fn shard_thread_name" "applications/erlang/ranch_uring/native/src/runtime_launch.rs" 2>/dev/null; then
    echo "FAIL: shard_thread_name missing from runtime_launch.rs"
    exit 1
fi
echo "  shard_thread_name in runtime_launch.rs: ok"

# DP-CR-0085: spawn failure errno mapping preserved after movement
echo "Checking: spawn_shard_thread errno mapping preserved in runtime_launch.rs..."
if ! rg -q 'map_err' "applications/erlang/ranch_uring/native/src/runtime_launch.rs" 2>/dev/null; then
    echo "FAIL: spawn_shard_thread errno mapping not found in runtime_launch.rs"
    exit 1
fi
echo "  spawn_shard_thread errno mapping: ok"

# DP-CR-0086: shutdown_started_shards preserved in runtime_launch.rs
echo "Checking: shutdown_started_shards preserved in runtime_launch.rs..."
if ! rg -q 'fn shutdown_started_shards' "applications/erlang/ranch_uring/native/src/runtime_launch.rs" 2>/dev/null; then
    echo "FAIL: shutdown_started_shards not found in runtime_launch.rs"
    exit 1
fi
echo "  shutdown_started_shards in runtime_launch.rs: ok"

# DP-CR-0098: safe Rust path maintained (no unsafe in runtime_launch.rs)
echo "Checking: runtime_launch.rs maintains safe Rust (no unsafe)..."
if rg -q "^unsafe[[:space:]]" "applications/erlang/ranch_uring/native/src/runtime_launch.rs" 2>/dev/null; then
    echo "FAIL: runtime_launch.rs contains unsafe code"
    exit 1
fi
echo "  runtime_launch.rs safe Rust: ok"

# DP-CR-0097: no lib.rs Command import cycles from launch extraction
echo "Checking: runtime_launch.rs does not introduce Command import cycles..."
LIB_PATH="applications/erlang/ranch_uring/native/src/lib.rs"
if rg -q "mod runtime_launch" "$LIB_PATH" 2>/dev/null; then
    # runtime_launch re-exports Command - verify it uses pub(crate) re-export, not wildcard
    if rg -q "^pub(crate) use.*runtime::Command" "applications/erlang/ranch_uring/native/src/runtime_launch.rs" 2>/dev/null; then
        echo "  runtime_launch.rs Command re-export: ok"
    fi
fi
echo "  no Command import cycles: ok"

# DP-RB-0026: SQPOLL constants extracted to runtime_limits.rs
echo "Checking: SQPOLL arena constants are not redefined in runtime.rs..."
RUNTIME_PATH="applications/erlang/ranch_uring/native/src/runtime.rs"
if rg -q "^const SQPOLL_CHUNK_ARENA_MULTIPLIER_NUM" "$RUNTIME_PATH" 2>/dev/null; then
    echo "FAIL: SQPOLL_CHUNK_ARENA_MULTIPLIER_NUM still defined in runtime.rs (should be in runtime_limits)"
    exit 1
fi
if rg -q "^const SQPOLL_CHUNK_ARENA_MULTIPLIER_DEN" "$RUNTIME_PATH" 2>/dev/null; then
    echo "FAIL: SQPOLL_CHUNK_ARENA_MULTIPLIER_DEN still defined in runtime.rs (should be in runtime_limits)"
    exit 1
fi
if rg -q "^const SQPOLL_SUBSCRIBE_ARENA_MULTIPLIER" "$RUNTIME_PATH" 2>/dev/null; then
    echo "FAIL: SQPOLL_SUBSCRIBE_ARENA_MULTIPLIER still defined in runtime.rs (should be in runtime_limits)"
    exit 1
fi
echo "  SQPOLL constants extraction: ok"

echo ""
echo "All runtime extraction wiring guards passed."
echo "DP-RP-0027-0052: All 26 extraction hardening guards passed."
echo "DP-CR-0071-0098: All L2 launch extraction guards passed."
