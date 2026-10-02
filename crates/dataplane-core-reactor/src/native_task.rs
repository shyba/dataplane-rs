use arrayvec::ArrayVec;
use std::collections::VecDeque;
use std::fmt;

#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct TaskRef(u64);

impl TaskRef {
    const INDEX_BITS: u64 = 32;
    const INDEX_MASK: u64 = (1u64 << Self::INDEX_BITS) - 1;

    #[inline(always)]
    pub const fn from_raw(raw: u64) -> Self {
        Self(raw)
    }

    #[inline(always)]
    pub const fn new(index: u32, generation: u32) -> Self {
        Self(((generation as u64) << Self::INDEX_BITS) | index as u64)
    }

    #[inline(always)]
    pub const fn into_raw(self) -> u64 {
        self.0
    }

    #[inline(always)]
    pub const fn index(self) -> usize {
        (self.0 & Self::INDEX_MASK) as usize
    }

    #[inline(always)]
    pub const fn generation(self) -> u32 {
        (self.0 >> Self::INDEX_BITS) as u32
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum StepResult {
    Ready,
    Parked,
    Complete,
}

pub trait NativeTask: Sized {
    fn step(&mut self, cx: &mut NativeTaskCx<Self>) -> StepResult;
}

pub struct NativeTaskCapacityError<T> {
    task: T,
    active: usize,
    max_slots: usize,
}

impl<T> fmt::Debug for NativeTaskCapacityError<T> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("NativeTaskCapacityError")
            .field("active", &self.active)
            .field("max_slots", &self.max_slots)
            .finish_non_exhaustive()
    }
}

impl<T> NativeTaskCapacityError<T> {
    #[inline(always)]
    pub fn into_task(self) -> T {
        self.task
    }

    #[inline(always)]
    pub fn active(&self) -> usize {
        self.active
    }

    #[inline(always)]
    pub fn max_slots(&self) -> usize {
        self.max_slots
    }
}

pub struct NativeTaskCx<T> {
    spawned_inline: ArrayVec<T, 8>,
    spilled: Vec<T>,
}

impl<T> NativeTaskCx<T> {
    #[inline(always)]
    pub fn new() -> Self {
        Self {
            spawned_inline: ArrayVec::new(),
            spilled: Vec::new(),
        }
    }

    #[inline(always)]
    pub fn spawn(&mut self, task: T) {
        if let Err(err) = self.spawned_inline.try_push(task) {
            self.spilled.push(err.element());
        }
    }

    #[inline(always)]
    pub fn has_spawned(&self) -> bool {
        !self.spawned_inline.is_empty() || !self.spilled.is_empty()
    }

    #[inline(always)]
    fn drain_spawned<F>(&mut self, mut f: F)
    where
        F: FnMut(T),
    {
        for task in self.spawned_inline.drain(..) {
            f(task);
        }
        for task in self.spilled.drain(..) {
            f(task);
        }
    }
}

impl<T> Default for NativeTaskCx<T> {
    fn default() -> Self {
        Self::new()
    }
}

struct TaskSlot<T> {
    generation: u32,
    next_free: u32,
    queued: bool,
    task: Option<T>,
}

impl<T> TaskSlot<T> {
    #[inline(always)]
    fn vacant(next_free: u32) -> Self {
        Self {
            generation: 0,
            next_free,
            queued: false,
            task: None,
        }
    }

    #[inline(always)]
    fn is_live(&self) -> bool {
        self.task.is_some()
    }

    #[inline(always)]
    fn mark_queued(&mut self) {
        self.queued = true;
    }

    #[inline(always)]
    fn mark_dequeued(&mut self) {
        self.queued = false;
    }

    #[inline(always)]
    fn task_mut(&mut self) -> Option<&mut T> {
        self.task.as_mut()
    }

    #[inline(always)]
    fn install(&mut self, task: T) {
        self.next_free = u32::MAX;
        self.queued = false;
        self.task = Some(task);
    }

    #[inline(always)]
    fn release_to_free_list(&mut self, next_free: u32) {
        self.task = None;
        self.queued = false;
        self.generation = self.generation.wrapping_add(1);
        self.next_free = next_free;
    }
}

pub struct NativeTaskEngine<T> {
    ready: VecDeque<u32>,
    slots: Vec<TaskSlot<T>>,
    free_head: u32,
    active: usize,
    max_slots: Option<usize>,
    deferred_children: VecDeque<T>,
    children_dropped: u64,
    dead_ready_skips: u64,
}

