//! Differential conformance tests between the no_std twins in
//! `noalloc_primitives` and their host-side counterparts in
//! `dataplane-core-reactor-alloc`. Both families are driven with identical
//! inputs and must produce identical observable behavior; any drift between
//! the twin definitions fails here on a plain x86 `cargo test`, without HIL.
#![cfg(feature = "host-core")]

use dataplane_core_reactor::local_exec as host_exec;
use dataplane_core_reactor::local_exec_counts as host_counts;
use dataplane_core_reactor::noalloc_primitives as fixed;
use dataplane_core_reactor::replay_protocol as host_replay;
use dataplane_core_reactor::settings as host_settings;
use dataplane_core_reactor::wait_tag as host_tag;

use proptest::prelude::*;

const SLOTS: usize = 4;
const CAP: usize = 8;

/// Twin-neutral decode outcome so the two `WaitRef`/`WaitDecodeError` enums
/// can be compared value-by-value.
#[derive(Debug, PartialEq, Eq)]
enum Decoded {
    Remote(u16),
    Local { kind: u8, payload: u16 },
    BadLocalKind,
}

fn decode_fixed(raw: u16) -> Decoded {
    match fixed::WaitTag::from_raw(raw).decode() {
        Ok(fixed::WaitRef::Remote(index)) => Decoded::Remote(index),
        Ok(fixed::WaitRef::Local { kind, payload }) => Decoded::Local {
            kind: kind as u8,
            payload,
        },
        Err(fixed::WaitDecodeError::BadLocalKind) => Decoded::BadLocalKind,
    }
}

fn decode_host(raw: u16) -> Decoded {
    match host_tag::WaitTag::from_raw(raw).decode() {
        Ok(host_tag::WaitRef::Remote(index)) => Decoded::Remote(index),
        Ok(host_tag::WaitRef::Local { kind, payload }) => Decoded::Local {
            kind: kind as u8,
            payload,
        },
        Err(host_tag::WaitDecodeError::BadLocalKind) => Decoded::BadLocalKind,
    }
}

#[test]
fn wait_tag_decode_agrees_for_every_raw_value() {
    for raw in 0..=u16::MAX {
        assert_eq!(decode_fixed(raw), decode_host(raw), "raw={raw:#06x}");
        assert_eq!(
            fixed::WaitTag::from_raw(raw).domain() as u8,
            host_tag::WaitTag::from_raw(raw).domain() as u8,
            "raw={raw:#06x}"
        );
    }
}

/// Constructors are compared over their valid input domain; both twins
/// `debug_assert!` on out-of-mask inputs and mask defensively in release.
#[test]
fn wait_tag_remote_constructor_agrees_for_every_index() {
    for index in 0..=fixed::WaitTag::REMOTE_INDEX_MASK {
        assert_eq!(
            fixed::WaitTag::new_remote(index).raw(),
            host_tag::WaitTag::new_remote(index).raw(),
            "index={index}"
        );
    }
}

#[test]
fn wait_tag_local_constructor_agrees_for_every_kind_and_payload() {
    let kinds = [
        (fixed::LocalWaitKind::Runnable, host_tag::LocalWaitKind::Runnable),
        (fixed::LocalWaitKind::Yield, host_tag::LocalWaitKind::Yield),
        (fixed::LocalWaitKind::Join, host_tag::LocalWaitKind::Join),
        (fixed::LocalWaitKind::Timer, host_tag::LocalWaitKind::Timer),
        (fixed::LocalWaitKind::Io, host_tag::LocalWaitKind::Io),
        (fixed::LocalWaitKind::Select, host_tag::LocalWaitKind::Select),
        (fixed::LocalWaitKind::Channel, host_tag::LocalWaitKind::Channel),
        (
            fixed::LocalWaitKind::Cancelled,
            host_tag::LocalWaitKind::Cancelled,
        ),
        (
            fixed::LocalWaitKind::Extended,
            host_tag::LocalWaitKind::Extended,
        ),
    ];
    for (fixed_kind, host_kind) in kinds {
        for payload in 0..=fixed::WaitTag::LOCAL_PAYLOAD_MASK {
            let fixed_raw = fixed::WaitTag::new_local(fixed_kind, payload).raw();
            let host_raw = host_tag::WaitTag::new_local(host_kind, payload).raw();
            assert_eq!(fixed_raw, host_raw, "kind={fixed_kind:?} payload={payload}");
        }
    }
}

