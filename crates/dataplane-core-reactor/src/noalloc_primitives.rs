#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct NativeHotTaskBudget(usize);

impl NativeHotTaskBudget {
    #[inline(always)]
    pub const fn new(budget: usize) -> Self {
        Self(budget)
    }

    #[inline(always)]
    pub const fn get(self) -> usize {
        self.0
    }
}

impl Default for NativeHotTaskBudget {
    fn default() -> Self {
        Self::new(0)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct DataPlaneSettings {
    native_hot_task_budget: NativeHotTaskBudget,
}

impl DataPlaneSettings {
    pub const fn new() -> Self {
        Self {
            native_hot_task_budget: NativeHotTaskBudget::new(0),
        }
    }

    #[inline(always)]
    pub const fn native_hot_task_budget(self) -> usize {
        self.native_hot_task_budget.get()
    }

    #[inline(always)]
    pub const fn native_hot_task_budget_value(self) -> NativeHotTaskBudget {
        self.native_hot_task_budget
    }

    #[inline(always)]
    pub const fn with_native_hot_task_budget(mut self, budget: usize) -> Self {
        self.native_hot_task_budget = NativeHotTaskBudget::new(budget);
        self
    }

    #[inline(always)]
    pub const fn with_native_hot_task_budget_value(mut self, budget: NativeHotTaskBudget) -> Self {
        self.native_hot_task_budget = budget;
        self
    }
}

impl Default for DataPlaneSettings {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct WaitTag(u16);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum WaitDomain {
    Remote = 0,
    Local = 1,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
#[repr(u8)]
pub enum LocalWaitKind {
    Runnable = 0,
    Yield = 1,
    Join = 2,
    Timer = 3,
    Io = 4,
    Select = 5,
    Channel = 6,
    Cancelled = 7,
    Extended = 31,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitRef {
    Remote(u16),
    Local { kind: LocalWaitKind, payload: u16 },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WaitDecodeError {
    BadLocalKind,
}

impl LocalWaitKind {
    const fn from_raw(raw: u8) -> Option<Self> {
        match raw {
            0 => Some(Self::Runnable),
            1 => Some(Self::Yield),
            2 => Some(Self::Join),
            3 => Some(Self::Timer),
            4 => Some(Self::Io),
            5 => Some(Self::Select),
            6 => Some(Self::Channel),
            7 => Some(Self::Cancelled),
            31 => Some(Self::Extended),
            _ => None,
        }
    }
}

impl WaitTag {
    pub const DOMAIN_MASK: u16 = 0b1;
    pub const REMOTE_INDEX_SHIFT: u16 = 1;
    pub const REMOTE_INDEX_MASK: u16 = 0x7fff;
    pub const LOCAL_KIND_SHIFT: u16 = 1;
    pub const LOCAL_KIND_MASK: u16 = 0b1_1111;
    pub const LOCAL_PAYLOAD_SHIFT: u16 = 6;
    pub const LOCAL_PAYLOAD_MASK: u16 = 0b11_1111_1111;

    #[inline(always)]
    pub const fn new_remote(index: u16) -> Self {
        Self((index & Self::REMOTE_INDEX_MASK) << Self::REMOTE_INDEX_SHIFT)
    }

    #[inline(always)]
    pub const fn new_local(kind: LocalWaitKind, payload: u16) -> Self {
        Self(
            WaitDomain::Local as u16
                | ((kind as u16) << Self::LOCAL_KIND_SHIFT)
                | ((payload & Self::LOCAL_PAYLOAD_MASK) << Self::LOCAL_PAYLOAD_SHIFT),
        )
    }

    #[inline(always)]
    pub const fn raw(self) -> u16 {
        self.0
    }

    #[inline(always)]
    pub const fn from_raw(raw: u16) -> Self {
        Self(raw)
    }

    #[inline(always)]
    pub const fn domain(self) -> WaitDomain {
        if self.0 & Self::DOMAIN_MASK == 0 {
            WaitDomain::Remote
        } else {
            WaitDomain::Local
        }
    }

    #[inline(always)]
    pub const fn remote_index(self) -> u16 {
        (self.0 >> Self::REMOTE_INDEX_SHIFT) & Self::REMOTE_INDEX_MASK
    }

    #[inline(always)]
    pub const fn local_payload(self) -> u16 {
        (self.0 >> Self::LOCAL_PAYLOAD_SHIFT) & Self::LOCAL_PAYLOAD_MASK
    }

    #[inline(always)]
    pub const fn local_kind(self) -> Result<LocalWaitKind, WaitDecodeError> {
        let raw = ((self.0 >> Self::LOCAL_KIND_SHIFT) & Self::LOCAL_KIND_MASK) as u8;
        match LocalWaitKind::from_raw(raw) {
            Some(kind) => Ok(kind),
            None => Err(WaitDecodeError::BadLocalKind),
        }
    }

    #[inline(always)]
    pub const fn decode(self) -> Result<WaitRef, WaitDecodeError> {
        match self.domain() {
            WaitDomain::Remote => Ok(WaitRef::Remote(self.remote_index())),
            WaitDomain::Local => match self.local_kind() {
                Ok(kind) => Ok(WaitRef::Local {
                    kind,
                    payload: self.local_payload(),
                }),
                Err(err) => Err(err),
            },
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ReplayKind {
    Recv { len: usize },
    Send { bytes: usize },
    Accept { timeout_ms: i64 },
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ScheduledOp {
    pub slot: usize,
    pub kind: ReplayKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PushError<T> {
    BadSlot(T),
    Full(T),
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum CountError {
    BadSlot,
    Full,
    Overflow,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct Ring<T: Copy + Default, const CAP: usize> {
    buf: [T; CAP],
    head: usize,
    len: usize,
}

impl<T: Copy + Default, const CAP: usize> Ring<T, CAP> {
    fn new() -> Self {
        Self {
            buf: core::array::from_fn(|_| T::default()),
            head: 0,
            len: 0,
        }
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn is_full(&self) -> bool {
        self.len == CAP
    }

    fn push(&mut self, item: T) -> Result<(), T> {
        if self.is_full() {
            return Err(item);
        }

        let tail = (self.head + self.len) % CAP;
        self.buf[tail] = item;
        self.len += 1;
        Ok(())
    }

    fn pop(&mut self) -> Option<T> {
        if self.is_empty() {
            return None;
        }

        let item = self.buf[self.head];
        self.head = (self.head + 1) % CAP;
        self.len -= 1;
        Some(item)
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
struct IndexRing<const CAP: usize> {
    buf: [usize; CAP],
    head: usize,
    len: usize,
}

impl<const CAP: usize> IndexRing<CAP> {
    fn new() -> Self {
        Self {
            buf: [0; CAP],
            head: 0,
            len: 0,
        }
    }

    fn is_empty(&self) -> bool {
        self.len == 0
    }

    fn is_full(&self) -> bool {
        self.len == CAP
    }

    fn push(&mut self, item: usize) -> bool {
        if self.is_full() {
            return false;
        }

        let tail = (self.head + self.len) % CAP;
        self.buf[tail] = item;
        self.len += 1;
        true
    }

    fn pop(&mut self) -> Option<usize> {
        if self.is_empty() {
            return None;
        }

        let item = self.buf[self.head];
        self.head = (self.head + 1) % CAP;
        self.len -= 1;
        Some(item)
    }
}

/// Per-slot item queue with a fixed number of slots and per-slot ring capacity.
///
/// # Slot-index invariant
/// Every index stored in `runnable` originates from a caller-supplied `slot`
/// that was already validated by `push` (via `queues.get_mut(slot)`, which
/// succeeds only when `slot < SLOTS`). Therefore every value that `runnable`
/// yields is guaranteed to satisfy `slot < SLOTS`, and direct array indexing
/// of `queues` and `enqueued` inside `drain` is sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedLocalExec<T: Copy + Default, const SLOTS: usize, const CAP: usize> {
    queues: [Ring<T, CAP>; SLOTS],
    runnable: IndexRing<SLOTS>,
    enqueued: [bool; SLOTS],
    pending: usize,
}

impl<T: Copy + Default, const SLOTS: usize, const CAP: usize> FixedLocalExec<T, SLOTS, CAP> {
    pub fn new() -> Self {
        Self {
            queues: core::array::from_fn(|_| Ring::new()),
            runnable: IndexRing::new(),
            enqueued: [false; SLOTS],
            pending: 0,
        }
    }

    pub fn push(&mut self, slot: usize, item: T) -> Result<(), PushError<T>> {
        let queue = self.queues.get_mut(slot).ok_or(PushError::BadSlot(item))?;
        let was_empty = queue.is_empty();
        queue.push(item).map_err(PushError::Full)?;
        self.pending += 1;
        if was_empty && !self.enqueued[slot] {
            self.enqueued[slot] = self.runnable.push(slot);
        }
        Ok(())
    }

    pub fn has_work(&self) -> bool {
        self.pending != 0
    }

    pub fn pending(&self) -> usize {
        self.pending
    }

    pub fn drain<F>(
        &mut self,
        runnable_budget: usize,
        session_run_budget: usize,
        drain_session: bool,
        mut f: F,
    ) -> usize
    where
        F: FnMut(T),
    {
        let mut progressed = 0usize;
        let mut runnable = 0usize;
        while runnable < runnable_budget {
            let Some(slot) = self.runnable.pop() else {
                break;
            };
            self.enqueued[slot] = false;

            let mut ran = 0usize;
            while drain_session || ran < session_run_budget {
                let Some(item) = self.queues[slot].pop() else {
                    break;
                };
                f(item);
                self.pending -= 1;
                progressed += 1;
                ran += 1;
            }

            if !self.queues[slot].is_empty() && !self.enqueued[slot] {
                self.enqueued[slot] = self.runnable.push(slot);
            }
            runnable += 1;
        }
        progressed
    }
}

impl<T: Copy + Default, const SLOTS: usize, const CAP: usize> Default
    for FixedLocalExec<T, SLOTS, CAP>
{
    fn default() -> Self {
        Self::new()
    }
}

/// Per-slot count queue with a fixed number of slots and a per-slot maximum.
///
/// # Slot-index invariant
/// Every index stored in `runnable` originates from a caller-supplied `slot`
/// that was already validated by `push_count` (via `pending_per_slot.get_mut(slot)`,
/// which succeeds only when `slot < SLOTS`). Therefore every value that `runnable`
/// yields is guaranteed to satisfy `slot < SLOTS`, and direct array indexing
/// of `pending_per_slot` and `enqueued` inside `drain` is sound.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedLocalExecCounts<const SLOTS: usize, const MAX_PER_SLOT: usize> {
    pending_per_slot: [usize; SLOTS],
    runnable: IndexRing<SLOTS>,
    enqueued: [bool; SLOTS],
    pending: usize,
}

impl<const SLOTS: usize, const MAX_PER_SLOT: usize> FixedLocalExecCounts<SLOTS, MAX_PER_SLOT> {
    pub fn new() -> Self {
        Self {
            pending_per_slot: [0; SLOTS],
            runnable: IndexRing::new(),
            enqueued: [false; SLOTS],
            pending: 0,
        }
    }

    pub fn push_count(&mut self, slot: usize, count: usize) -> Result<(), CountError> {
        if count == 0 {
            return Ok(());
        }
        let current = self
            .pending_per_slot
            .get_mut(slot)
            .ok_or(CountError::BadSlot)?;
        let next = current.checked_add(count).ok_or(CountError::Overflow)?;
        if next > MAX_PER_SLOT {
            return Err(CountError::Full);
        }
        let total = self
            .pending
            .checked_add(count)
            .ok_or(CountError::Overflow)?;
        let was_empty = *current == 0;
        *current = next;
        self.pending = total;
        if was_empty && !self.enqueued[slot] {
            self.enqueued[slot] = self.runnable.push(slot);
        }
        Ok(())
    }

    pub fn has_work(&self) -> bool {
        self.pending != 0
    }

    pub fn pending(&self) -> usize {
        self.pending
    }

    pub fn drain<F>(
        &mut self,
        runnable_budget: usize,
        session_run_budget: usize,
        drain_session: bool,
        mut f: F,
    ) -> usize
    where
        F: FnMut(),
    {
        let mut progressed = 0usize;
        let mut runnable = 0usize;
        while runnable < runnable_budget {
            let Some(slot) = self.runnable.pop() else {
                break;
            };
            self.enqueued[slot] = false;

            let mut ran = 0usize;
            while drain_session || ran < session_run_budget {
                if self.pending_per_slot[slot] == 0 {
                    break;
                }
                self.pending_per_slot[slot] -= 1;
                self.pending -= 1;
                f();
                progressed += 1;
                ran += 1;
            }

            if self.pending_per_slot[slot] != 0 && !self.enqueued[slot] {
                self.enqueued[slot] = self.runnable.push(slot);
            }
            runnable += 1;
        }
        progressed
    }
}

impl<const SLOTS: usize, const MAX_PER_SLOT: usize> Default
    for FixedLocalExecCounts<SLOTS, MAX_PER_SLOT>
{
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedStepResult {
    pub progressed: usize,
    pub work_remaining: bool,
}

pub fn drive_counts_step<const SLOTS: usize, const MAX_PER_SLOT: usize>(
    counts: &mut FixedLocalExecCounts<SLOTS, MAX_PER_SLOT>,
    runnable_budget: usize,
    session_run_budget: usize,
    drain_session: bool,
) -> FixedStepResult {
    let progressed = counts.drain(runnable_budget, session_run_budget, drain_session, || {});
    FixedStepResult {
        progressed,
        work_remaining: counts.has_work(),
    }
}

// ---------------------------------------------------------------------------
// FixedTaskSlots — fixed-capacity task slot table with generation stale-refuse
// ---------------------------------------------------------------------------

/// Error returned when a slot table has zero capacity.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CapacityError {
    _priv: (),
}

impl CapacityError {
    const fn new() -> Self {
        Self { _priv: () }
    }
}

/// A handle to an inserted task. Carries index and generation so that a
/// `FixedTaskRef` becomes invalid after the slot is released and reused.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct FixedTaskRef {
    index: usize,
    generation: u32,
}

/// Fixed-capacity task slot table with index+generation stale-reference rejection.
///
/// Occupancy is tracked separately from generation. Generation changes on every
/// insertion, and release never resets it, so stale references from prior reuse
/// cycles cannot become valid again.
///
/// # Type parameters
/// - `T` – task value type. Required to be `Default` for slot initialisation and
///   `Clone` so `get` can return an owned clone.
/// - `SLOTS` – compile-time slot count. Zero is a capacity error (no panic).
pub struct FixedTaskSlots<T, const SLOTS: usize>
where
    T: Clone + Default,
{
    /// (task, generation) pairs. Occupancy is held in `occupied`.
    entries: [(T, u32); SLOTS],
    occupied: [bool; SLOTS],
    /// Number of currently occupied slots.
    active: usize,
}

impl<T, const SLOTS: usize> core::fmt::Debug for FixedTaskSlots<T, SLOTS>
where
    T: Clone + Default + core::fmt::Debug,
{
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        f.debug_struct("FixedTaskSlots")
            .field("entries", &self.entries)
            .field("occupied", &self.occupied)
            .field("active", &self.active)
            .finish()
    }
}

impl<T, const SLOTS: usize> FixedTaskSlots<T, SLOTS>
where
    T: Clone + Default,
{
    /// Creates a new slot table. Returns `Err(CapacityError)` when `SLOTS == 0`.
    pub fn new() -> Result<Self, CapacityError> {
        if SLOTS == 0 {
            return Err(CapacityError::new());
        }
        Ok(Self {
            entries: core::array::from_fn(|_| (T::default(), 0)),
            occupied: [false; SLOTS],
            active: 0,
        })
    }

    /// Attempts to insert `task` into a free slot.
    ///
    /// Returns `Ok(FixedTaskRef)` on success; the caller may use the ref to
    /// `get` or `get_mut` the stored value and must call `release` when done.
    ///
    /// Returns `Err(task)` if no free slot is available (no panic).
    pub fn insert(&mut self, task: T) -> Result<FixedTaskRef, T> {
        if self.active >= SLOTS {
            return Err(task);
        }

        let Some(index) = (0..SLOTS).find(|&i| !self.occupied[i]) else {
            return Err(task);
        };

        let entry = &mut self.entries[index];
        let mut next_generation = entry.1.wrapping_add(1);
        if next_generation == 0 {
            next_generation = 1;
        }
        entry.1 = next_generation;
        entry.0 = task;
        self.occupied[index] = true;
        self.active += 1;

        Ok(FixedTaskRef {
            index,
            generation: entry.1,
        })
    }

    /// Returns a clone of the task stored at `ref_`, or `None` if the slot is
    /// stale (generation mismatch) or the index is out of bounds.
    pub fn get(&self, ref_: FixedTaskRef) -> Option<T> {
        let (task, generation) = self.entries.get(ref_.index)?;
        let occupied = self.occupied.get(ref_.index).copied().unwrap_or(false);
        if occupied && *generation == ref_.generation && ref_.generation != 0 {
            Some(task.clone())
        } else {
            None
        }
    }

    /// Returns a mutable reference to the task stored at `ref_`, or `None` if
    /// the slot is stale or the index is out of bounds.
    pub fn get_mut(&mut self, ref_: FixedTaskRef) -> Option<&mut T> {
        let occupied = self.occupied.get(ref_.index).copied().unwrap_or(false);
        let (task, generation) = self.entries.get_mut(ref_.index)?;
        if occupied && *generation == ref_.generation && ref_.generation != 0 {
            Some(task)
        } else {
            None
        }
    }

    /// Marks the slot referenced by `ref_` as free, invalidating that ref.
    ///
    /// Silently ignores requests where the index is out of range or the
    /// generation does not match (e.g. double-release).
    pub fn release(&mut self, ref_: FixedTaskRef) {
        let Some(occupied) = self.occupied.get_mut(ref_.index) else {
            return;
        };
        let Some(entry) = self.entries.get_mut(ref_.index) else {
            return;
        };
        if *occupied && entry.1 == ref_.generation && ref_.generation != 0 {
            *occupied = false;
            entry.0 = T::default();
            self.active -= 1;
        }
    }

    /// Returns the number of currently occupied slots.
    pub fn active_count(&self) -> usize {
        self.active
    }

    /// Returns the total slot capacity.
    pub const fn capacity(&self) -> usize {
        SLOTS
    }
}

#[cfg(test)]
mod tests {
    use super::{
        drive_counts_step, CapacityError, CountError, DataPlaneSettings, FixedLocalExec,
        FixedLocalExecCounts, FixedStepResult, FixedTaskRef, FixedTaskSlots, LocalWaitKind,
        NativeHotTaskBudget, PushError, ReplayKind, ScheduledOp, WaitRef, WaitTag,
    };

    #[derive(Clone, Copy, Default, Debug, PartialEq, Eq)]
    struct Num(u8);

    #[test]
    fn item_queue_drains_in_runnable_order() {
        let mut exec: FixedLocalExec<Num, 2, 3> = FixedLocalExec::new();
        assert_eq!(exec.push(0, Num(10)), Ok(()));
        assert_eq!(exec.push(0, Num(11)), Ok(()));
        assert_eq!(exec.push(1, Num(20)), Ok(()));

        let mut seen = [Num(0); 3];
        let mut idx = 0usize;
        let progressed = exec.drain(usize::MAX, 1, false, |item| {
            seen[idx] = item;
            idx += 1;
        });

        assert_eq!(progressed, 3);
        assert_eq!(seen, [Num(10), Num(20), Num(11)]);
        assert_eq!(exec.pending(), 0);
        assert!(!exec.has_work());
    }

    #[test]
    fn item_queue_returns_item_on_push_failure() {
        let mut exec: FixedLocalExec<Num, 1, 1> = FixedLocalExec::new();
        assert_eq!(exec.push(9, Num(4)), Err(PushError::BadSlot(Num(4))));
        assert_eq!(exec.push(0, Num(4)), Ok(()));
        assert_eq!(exec.push(0, Num(5)), Err(PushError::Full(Num(5))));
    }

    #[test]
    fn item_queue_zero_session_budget_requeues_without_progress() {
        let mut exec: FixedLocalExec<Num, 1, 2> = FixedLocalExec::new();
        assert_eq!(exec.push(0, Num(1)), Ok(()));
        assert_eq!(exec.push(0, Num(2)), Ok(()));

        let progressed = exec.drain(1, 0, false, |_| {});

        assert_eq!(progressed, 0);
        assert_eq!(exec.pending(), 2);
        let mut seen = [Num(0); 2];
        let mut idx = 0usize;
        assert_eq!(
            exec.drain(1, 2, false, |item| {
                seen[idx] = item;
                idx += 1;
            }),
            2
        );
        assert_eq!(seen, [Num(1), Num(2)]);
    }

    #[test]
    fn count_queue_drains_one_callback_per_count() {
        let mut exec: FixedLocalExecCounts<2, 4> = FixedLocalExecCounts::new();
        assert_eq!(exec.push_count(0, 2), Ok(()));
        assert_eq!(exec.push_count(1, 1), Ok(()));
        assert_eq!(exec.push_count(0, 1), Ok(()));
        assert_eq!(exec.pending(), 4);

        let mut calls = 0usize;
        let progressed = exec.drain(usize::MAX, 1, false, || {
            calls += 1;
        });

        assert_eq!(progressed, 4);
        assert_eq!(calls, 4);
        assert_eq!(exec.pending(), 0);
        assert!(!exec.has_work());
    }

    #[test]
    fn count_queue_reports_bad_slot_and_full() {
        let mut exec: FixedLocalExecCounts<1, 2> = FixedLocalExecCounts::new();
        assert_eq!(exec.push_count(3, 1), Err(CountError::BadSlot));
        assert_eq!(exec.push_count(0, 2), Ok(()));
        assert_eq!(exec.push_count(0, 1), Err(CountError::Full));
    }

    #[test]
    fn settings_wait_tag_and_replay_models_round_trip() {
        let budget = NativeHotTaskBudget::new(3);
        let settings = DataPlaneSettings::new().with_native_hot_task_budget_value(budget);
        assert_eq!(settings.native_hot_task_budget(), 3);

        let remote = WaitTag::new_remote(77);
        assert_eq!(remote.decode(), Ok(WaitRef::Remote(77)));

        let local = WaitTag::new_local(LocalWaitKind::Io, 11);
        assert_eq!(
            local.decode(),
            Ok(WaitRef::Local {
                kind: LocalWaitKind::Io,
                payload: 11
            })
        );

        let op = ScheduledOp {
            slot: 2,
            kind: ReplayKind::Send { bytes: 5 },
        };
        assert_eq!(op.slot, 2);
        assert_eq!(op.kind, ReplayKind::Send { bytes: 5 });
    }

    #[test]
    fn count_step_reports_progress_and_remaining_work() {
        let mut exec: FixedLocalExecCounts<1, 4> = FixedLocalExecCounts::new();
        assert_eq!(exec.push_count(0, 3), Ok(()));

        let first = drive_counts_step(&mut exec, 1, 2, false);
        assert_eq!(
            first,
            FixedStepResult {
                progressed: 2,
                work_remaining: true
            }
        );

        let second = drive_counts_step(&mut exec, 1, 2, false);
        assert_eq!(
            second,
            FixedStepResult {
                progressed: 1,
                work_remaining: false
            }
        );
    }

    // ---- FixedTaskSlots tests ----

    #[test]
    fn task_slots_insert_access_mutate_release_active_count() {
        let mut slots: FixedTaskSlots<i32, 4> = FixedTaskSlots::new().unwrap();
        assert_eq!(slots.capacity(), 4);
        assert_eq!(slots.active_count(), 0);

        // Insert three tasks.
        let r0 = slots.insert(10).unwrap();
        let r1 = slots.insert(20).unwrap();
        let r2 = slots.insert(30).unwrap();
        assert_eq!(slots.active_count(), 3);

        // Access via get.
        assert_eq!(slots.get(r0), Some(10));
        assert_eq!(slots.get(r1), Some(20));
        assert_eq!(slots.get(r2), Some(30));

        // Mutate via get_mut.
        let slot_ref = slots.get_mut(r1).unwrap();
        *slot_ref = 99;
        assert_eq!(slots.get(r1), Some(99));

        // Release one slot.
        slots.release(r1);
        assert_eq!(slots.active_count(), 2);
        assert_eq!(slots.get(r1), None); // stale ref now returns None

        // Remaining slots still accessible.
        assert_eq!(slots.get(r0), Some(10));
        assert_eq!(slots.get(r2), Some(30));

        // Release all.
        slots.release(r0);
        slots.release(r2);
        assert_eq!(slots.active_count(), 0);
    }

    #[test]
    fn task_slots_full_returns_original_task() {
        let mut slots: FixedTaskSlots<i32, 2> = FixedTaskSlots::new().unwrap();
        let _r0 = slots.insert(1).unwrap();
        let _r1 = slots.insert(2).unwrap();
        assert_eq!(slots.active_count(), 2);

        // No more room — insert returns the original task via Err.
        let overflow = 99;
        let result = slots.insert(overflow);
        assert!(result.is_err());
        assert_eq!(result.unwrap_err(), 99);
        assert_eq!(slots.active_count(), 2); // unchanged
    }

    #[test]
    fn task_slots_stale_ref_after_release_and_reuse() {
        let mut slots: FixedTaskSlots<i32, 2> = FixedTaskSlots::new().unwrap();
        let r0 = slots.insert(10).unwrap();
        let r1 = slots.insert(20).unwrap();

        // Release slot 0 and re-use it.
        slots.release(r0);
        let r0_prime = slots.insert(999).unwrap(); // reuses index 0 with generation bumped

        // Old ref from first insertion is stale.
        assert_eq!(slots.get(r0), None);
        assert_eq!(slots.get_mut(r0), None);

        // New ref works fine.
        assert_eq!(slots.get(r0_prime), Some(999));
        assert_eq!(slots.get(r1), Some(20)); // r1 unaffected
    }

    #[test]
    fn task_slots_stale_refs_stay_invalid_across_multiple_reuse_cycles() {
        let mut slots: FixedTaskSlots<i32, 1> = FixedTaskSlots::new().unwrap();
        let first = slots.insert(10).unwrap();
        slots.release(first);
        let second = slots.insert(20).unwrap();
        slots.release(second);
        let third = slots.insert(30).unwrap();

        assert_eq!(slots.get(first), None);
        assert_eq!(slots.get(second), None);
        assert_eq!(slots.get(third), Some(30));
    }

    #[test]
    fn task_slots_bad_index_and_bad_generation() {
        let mut slots: FixedTaskSlots<i32, 2> = FixedTaskSlots::new().unwrap();

        // Bad index — a FixedTaskRef constructed directly with out-of-range index.
        let bad_ref = FixedTaskRef {
            index: 99,
            generation: 1,
        };
        assert_eq!(slots.get(bad_ref), None);
        assert_eq!(slots.get_mut(bad_ref), None);

        // Bad generation — generation 0 means "never inserted" and is always rejected.
        let never_inserted = FixedTaskRef {
            index: 0,
            generation: 0,
        };
        assert_eq!(slots.get(never_inserted), None);
        assert_eq!(slots.get_mut(never_inserted), None);
    }

    #[test]
    fn task_slots_zero_capacity_returns_error() {
        // Zero slots must NOT panic; it must return CapacityError.
        let result: Result<FixedTaskSlots<i32, 0>, CapacityError> = FixedTaskSlots::new();
        assert!(result.is_err());
        let err = result.unwrap_err();
        assert_eq!(err, CapacityError::new());
    }

    #[test]
    fn task_slots_parity_with_fixed_local_exec_behavior() {
        // Parity check: FixedTaskSlots and FixedLocalExec both report active
        // counts correctly and both reject bad slot indices.
        let mut exec: FixedLocalExec<i32, 3, 2> = FixedLocalExec::new();
        let mut slots: FixedTaskSlots<i32, 3> = FixedTaskSlots::new().unwrap();

        // Both should accept slot indices < their capacity.
        assert!(exec.push(0, 10).is_ok());
        let _s0 = slots.insert(10).unwrap();
        assert_eq!(slots.active_count(), 1);
        assert_eq!(exec.pending(), 1);

        // Both should reject bad slot indices.
        assert!(matches!(exec.push(99, 20), Err(PushError::BadSlot(20))));
        let bad_ref = FixedTaskRef {
            index: 99,
            generation: 1,
        };
        assert!(slots.get(bad_ref).is_none());

        // Both fill up at their declared limits (FixedLocalExec SLOTS*CAP, FixedTaskSlots SLOTS).
        for i in 1..3 {
            let _ = exec.push(i, i as i32 * 10);
            let _ = slots.insert(i as i32 * 10);
        }
        assert_eq!(slots.active_count(), 3);
        assert_eq!(exec.pending(), 3);

        // FixedTaskSlots rejects past its capacity of 3.
        let overflow_result = slots.insert(999);
        assert!(overflow_result.is_err());
        assert_eq!(overflow_result.unwrap_err(), 999);
    }
}