impl<T> NativeTaskEngine<T>
where
    T: NativeTask,
{
    pub fn with_task_capacity(task_capacity: usize) -> Self {
        let task_capacity = task_capacity.max(1);
        Self::with_task_capacity_internal(task_capacity, None)
    }

    /// Preallocate up to `task_capacity` slots without exceeding the hard limit.
    /// A zero `max_slots` creates an engine that rejects all admission.
    pub fn with_task_capacity_limit(task_capacity: usize, max_slots: usize) -> Self {
        let task_capacity = task_capacity.max(1).min(max_slots);
        Self::with_task_capacity_internal(task_capacity, Some(max_slots))
    }

    fn with_task_capacity_internal(task_capacity: usize, max_slots: Option<usize>) -> Self {
        let mut slots = Vec::with_capacity(task_capacity);
        let mut idx = 0usize;
        while idx < task_capacity {
            let next = if idx + 1 < task_capacity {
                (idx + 1) as u32
            } else {
                u32::MAX
            };
            slots.push(TaskSlot::vacant(next));
            idx += 1;
        }
        Self {
            ready: VecDeque::with_capacity(task_capacity),
            slots,
            free_head: if task_capacity == 0 { u32::MAX } else { 0 },
            active: 0,
            max_slots,
            deferred_children: VecDeque::new(),
            children_dropped: 0,
            dead_ready_skips: 0,
        }
    }

    #[inline(always)]
    pub fn try_spawn(&mut self, task: T) -> Result<TaskRef, NativeTaskCapacityError<T>> {
        let idx = self.alloc_slot(task)?;
        let task_ref = TaskRef::new(idx as u32, self.slots[idx].generation);
        self.ready.push_back(idx as u32);
        self.slots[idx].mark_queued();
        self.active += 1;
        Ok(task_ref)
    }

    /// Spawn a task, panicking if the engine is at capacity.
    ///
    /// This is an ergonomic wrapper for contexts where capacity is statically known
    /// (tests, fixed-size setup). Production paths that can hit admission saturation
    /// must call [`try_spawn`](Self::try_spawn) and handle
    /// [`NativeTaskCapacityError`] instead — saturation is normal backpressure, not a
    /// fatal condition.
    ///
    /// # Panics
    /// Panics if all task slots are occupied. `#[track_caller]` attributes the panic
    /// to the caller.
    #[inline(always)]
    #[track_caller]
    pub fn spawn(&mut self, task: T) -> TaskRef {
        match self.try_spawn(task) {
            Ok(task_ref) => task_ref,
            Err(err) => panic!(
                "native task capacity exceeded; use try_spawn to handle saturation (active={}, max_slots={})",
                err.active(),
                err.max_slots()
            ),
        }
    }

    #[inline(always)]
    pub fn run_until_idle(&mut self) -> usize {
        self.run_budget(usize::MAX)
    }

    #[inline(always)]
    pub fn run_hot_until_idle<const HOT: usize>(&mut self) -> usize {
        self.run_hot_budget::<HOT>(usize::MAX)
    }

    #[inline(always)]
    pub fn run_hot_budget_dynamic(&mut self, hot: usize, budget: usize) -> usize {
        match round_hot_budget(hot) {
            Some(1) => self.run_hot_budget::<1>(budget),
            Some(2) => self.run_hot_budget::<2>(budget),
            Some(4) => self.run_hot_budget::<4>(budget),
            Some(8) => self.run_hot_budget::<8>(budget),
            Some(16) => self.run_hot_budget::<16>(budget),
            Some(32) => self.run_hot_budget::<32>(budget),
            Some(64) => self.run_hot_budget::<64>(budget),
            Some(128) => self.run_hot_budget::<128>(budget),
            _ => self.run_budget(budget),
        }
    }

    #[inline(always)]
    pub fn run_hot_until_idle_dynamic(&mut self, hot: usize) -> usize {
        self.run_hot_budget_dynamic(hot, usize::MAX)
    }

    pub fn run_budget(&mut self, budget: usize) -> usize {
        if budget == 0 {
            return 0;
        }
        if self.ready.len() <= 4 {
            return self.run_tiny_frontier_budget::<4>(budget);
        }

        let mut progressed = 0usize;
        let mut cx = NativeTaskCx::new();
        while progressed < budget {
            let Some(task_idx) = self.take_live_ready_front() else {
                break;
            };
            let idx = task_idx as usize;
            let task = match self.slots[idx].task_mut() {
                Some(task) => task,
                None => {
                    debug_assert!(false, "task slot vanished after presence check");
                    continue;
                }
            };
            let step = task.step(&mut cx);
            if cx.has_spawned() {
                cx.drain_spawned(|child| {
                    self.spawn_child(child);
                });
            }

            match step {
                StepResult::Ready => {
                    self.requeue_ready_back(task_idx);
                }
                StepResult::Parked => {}
                StepResult::Complete => {
                    self.release_slot(idx);
                }
            }
            progressed += 1;
        }
        progressed
    }

    #[inline(always)]
    fn run_tiny_frontier_budget<const N: usize>(&mut self, budget: usize) -> usize {
        if budget == 0 {
            return 0;
        }

        let mut local: ArrayVec<u32, N> = ArrayVec::new();
        while local.len() < N {
            let Some(task_idx) = self.take_live_ready_front() else {
                break;
            };
            local.push(task_idx);
        }
        if local.is_empty() {
            return 0;
        }

        let mut progressed = 0usize;
        let mut cx = NativeTaskCx::new();
        let mut fallback = false;

        'outer: while progressed < budget && !local.is_empty() {
            let mut i = 0usize;
            while i < local.len() && progressed < budget {
                let idx = local[i] as usize;
                if idx >= self.slots.len() || !self.slots[idx].is_live() {
                    local.swap_remove(i);
                    continue;
                }

                let task = match self.slots[idx].task_mut() {
                    Some(task) => task,
                    None => {
                        local.swap_remove(i);
                        debug_assert!(false, "task slot vanished after presence check");
                        continue;
                    }
                };
                let step = task.step(&mut cx);
                let spawned = cx.has_spawned();
                if spawned {
                    cx.drain_spawned(|child| {
                        self.spawn_child(child);
                    });
                }

                match step {
                    StepResult::Ready => {
                        i += 1;
                    }
                    StepResult::Parked => {
                        local.swap_remove(i);
                    }
                    StepResult::Complete => {
                        self.release_slot(idx);
                        local.swap_remove(i);
                    }
                }
                progressed += 1;

                if spawned || !self.ready.is_empty() {
                    fallback = true;
                    break 'outer;
                }
            }
        }

        while let Some(task_idx) = local.pop() {
            self.requeue_ready_front(task_idx);
        }

        if fallback && progressed < budget {
            progressed += self.run_budget(budget - progressed);
        }

        progressed
    }

    #[inline(always)]
    fn run_hot_budget_one(&mut self, budget: usize) -> usize {
        if budget == 0 {
            return 0;
        }
        if self.ready.len() <= 4 {
            return self.run_hot_budget::<4>(budget);
        }

        let mut progressed = 0usize;
        let mut cx = NativeTaskCx::new();
        let mut hot: Option<u32> = None;

        while progressed < budget {
            if hot.is_none() {
                let Some(task_idx) = self.take_live_ready_front() else {
                    break;
                };
                hot = Some(task_idx);
            }

            let task_idx = match hot {
                Some(task_idx) => task_idx,
                None => continue,
            };
            let idx = task_idx as usize;
            if idx >= self.slots.len() || !self.slots[idx].is_live() {
                hot = None;
                continue;
            }

            let task = match self.slots[idx].task_mut() {
                Some(task) => task,
                None => {
                    hot = None;
                    debug_assert!(false, "task slot vanished after presence check");
                    continue;
                }
            };
            let step = task.step(&mut cx);
            if cx.has_spawned() {
                cx.drain_spawned(|child| {
                    self.spawn_child(child);
                });
            }

            match step {
                StepResult::Ready => {}
                StepResult::Parked => {
                    hot = None;
                }
                StepResult::Complete => {
                    self.release_slot(idx);
                    hot = None;
                }
            }
            progressed += 1;
        }

        if let Some(task_idx) = hot {
            self.requeue_ready_front(task_idx);
        }

        progressed
    }

    pub fn run_hot_budget<const HOT: usize>(&mut self, budget: usize) -> usize {
        if HOT == 1 {
            return self.run_hot_budget_one(budget);
        }
        if budget == 0 || HOT == 0 {
            return 0;
        }

        let mut progressed = 0usize;
        let mut cx = NativeTaskCx::new();
        let mut hot: ArrayVec<u32, HOT> = ArrayVec::new();

        while progressed < budget {
            while hot.len() < HOT {
                let Some(task_idx) = self.take_live_ready_front() else {
                    break;
                };
                hot.push(task_idx);
            }

            if hot.is_empty() {
                break;
            }

            let mut i = 0usize;
            while i < hot.len() && progressed < budget {
                let idx = hot[i] as usize;
                if idx >= self.slots.len() || !self.slots[idx].is_live() {
                    hot.swap_remove(i);
                    continue;
                }
                let task = match self.slots[idx].task_mut() {
                    Some(task) => task,
                    None => {
                        hot.swap_remove(i);
                        debug_assert!(false, "task slot vanished after presence check");
                        continue;
                    }
                };
                let step = task.step(&mut cx);
                if cx.has_spawned() {
                    cx.drain_spawned(|child| {
                        self.spawn_child(child);
                    });
                }

                match step {
                    StepResult::Ready => {
                        i += 1;
                    }
                    StepResult::Parked => {
                        hot.swap_remove(i);
                    }
                    StepResult::Complete => {
                        self.release_slot(idx);
                        hot.swap_remove(i);
                    }
                }
                progressed += 1;
            }
        }

        for task_idx in hot {
            self.requeue_ready_front(task_idx);
        }

        progressed
    }

    #[inline(always)]
    pub fn active_tasks(&self) -> usize {
        self.active
    }

    #[inline(always)]
    /// Requeues a parked task for execution. Returns false when the ref is
    /// stale (slot dead or generation mismatch) or the task is already queued.
    pub fn wake(&mut self, task: TaskRef) -> bool {
        let idx = task.index();
        let Some(slot) = self.slots.get_mut(idx) else {
            return false;
        };
        if !slot.is_live() || slot.generation != task.generation() || slot.queued {
            return false;
        }
        slot.mark_queued();
        self.ready.push_back(idx as u32);
        true
    }

    pub fn ready_len(&self) -> usize {
        self.ready.len()
    }

    #[inline(always)]
    fn take_live_ready_front(&mut self) -> Option<u32> {
        loop {
            let task_idx = self.ready.pop_front()?;
            let idx = task_idx as usize;
            if idx >= self.slots.len() || !self.slots[idx].is_live() {
                self.dead_ready_skips = self.dead_ready_skips.saturating_add(1);
                continue;
            }
            self.slots[idx].mark_dequeued();
            return Some(task_idx);
        }
    }

    #[inline(always)]
    fn requeue_ready_back(&mut self, task_idx: u32) {
        let idx = task_idx as usize;
        if idx < self.slots.len() && self.slots[idx].is_live() {
            self.ready.push_back(task_idx);
            self.slots[idx].mark_queued();
        }
    }

    #[inline(always)]
    fn requeue_ready_front(&mut self, task_idx: u32) {
        let idx = task_idx as usize;
        if idx < self.slots.len() && self.slots[idx].is_live() {
            self.slots[idx].mark_queued();
            self.ready.push_front(task_idx);
        }
    }

    fn alloc_slot(&mut self, task: T) -> Result<usize, NativeTaskCapacityError<T>> {
        if self.free_head == u32::MAX && !self.grow() {
            return Err(NativeTaskCapacityError {
                task,
                active: self.active,
                max_slots: self.max_slots.unwrap_or(self.slots.len()),
            });
        }
        let idx = self.free_head as usize;
        let slot = &mut self.slots[idx];
        self.free_head = slot.next_free;
        slot.install(task);
        Ok(idx)
    }

    fn release_slot(&mut self, idx: usize) {
        let slot = &mut self.slots[idx];
        slot.release_to_free_list(self.free_head);
        self.free_head = idx as u32;
        self.active = self.active.saturating_sub(1);
        if let Some(child) = self.deferred_children.pop_front() {
            if let Err(err) = self.try_spawn(child) {
                self.deferred_children.push_front(err.into_task());
            }
        }
    }

    fn grow(&mut self) -> bool {
        let old_len = self.slots.len();
        let mut new_len = old_len.saturating_mul(2).max(1);
        if let Some(max_slots) = self.max_slots {
            if old_len >= max_slots {
                return false;
            }
            new_len = new_len.min(max_slots);
        }
        self.slots.reserve(new_len - old_len);
        let mut idx = old_len;
        while idx < new_len {
            let next = if idx + 1 < new_len {
                (idx + 1) as u32
            } else {
                self.free_head
            };
            self.slots.push(TaskSlot::vacant(next));
            idx += 1;
        }
        self.free_head = old_len as u32;
        true
    }

    /// Spawns a child task, deferring it when the slot cap is reached.
    ///
    /// Deferred children are installed as slots free up (`release_slot`).
    /// The deferred queue is itself bounded by `max_slots`; beyond that
    /// children are dropped and counted in `children_dropped` instead of
    /// aborting the reactor.
    #[inline(always)]
    fn spawn_child(&mut self, task: T) {
        let Err(err) = self.try_spawn(task) else {
            return;
        };
        let deferred_cap = self.max_slots.unwrap_or(usize::MAX);
        if self.deferred_children.len() < deferred_cap {
            self.deferred_children.push_back(err.into_task());
        } else {
            self.children_dropped = self.children_dropped.saturating_add(1);
        }
    }

    /// Number of children waiting for a free slot.
    #[inline(always)]
    pub fn deferred_children(&self) -> usize {
        self.deferred_children.len()
    }

    /// Children dropped because both the slots and the deferred queue were full.
    #[inline(always)]
    pub fn children_dropped(&self) -> u64 {
        self.children_dropped
    }

    /// Ready-queue entries skipped because their task slot was already dead.
    #[inline(always)]
    pub fn dead_ready_skips(&self) -> u64 {
        self.dead_ready_skips
    }
}

