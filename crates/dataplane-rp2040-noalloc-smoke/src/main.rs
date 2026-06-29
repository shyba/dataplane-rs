#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

#[cfg(target_os = "none")]
use cortex_m_rt::entry;
#[cfg(target_os = "none")]
use dataplane_runtime::noalloc::{
    drive_counts_step, DataPlaneSettings, FixedLocalExec, FixedLocalExecCounts, FixedTaskSlots,
    LocalWaitKind, ReplayKind, ScheduledOp, WaitTag,
};

#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        cortex_m::asm::wfi();
    }
}

#[cfg(target_os = "none")]
#[entry]
fn main() -> ! {
    // ------------------------------------------------------------
    // Exercise FixedTaskSlots - fixed-capacity task slot table
    // with generation-based stale-reference rejection
    // ------------------------------------------------------------

    // Create a task slot table with capacity for 4 tasks
    let mut task_slots: FixedTaskSlots<u32, 4> = FixedTaskSlots::new().unwrap();

    // Contract: capacity should be 4
    assert_eq!(task_slots.capacity(), 4);
    assert_eq!(task_slots.active_count(), 0);

    // Insert 3 tasks into the slot table
    let ref0 = task_slots.insert(100).unwrap();
    let ref1 = task_slots.insert(200).unwrap();
    let ref2 = task_slots.insert(300).unwrap();

    // Verify active count increased
    assert_eq!(task_slots.active_count(), 3);

    // Access tasks via references - should return the values
    assert_eq!(task_slots.get(ref0), Some(100));
    assert_eq!(task_slots.get(ref1), Some(200));
    assert_eq!(task_slots.get(ref2), Some(300));

    // Mutable access
    if let Some(val) = task_slots.get_mut(ref1) {
        *val = 250;
    }
    assert_eq!(task_slots.get(ref1), Some(250));

    // ------------------------------------------------------------
    // Exercise full capacity behavior
    // ------------------------------------------------------------

    // Fill the slot table to capacity
    let ref3 = task_slots.insert(400).unwrap();
    assert_eq!(task_slots.active_count(), 4);
    assert_eq!(task_slots.capacity(), 4);

    // Attempting to insert when full returns the task (Err with original value)
    let overflow_task = 999u32;
    let result = task_slots.insert(overflow_task);
    assert!(result.is_err());
    assert_eq!(result.unwrap_err(), 999); // Original task returned

    // Slot table still has 4 active slots
    assert_eq!(task_slots.active_count(), 4);

    // ------------------------------------------------------------
    // Exercise stale reference rejection after release
    // ------------------------------------------------------------

    // Release ref1 (value 250)
    task_slots.release(ref1);
    assert_eq!(task_slots.active_count(), 3);

    // ref1 is now stale - get should return None
    assert_eq!(task_slots.get(ref1), None);
    assert_eq!(task_slots.get_mut(ref1), None);

    // But other refs are still valid
    assert_eq!(task_slots.get(ref0), Some(100));
    assert_eq!(task_slots.get(ref2), Some(300));
    assert_eq!(task_slots.get(ref3), Some(400));

    // ------------------------------------------------------------
    // Exercise slot reuse after release
    // ------------------------------------------------------------

    // Insert into the now-free slot (ref1's slot)
    let ref1_new = task_slots.insert(2500).unwrap();

    // New ref1 has different generation, old ref1 is still stale
    assert_eq!(task_slots.get(ref1), None); // Stale
    assert_eq!(task_slots.get(ref1_new), Some(2500)); // New valid ref

    // Release the old ref1 again (should be silent no-op)
    task_slots.release(ref1);
    assert_eq!(task_slots.get(ref1_new), Some(2500)); // Still valid

    // ------------------------------------------------------------
    // Exercise zero-capacity behavior (no panic)
    // ------------------------------------------------------------

    // Creating a zero-slot table should return CapacityError, not panic
    let zero_slots: Result<FixedTaskSlots<u32, 0>, _> = FixedTaskSlots::new();
    assert!(zero_slots.is_err());

    // ------------------------------------------------------------
    // Exercise FixedLocalExec and FixedLocalExecCounts (keep existing)
    // ------------------------------------------------------------

    let mut items: FixedLocalExec<u8, 2, 4> = FixedLocalExec::new();
    let _ = items.push(0, 1);
    let _ = items.push(0, 2);
    let _ = items.push(1, 3);

    let mut item_total = 0u32;
    let item_progress = items.drain(usize::MAX, 1, false, |item| {
        item_total += item as u32;
    });

    let mut counts: FixedLocalExecCounts<2, 4> = FixedLocalExecCounts::new();
    let _ = counts.push_count(0, 2);
    let _ = counts.push_count(1, 1);

    let mut count_total = 0u32;
    let count_progress = counts.drain(usize::MAX, 1, false, || {
        count_total += 1;
    });

    let mut step_counts: FixedLocalExecCounts<1, 4> = FixedLocalExecCounts::new();
    let _ = step_counts.push_count(0, 2);
    let step = drive_counts_step(&mut step_counts, 1, 1, false);
    let settings = DataPlaneSettings::new().with_native_hot_task_budget(2);
    let wait = WaitTag::new_local(LocalWaitKind::Runnable, 1);
    let op = ScheduledOp {
        slot: 0,
        kind: ReplayKind::Send { bytes: 3 },
    };

    // ------------------------------------------------------------
    // Black-box all computed values to prevent optimization
    // ------------------------------------------------------------
    core::hint::black_box((
        item_progress,
        item_total,
        count_progress,
        count_total,
        step,
        settings,
        wait,
        op,
        // FixedTaskSlots exercise results
        task_slots.capacity(),
        task_slots.active_count(),
        ref0,
        ref2,
        ref3,
        ref1_new,
    ));

    loop {
        cortex_m::asm::wfi();
    }
}

#[cfg(not(target_os = "none"))]
fn main() {
    println!("dataplane-rp2040-noalloc-smoke is only active for target_os=none");
}