#[test]
fn wait_tag_field_masks_agree() {
    assert_eq!(fixed::WaitTag::DOMAIN_MASK, host_tag::WaitTag::DOMAIN_MASK);
    assert_eq!(
        fixed::WaitTag::REMOTE_INDEX_SHIFT,
        host_tag::WaitTag::REMOTE_INDEX_SHIFT
    );
    assert_eq!(
        fixed::WaitTag::REMOTE_INDEX_MASK,
        host_tag::WaitTag::REMOTE_INDEX_MASK
    );
    assert_eq!(
        fixed::WaitTag::LOCAL_KIND_SHIFT,
        host_tag::WaitTag::LOCAL_KIND_SHIFT
    );
    assert_eq!(
        fixed::WaitTag::LOCAL_KIND_MASK,
        host_tag::WaitTag::LOCAL_KIND_MASK
    );
    assert_eq!(
        fixed::WaitTag::LOCAL_PAYLOAD_SHIFT,
        host_tag::WaitTag::LOCAL_PAYLOAD_SHIFT
    );
    assert_eq!(
        fixed::WaitTag::LOCAL_PAYLOAD_MASK,
        host_tag::WaitTag::LOCAL_PAYLOAD_MASK
    );
}

#[test]
fn settings_budget_roundtrip_agrees() {
    assert_eq!(
        fixed::DataPlaneSettings::new().native_hot_task_budget(),
        host_settings::DataPlaneSettings::new().native_hot_task_budget()
    );
    for budget in [0, 1, 7, usize::MAX] {
        assert_eq!(
            fixed::DataPlaneSettings::new()
                .with_native_hot_task_budget(budget)
                .native_hot_task_budget(),
            host_settings::DataPlaneSettings::new()
                .with_native_hot_task_budget(budget)
                .native_hot_task_budget(),
            "budget={budget}"
        );
        assert_eq!(
            fixed::NativeHotTaskBudget::new(budget).get(),
            host_settings::NativeHotTaskBudget::new(budget).get(),
            "budget={budget}"
        );
    }
}

/// Field-alignment guard: builds the same replay values through both twins
/// and matches them variant-by-variant. Adding, removing, or retyping a
/// field on one side breaks this test until the other side matches.
#[test]
fn replay_protocol_fields_agree() {
    let cases = [
        (
            fixed::ReplayKind::Recv { len: 42 },
            host_replay::ReplayKind::Recv { len: 42 },
        ),
        (
            fixed::ReplayKind::Send { bytes: 7 },
            host_replay::ReplayKind::Send { bytes: 7 },
        ),
        (
            fixed::ReplayKind::Accept { timeout_ms: -1 },
            host_replay::ReplayKind::Accept { timeout_ms: -1 },
        ),
    ];
    for (fixed_kind, host_kind) in cases {
        let matched = match (fixed_kind, host_kind) {
            (
                fixed::ReplayKind::Recv { len: a },
                host_replay::ReplayKind::Recv { len: b },
            ) => a == b,
            (
                fixed::ReplayKind::Send { bytes: a },
                host_replay::ReplayKind::Send { bytes: b },
            ) => a == b,
            (
                fixed::ReplayKind::Accept { timeout_ms: a },
                host_replay::ReplayKind::Accept { timeout_ms: b },
            ) => a == b,
            _ => false,
        };
        assert!(matched, "replay kind variants diverged: {fixed_kind:?}");
        let fixed_op = fixed::ScheduledOp {
            slot: 3,
            kind: fixed_kind,
        };
        let host_op = host_replay::ScheduledOp {
            slot: 3,
            kind: host_kind,
        };
        assert_eq!(fixed_op.slot, host_op.slot);
    }
}

#[derive(Clone, Copy, Debug)]
enum ExecOp {
    Push { slot: usize, seq: u32 },
    Drain {
        runnable_budget: usize,
        session_run_budget: usize,
        drain_session: bool,
    },
}

fn exec_op_strategy() -> impl Strategy<Value = ExecOp> {
    prop_oneof![
        // slot range deliberately exceeds SLOTS so BadSlot paths are compared too
        (0usize..SLOTS + 2, 0u32..1000).prop_map(|(slot, seq)| ExecOp::Push { slot, seq }),
        (0usize..SLOTS + 2, 0usize..CAP + 2, any::<bool>()).prop_map(
            |(runnable_budget, session_run_budget, drain_session)| ExecOp::Drain {
                runnable_budget,
                session_run_budget,
                drain_session,
            }
        ),
    ]
}

const SLOT_TAG_BASE: u32 = 10_000;

fn slot_of(value: u32) -> usize {
    (value / SLOT_TAG_BASE) as usize
}