impl<T> Default for NativeTaskEngine<T>
where
    T: NativeTask,
{
    fn default() -> Self {
        Self::with_task_capacity(4096)
    }
}

fn round_hot_budget(hot: usize) -> Option<usize> {
    if hot == 0 {
        return None;
    }
    let rounded = hot.checked_next_power_of_two()?;
    if rounded <= 128 {
        Some(rounded)
    } else {
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[derive(Clone, Copy)]
    struct CounterTask {
        remaining: usize,
    }

    impl NativeTask for CounterTask {
        fn step(&mut self, _cx: &mut NativeTaskCx<Self>) -> StepResult {
            if self.remaining == 0 {
                StepResult::Complete
            } else {
                self.remaining -= 1;
                StepResult::Ready
            }
        }
    }

    #[derive(Clone, Copy)]
    struct SpawnerTask {
        spawn: usize,
    }

    impl NativeTask for SpawnerTask {
        fn step(&mut self, cx: &mut NativeTaskCx<Self>) -> StepResult {
            let mut i = 0usize;
            while i < self.spawn {
                cx.spawn(Self { spawn: 0 });
                i += 1;
            }
            StepResult::Complete
        }
    }

    #[derive(Clone, Copy)]
    struct ParkingSpawnerTask;

    impl NativeTask for ParkingSpawnerTask {
        fn step(&mut self, cx: &mut NativeTaskCx<Self>) -> StepResult {
            cx.spawn(Self);
            StepResult::Parked
        }
    }

    #[test]
    fn pending_tasks_requeue_until_complete() {
        let mut engine = NativeTaskEngine::with_task_capacity(4);
        engine.spawn(CounterTask { remaining: 3 });
        let progressed = engine.run_until_idle();
        assert_eq!(progressed, 4);
        assert_eq!(engine.active_tasks(), 0);
        assert_eq!(engine.ready_len(), 0);
    }

    #[test]
    fn hard_limit_wins_over_preallocation_hint_including_zero() {
        let mut zero = NativeTaskEngine::with_task_capacity_limit(8, 0);
        let error = zero.try_spawn(CounterTask { remaining: 7 }).unwrap_err();
        assert_eq!(error.max_slots(), 0);
        assert_eq!(error.into_task().remaining, 7);
        assert_eq!(zero.run_until_idle(), 0);

        let mut one = NativeTaskEngine::with_task_capacity_limit(8, 1);
        assert!(one.try_spawn(CounterTask { remaining: 1 }).is_ok());
        assert!(one.try_spawn(CounterTask { remaining: 1 }).is_err());
    }

    #[test]
    fn try_spawn_returns_capacity_error_at_saturation() {
        let mut engine = NativeTaskEngine::with_task_capacity_limit(1, 1);
        assert!(engine.try_spawn(CounterTask { remaining: 1 }).is_ok());

        let err = engine
            .try_spawn(CounterTask { remaining: 1 })
            .expect_err("second spawn must be rejected");
        assert_eq!(err.active(), 1);
        assert_eq!(err.max_slots(), 1);
    }

    #[test]
    #[should_panic(expected = "native task capacity exceeded")]
    fn spawn_panics_at_saturation() {
        let mut engine = NativeTaskEngine::with_task_capacity_limit(1, 1);
        engine.spawn(CounterTask { remaining: 1 });
        engine.spawn(CounterTask { remaining: 1 });
    }

    #[test]
    fn task_can_spawn_children() {
        let mut engine = NativeTaskEngine::with_task_capacity(4);
        engine.spawn(SpawnerTask { spawn: 3 });
        let progressed = engine.run_until_idle();
        assert_eq!(progressed, 4);
        assert_eq!(engine.active_tasks(), 0);
    }

    #[test]
    fn slot_lifecycle_helpers_preserve_generation_and_queue_state() {
        let mut slot = TaskSlot::vacant(7);
        assert!(!slot.is_live());
        assert!(!slot.queued);
        assert_eq!(slot.next_free, 7);

        slot.install(CounterTask { remaining: 0 });
        assert!(slot.is_live());
        assert!(!slot.queued);
        assert_eq!(slot.next_free, u32::MAX);

        slot.mark_queued();
        assert!(slot.queued);
        slot.mark_dequeued();
        assert!(!slot.queued);

        slot.release_to_free_list(11);
        assert!(!slot.is_live());
        assert!(!slot.queued);
        assert_eq!(slot.generation, 1);
        assert_eq!(slot.next_free, 11);
    }

    #[test]
    fn ready_queue_helpers_skip_dead_slots_and_preserve_live_requeue() {
        let mut engine = NativeTaskEngine::with_task_capacity(2);
        let first = engine.spawn(CounterTask { remaining: 0 });
        let second = engine.spawn(CounterTask { remaining: 1 });

        engine.release_slot(first.index());

        let task_idx = engine
            .take_live_ready_front()
            .expect("live task should remain available");
        assert_eq!(task_idx as usize, second.index());
        assert_eq!(engine.ready_len(), 0);

        engine.requeue_ready_back(task_idx);
        assert_eq!(engine.ready_len(), 1);

        let task_idx = engine
            .take_live_ready_front()
            .expect("requeued live task should be available");
        engine.requeue_ready_front(task_idx);
        assert_eq!(engine.ready.front().copied(), Some(task_idx));
    }

    #[test]
    fn task_ref_roundtrips_raw_index_and_generation() {
        let raw = TaskRef::new(u32::MAX, 0x1234_5678).into_raw();
        let task_ref = TaskRef::from_raw(raw);

        assert_eq!(task_ref.index(), u32::MAX as usize);
        assert_eq!(task_ref.generation(), 0x1234_5678);
        assert_eq!(task_ref.into_raw(), raw);
    }

    #[test]
    fn hot_budget_rounds_or_falls_back_to_plain_budget() {
        let mut rounded_to_four = NativeTaskEngine::with_task_capacity(4);
        rounded_to_four.spawn(CounterTask { remaining: 2 });
        rounded_to_four.spawn(CounterTask { remaining: 1 });
        let rounded_progress = rounded_to_four.run_hot_budget_dynamic(3, 3);

        let mut direct_four = NativeTaskEngine::with_task_capacity(4);
        direct_four.spawn(CounterTask { remaining: 2 });
        direct_four.spawn(CounterTask { remaining: 1 });
        let direct_progress = direct_four.run_hot_budget::<4>(3);

        assert_eq!(rounded_progress, direct_progress);
        assert_eq!(rounded_to_four.active_tasks(), direct_four.active_tasks());
        assert_eq!(rounded_to_four.ready_len(), direct_four.ready_len());

        let mut rounded_to_eight = NativeTaskEngine::with_task_capacity(4);
        rounded_to_eight.spawn(CounterTask { remaining: 2 });
        rounded_to_eight.spawn(CounterTask { remaining: 1 });
        let rounded_progress = rounded_to_eight.run_hot_budget_dynamic(5, 3);

        let mut direct_eight = NativeTaskEngine::with_task_capacity(4);
        direct_eight.spawn(CounterTask { remaining: 2 });
        direct_eight.spawn(CounterTask { remaining: 1 });
        let direct_progress = direct_eight.run_hot_budget::<8>(3);

        assert_eq!(rounded_progress, direct_progress);
        assert_eq!(rounded_to_eight.active_tasks(), direct_eight.active_tasks());
        assert_eq!(rounded_to_eight.ready_len(), direct_eight.ready_len());

        let mut unsupported_budget_direct = NativeTaskEngine::with_task_capacity(4);
        unsupported_budget_direct.spawn(CounterTask { remaining: 2 });
        unsupported_budget_direct.spawn(CounterTask { remaining: 1 });
        let unsupported_budget_direct_progress = unsupported_budget_direct.run_budget(3);

        let mut unsupported_dynamic = NativeTaskEngine::with_task_capacity(4);
        unsupported_dynamic.spawn(CounterTask { remaining: 2 });
        unsupported_dynamic.spawn(CounterTask { remaining: 1 });
        let unsupported_dynamic_progress = unsupported_dynamic.run_hot_budget_dynamic(129, 3);

        assert_eq!(
            unsupported_dynamic_progress,
            unsupported_budget_direct_progress
        );
        assert_eq!(
            unsupported_dynamic.active_tasks(),
            unsupported_budget_direct.active_tasks()
        );
        assert_eq!(
            unsupported_dynamic.ready_len(),
            unsupported_budget_direct.ready_len()
        );

        let mut oversized_budget_direct = NativeTaskEngine::with_task_capacity(4);
        oversized_budget_direct.spawn(CounterTask { remaining: 2 });
        oversized_budget_direct.spawn(CounterTask { remaining: 1 });
        let oversized_budget_direct_progress = oversized_budget_direct.run_budget(3);

        let mut oversized_dynamic = NativeTaskEngine::with_task_capacity(4);
        oversized_dynamic.spawn(CounterTask { remaining: 2 });
        oversized_dynamic.spawn(CounterTask { remaining: 1 });
        let oversized_dynamic_progress = oversized_dynamic.run_hot_budget_dynamic(usize::MAX, 3);

        assert_eq!(oversized_dynamic_progress, oversized_budget_direct_progress);
        assert_eq!(
            oversized_dynamic.active_tasks(),
            oversized_budget_direct.active_tasks()
        );
        assert_eq!(
            oversized_dynamic.ready_len(),
            oversized_budget_direct.ready_len()
        );
    }

    #[test]
    fn try_spawn_returns_capacity_error_at_configured_limit() {
        let mut engine = NativeTaskEngine::with_task_capacity_limit(2, 2);
        assert!(engine.try_spawn(CounterTask { remaining: 1 }).is_ok());
        assert!(engine.try_spawn(CounterTask { remaining: 1 }).is_ok());

        let err = engine
            .try_spawn(CounterTask { remaining: 1 })
            .expect_err("spawn beyond cap should fail");
        let task = err.into_task();
        assert_eq!(task.remaining, 1);
        assert_eq!(engine.active_tasks(), 2);
    }

    #[test]
    fn hosted_task_capacity_grows_without_default_hard_cap() {
        const TASKS: usize = 65_537;
        let mut engine = NativeTaskEngine::with_task_capacity(1);
        for _ in 0..TASKS {
            engine.spawn(CounterTask { remaining: 0 });
        }
        assert_eq!(engine.active_tasks(), TASKS);
    }

    #[test]
    fn bounded_child_spawn_saturation_defers_instead_of_panicking() {
        let mut engine = NativeTaskEngine::with_task_capacity_limit(1, 1);
        engine.spawn(ParkingSpawnerTask);
        let _ = engine.run_budget(1);
        assert_eq!(engine.active_tasks(), 1);
        assert_eq!(engine.deferred_children(), 1);
        assert_eq!(engine.children_dropped(), 0);
    }

    #[test]
    fn deferred_child_installs_when_slot_frees() {
        let mut engine = NativeTaskEngine::with_task_capacity_limit(1, 1);
        engine.spawn(SpawnerTask { spawn: 1 });
        let _ = engine.run_budget(4);
        assert_eq!(engine.deferred_children(), 0);
        assert_eq!(engine.children_dropped(), 0);
    }

    #[test]
    fn deferred_queue_overflow_drops_and_counts() {
        let mut engine = NativeTaskEngine::with_task_capacity_limit(1, 1);
        engine.spawn(ParkingSpawnerTask);
        let _ = engine.run_budget(1);
        engine.spawn_child(ParkingSpawnerTask);
        assert_eq!(engine.deferred_children(), 1);
        assert_eq!(engine.children_dropped(), 1);
    }
}
