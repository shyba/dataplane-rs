//! Compile-surface probe for the allocator-backed RP2040 execution primitives.
//! Uses a deliberately leaking 1 KiB bump allocator on one core with interrupts
//! disabled; it is not a reusable firmware allocator. Checks one item/count drain
//! and leaves the result in a volatile cell before sleeping.
//! Build/check with `--features bare-metal-bin --target thumbv6m-none-eabi`.
#![cfg_attr(target_os = "none", no_std)]
#![cfg_attr(target_os = "none", no_main)]

#[cfg(target_os = "none")]
use core::alloc::{GlobalAlloc, Layout};
#[cfg(target_os = "none")]
use core::cell::UnsafeCell;
#[cfg(target_os = "none")]
use core::future::Future;
#[cfg(target_os = "none")]
use core::pin::Pin;
#[cfg(target_os = "none")]
use core::task::{Context, Poll};
#[cfg(target_os = "none")]
use cortex_m_rt::entry;
#[cfg(target_os = "none")]
use dataplane_runtime::rp2040::{
    future_task::FutureTask, local_exec::LocalExec, local_exec_counts::LocalExecCounts,
};

#[cfg(target_os = "none")]
#[global_allocator]
static ALLOCATOR: BumpAllocator = BumpAllocator::new();

#[cfg(target_os = "none")]
static SMOKE_RESULT: SmokeCell = SmokeCell::new();

#[cfg(target_os = "none")]
struct SmokeCell(UnsafeCell<u32>);

#[cfg(target_os = "none")]
// SAFETY: this is only written from the single `#[entry]` path after startup.
unsafe impl Sync for SmokeCell {}

#[cfg(target_os = "none")]
impl SmokeCell {
    const fn new() -> Self {
        Self(UnsafeCell::new(0))
    }

    fn record(&self, value: u32) {
        // SAFETY: the smoke binary is single-threaded and uses a volatile store
        // only to keep the dataplane exercise visible to the linker/optimizer.
        unsafe { core::ptr::write_volatile(self.0.get(), value) };
    }
}

#[cfg(target_os = "none")]
#[repr(align(8))]
struct Heap([u8; 1024]);

#[cfg(target_os = "none")]
struct BumpAllocator {
    heap: UnsafeCell<Heap>,
    next: UnsafeCell<usize>,
}

#[cfg(target_os = "none")]
// SAFETY: this smoke binary runs on one core, never enables interrupts, and only
// allocates from `main` while proving the RP2040 compile surface. It is not a
// reusable allocator for board firmware.
unsafe impl Sync for BumpAllocator {}

#[cfg(target_os = "none")]
impl BumpAllocator {
    const fn new() -> Self {
        Self {
            heap: UnsafeCell::new(Heap([0; 1024])),
            next: UnsafeCell::new(0),
        }
    }
}

#[cfg(target_os = "none")]
// Minimal single-core bump allocator for the smoke binary. The binary only
// needs enough heap for a few Vec/task allocations and intentionally leaks.
// SAFETY: callers uphold `GlobalAlloc` layout invariants; the allocator returns
// aligned, non-overlapping ranges from a fixed private buffer or null on OOM.
unsafe impl GlobalAlloc for BumpAllocator {
    unsafe fn alloc(&self, layout: Layout) -> *mut u8 {
        // SAFETY: `Sync` is only justified for this single-core smoke binary,
        // so allocation cannot race with another core or interrupt handler here.
        let heap = unsafe { &mut *self.heap.get() };
        // SAFETY: same single-threaded allocator invariant as `heap`.
        let next = unsafe { &mut *self.next.get() };

        let base_ptr = heap.0.as_mut_ptr();
        let base = base_ptr as usize;
        let Some(current) = base.checked_add(*next) else {
            return core::ptr::null_mut();
        };
        let align_mask = layout.align() - 1;
        let Some(aligned_with_mask) = current.checked_add(align_mask) else {
            return core::ptr::null_mut();
        };
        let aligned = aligned_with_mask & !align_mask;
        let Some(end) = aligned.checked_add(layout.size()) else {
            return core::ptr::null_mut();
        };
        let Some(end_offset) = end.checked_sub(base) else {
            return core::ptr::null_mut();
        };
        if end_offset > heap.0.len() {
            core::ptr::null_mut()
        } else {
            *next = end_offset;
            let offset = aligned - base;
            // SAFETY: `offset < heap.0.len()` follows from `end_offset <=
            // heap.0.len()` and `layout` having non-zero size for GlobalAlloc.
            unsafe { base_ptr.add(offset) }
        }
    }

    unsafe fn dealloc(&self, _ptr: *mut u8, _layout: Layout) {}
}

#[cfg(target_os = "none")]
#[panic_handler]
fn panic(_info: &core::panic::PanicInfo<'_>) -> ! {
    loop {
        cortex_m::asm::wfi();
    }
}

#[cfg(target_os = "none")]
struct OneShot(bool);

#[cfg(target_os = "none")]
impl Future for OneShot {
    type Output = u32;

    fn poll(mut self: Pin<&mut Self>, _cx: &mut Context<'_>) -> Poll<Self::Output> {
        if self.0 {
            Poll::Ready(7)
        } else {
            self.0 = true;
            Poll::Pending
        }
    }
}

#[cfg(target_os = "none")]
#[entry]
fn main() -> ! {
    let mut task = FutureTask::new_local(OneShot(false));
    let _ = task.poll_dummy();
    let _ = task.poll_dummy();

    let mut exec = LocalExec::new(1);
    exec.push(0, 1u32).expect("valid smoke slot");
    let mut counts = LocalExecCounts::new(1);
    counts.push_count(0, 1).expect("valid smoke count");

    let mut progressed = 0usize;
    let mut counted = 0usize;
    progressed += exec.drain(1, 1, true, |_| {});
    counted += counts.drain(1, 1, true, || {});

    assert_eq!((progressed, counted), (1, 1));
    SMOKE_RESULT.record((progressed as u32) << 16 | counted as u32);
    let _ = (task, exec, counts);
    loop {
        cortex_m::asm::wfi();
    }
}

#[cfg(not(target_os = "none"))]
fn main() {
    println!("dataplane-rp2040-smoke is only active for target_os=none");
}