proptest! {
    /// The host `LocalExec` and no_std `FixedLocalExec` must drain the same
    /// items in the same order, report the same progress/pending counts, and
    /// agree on slot validation for any operation sequence within the fixed
    /// twin's capacity. Values are tagged with their slot so per-slot
    /// occupancy can be tracked exactly across drains.
    #[test]
    fn local_exec_twins_agree_on_operation_sequences(
        ops in proptest::collection::vec(exec_op_strategy(), 0..256)
    ) {
        let mut fixed_exec = fixed::FixedLocalExec::<u32, SLOTS, CAP>::new();
        let mut host_exec = host_exec::LocalExec::<u32>::new(SLOTS);
        let mut per_slot = [0usize; SLOTS];

        for op in ops {
            match op {
                ExecOp::Push { slot, seq } => {
                    // Stay within the fixed twin's per-slot capacity: the
                    // unbounded host twin has no Full error by design, so
                    // only the shared domain is compared.
                    if slot < SLOTS && per_slot[slot] == CAP {
                        continue;
                    }
                    let value = slot as u32 * SLOT_TAG_BASE + seq;
                    let fixed_res = fixed_exec.push(slot, value);
                    let host_res = host_exec.push(slot, value);
                    match (fixed_res, host_res) {
                        (Ok(()), Ok(())) => per_slot[slot] += 1,
                        (
                            Err(fixed::PushError::BadSlot(a)),
                            Err(host_exec::PushError::BadSlot(b)),
                        ) => prop_assert_eq!(a, b),
                        (f, h) => prop_assert!(false, "push diverged: fixed={:?} host={:?}", f, h),
                    }
                }
                ExecOp::Drain { runnable_budget, session_run_budget, drain_session } => {
                    let mut fixed_drained = Vec::new();
                    let mut host_drained = Vec::new();
                    let fixed_progress = fixed_exec.drain(
                        runnable_budget,
                        session_run_budget,
                        drain_session,
                        |value| fixed_drained.push(value),
                    );
                    let host_progress = host_exec.drain(
                        runnable_budget,
                        session_run_budget,
                        drain_session,
                        |value| host_drained.push(value),
                    );
                    prop_assert_eq!(fixed_progress, host_progress);
                    prop_assert_eq!(&fixed_drained, &host_drained);
                    for value in fixed_drained {
                        per_slot[slot_of(value)] -= 1;
                    }
                }
            }
            prop_assert_eq!(fixed_exec.pending(), host_exec.pending());
            prop_assert_eq!(fixed_exec.has_work(), host_exec.has_work());
        }
    }

    /// Same differential harness for the count-based twins. Per-slot
    /// occupancy is tracked via a shadow count so pushes stay within the
    /// fixed twin's `MAX_PER_SLOT`; drains only report totals, so the shadow
    /// is reconciled against the twin-agreed pending counts.
    #[test]
    fn local_exec_counts_twins_agree_on_operation_sequences(
        pushes in proptest::collection::vec(
            (0usize..SLOTS + 2, 0usize..CAP + 2),
            0..64
        ),
        drains in proptest::collection::vec(
            (0usize..SLOTS + 2, 0usize..CAP + 2, any::<bool>()),
            0..16
        )
    ) {
        let mut fixed_counts = fixed::FixedLocalExecCounts::<SLOTS, CAP>::new();
        let mut host = host_counts::LocalExecCounts::new(SLOTS);
        let mut per_slot = [0usize; SLOTS];

        // Interleave: after each push batch segment, run one drain.
        let mut drains = drains.into_iter();
        for (index, (slot, count)) in pushes.into_iter().enumerate() {
            // Skip pushes the bounded twin would reject as Full so both
            // twins stay in the shared domain; BadSlot is still exercised
            // because slot may exceed SLOTS.
            let skip = slot < SLOTS && per_slot[slot] + count > CAP;
            if !skip {
                let fixed_res = fixed_counts.push_count(slot, count);
                let host_res = host.push_count(slot, count);
                match (fixed_res, host_res) {
                    (Ok(()), Ok(())) => {
                        if slot < SLOTS {
                            per_slot[slot] += count;
                        }
                    }
                    (
                        Err(fixed::CountError::BadSlot),
                        Err(host_counts::PushCountError::BadSlot),
                    ) => {}
                    (f, h) => prop_assert!(false, "push diverged: fixed={:?} host={:?}", f, h),
                }
                prop_assert_eq!(fixed_counts.pending(), host.pending());
                prop_assert_eq!(fixed_counts.has_work(), host.has_work());
            }

            if index % 4 == 3 {
                if let Some((runnable_budget, session_run_budget, drain_session)) = drains.next() {
                    let mut fixed_ran = 0usize;
                    let mut host_ran = 0usize;
                    let fixed_progress = fixed_counts.drain(
                        runnable_budget,
                        session_run_budget,
                        drain_session,
                        || fixed_ran += 1,
                    );
                    let host_progress = host.drain(
                        runnable_budget,
                        session_run_budget,
                        drain_session,
                        || host_ran += 1,
                    );
                    prop_assert_eq!(fixed_progress, host_progress);
                    prop_assert_eq!(fixed_ran, host_ran);
                    prop_assert_eq!(fixed_counts.pending(), host.pending());
                    prop_assert_eq!(fixed_counts.has_work(), host.has_work());
                    // Counts drains are anonymous (no per-slot attribution in
                    // the callback), so fully drain both twins to restore an
                    // exactly-known shadow state before continuing.
                    fixed_counts.drain(usize::MAX, 0, true, || {});
                    host.drain(usize::MAX, 0, true, || {});
                    prop_assert_eq!(fixed_counts.pending(), host.pending());
                    prop_assert_eq!(fixed_counts.pending(), 0);
                    per_slot = [0; SLOTS];
                }
            }
        }
    }
}
