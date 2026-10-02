//! High-level entry point for the dataplane runtime.
//!
//! This crate wires the concrete reactor backends from `dataplane-reactor`
//! (io_uring with syscall fallback) into the profiled runtime from
//! `dataplane-runtime`, so ordinary consumers never name driver or
//! store generics. The advanced/core API remains available through
//! `dataplane_runtime::runtime_profiles` for callers that need it.
//!
//! ```no_run
//! use dataplane::{ProfileKind, Runtime};
//! use dataplane::task::{NativeTask, NativeTaskCx, StepResult};
//!
//! struct Hello;
//!
//! impl NativeTask for Hello {
//!     fn step(&mut self, _cx: &mut NativeTaskCx<Self>) -> StepResult {
//!         println!("hello from a dataplane task");
//!         StepResult::Complete
//!     }
//! }
//!
//! let mut runtime = Runtime::<Hello>::builder()
//!     .profile(ProfileKind::Balanced)
//!     .build()
//!     .expect("build runtime");
//! runtime.spawn(Hello).expect("spawn");
//! runtime.run().expect("run to completion");
//! ```
#![forbid(unsafe_code)]
#![warn(missing_docs)]

use std::io;
use std::time::Instant;

use dataplane_core_reactor::balanced_profile::BalancedHostAction;
use dataplane_core_reactor::native_task::{NativeTask, NativeTaskCapacityError, TaskRef};
use dataplane_core_reactor::wake_handle::WakeHandle;
use dataplane_reactor::reactor::adaptive::UnifiedReactor;
use dataplane_runtime::runtime_profiles::{build_profiled_runtime, ProfiledRuntime};

pub use dataplane_reactor::reactor::adaptive::ReactorBackend;
pub use dataplane_topology::ProfileKind;

/// Task vocabulary re-exported for consumers: implement [`task::NativeTask`]
/// and hand instances to [`Runtime::spawn`].
pub mod task {
    pub use dataplane_core_reactor::native_task::{
        NativeTask, NativeTaskCapacityError, NativeTaskCx, StepResult, TaskRef,
    };
}

/// Network operation vocabulary for tasks that submit reactor I/O.
pub mod net {
    pub use dataplane_reactor::reactor::{NetEvent, NetOp, NetOpKind, OpToken, RawNetOp};
}

/// Errors surfaced while building or driving a [`Runtime`].
#[derive(Debug)]
pub enum RuntimeError {
    /// The reactor backend could not be created (ring setup, fd limits, …).
    Reactor(io::Error),
    /// The requested profile could not produce a valid layout.
    Profile(dataplane_runtime::runtime_profiles::BalancedProfileError),
    /// Driving the runtime failed with a reactor-level I/O error.
    Io(io::Error),
    /// Tasks are alive but none can ever run again: all remaining tasks are
    /// parked with no outstanding I/O and no armed timer to wake them.
    Stalled {
        /// Alive-but-parked tasks at the moment the stall was detected.
        parked_tasks: usize,
    },
}

impl core::fmt::Display for RuntimeError {
    fn fmt(&self, f: &mut core::fmt::Formatter<'_>) -> core::fmt::Result {
        match self {
            Self::Reactor(err) => write!(f, "reactor backend setup failed: {err}"),
            Self::Profile(err) => write!(f, "profile layout failed: {err:?}"),
            Self::Io(err) => write!(f, "runtime I/O failed: {err}"),
            Self::Stalled { parked_tasks } => write!(
                f,
                "runtime stalled: {parked_tasks} parked task(s) with no I/O or timer to wake them"
            ),
        }
    }
}

impl std::error::Error for RuntimeError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            Self::Reactor(error) | Self::Io(error) => Some(error),
            Self::Profile(_) | Self::Stalled { .. } => None,
        }
    }
}

/// Builder for [`Runtime`]. Obtain via [`Runtime::builder`].
pub struct RuntimeBuilder<T>
where
    T: NativeTask,
{
    profile: ProfileKind,
    backend: ReactorBackend,
    task_capacity: usize,
    ring_entries: u32,
    _tasks: core::marker::PhantomData<T>,
}

impl<T> RuntimeBuilder<T>
where
    T: NativeTask,
{
    /// Select the runtime profile (defaults to [`ProfileKind::Balanced`]).
    pub fn profile(mut self, profile: ProfileKind) -> Self {
        self.profile = profile;
        self
    }

    /// Select the reactor backend (defaults to [`ReactorBackend::Auto`],
    /// which prefers io_uring and falls back to the syscall reactor).
    pub fn backend(mut self, backend: ReactorBackend) -> Self {
        self.backend = backend;
        self
    }

    /// Maximum number of concurrently live tasks (default 64).
    pub fn task_capacity(mut self, capacity: usize) -> Self {
        self.task_capacity = capacity;
        self
    }

    /// Submission-ring entry count for the reactor backend (default 256).
    pub fn ring_entries(mut self, entries: u32) -> Self {
        self.ring_entries = entries;
        self
    }

    /// Build the runtime: create the reactor backend and the profiled
    /// runtime for the selected profile.
    pub fn build(self) -> Result<Runtime<T>, RuntimeError> {
        let driver =
            UnifiedReactor::new(self.ring_entries, self.backend).map_err(RuntimeError::Reactor)?;
        let inner = build_profiled_runtime(self.profile, driver, self.task_capacity)
            .map_err(RuntimeError::Profile)?;
        Ok(Runtime {
            inner,
            epoch: Instant::now(),
        })
    }
}

/// A ready-to-drive dataplane runtime with the reactor backend and profile
/// layout chosen at build time.
///
/// `T` is the application's task type: a state machine implementing
/// [`task::NativeTask`]. Spawn instances with [`Runtime::spawn`], then either
/// call [`Runtime::run`] to drive everything to completion or call
/// [`Runtime::tick`] from a caller-owned loop.
pub struct Runtime<T>
where
    T: NativeTask,
{
    inner: ProfiledRuntime<UnifiedReactor, T>,
    epoch: Instant,
}

impl<T> Runtime<T>
where
    T: NativeTask,
{
    /// Start configuring a runtime.
    pub fn builder() -> RuntimeBuilder<T> {
        RuntimeBuilder {
            profile: ProfileKind::Balanced,
            backend: ReactorBackend::Auto,
            task_capacity: 64,
            ring_entries: 256,
            _tasks: core::marker::PhantomData,
        }
    }

    /// Spawn a task, failing if the profile's bounded task capacity is full.
    pub fn spawn(&mut self, task: T) -> Result<TaskRef, NativeTaskCapacityError<T>> {
        self.inner.try_spawn(task)
    }

    /// True while any task or reactor operation is outstanding.
    pub fn has_work(&self) -> bool {
        self.inner.has_work()
    }

    /// Submit a previously validated raw-pointer reactor operation. The wrapper's
    /// constructor safety contract continues until terminal completion or backend teardown.
    pub fn submit(
        &mut self,
        op: net::RawNetOp,
        wake: Option<TaskRef>,
    ) -> Result<net::OpToken, RuntimeError> {
        let wake = match wake {
            Some(task) => WakeHandle::LocalTask(task),
            None => WakeHandle::None,
        };
        self.inner
            .submit_and_flush_token(op, wake)
            .map_err(RuntimeError::Io)
    }

    /// Run one non-blocking scheduling tick: drain ready completions (waking
    /// their registered tasks) and step ready tasks. Returns the number of
    /// completions drained.
    pub fn tick(&mut self, max_events: usize, task_budget: usize) -> Result<usize, RuntimeError> {
        self.inner
            .tick_completions(max_events, task_budget)
            .map(|(completions, _tasks)| completions.len())
            .map_err(RuntimeError::Io)
    }

    /// Drive the runtime until no task or reactor work remains.
    ///
    /// Budgets come from the profile layout. Blocking waits are bounded by
    /// armed timer deadlines. Returns [`RuntimeError::Stalled`] when the only
    /// remaining tasks are parked with nothing (no in-flight I/O, no timer)
    /// that could ever wake them.
    pub fn run(&mut self) -> Result<(), RuntimeError> {
        const MIN_EVENTS: usize = 1;
        let budgets = self.inner.layout().budgets();
        while self.inner.has_work() {
            let now_ns = self.now_ns();
            let tick = self
                .inner
                .tick_completions_or_wait(
                    now_ns,
                    budgets.completion_budget,
                    MIN_EVENTS,
                    budgets.task_budget,
                )
                .map_err(RuntimeError::Io)?;
            if matches!(tick.controller_step.host_action, BalancedHostAction::Idle)
                && self.inner.has_work()
            {
                return Err(RuntimeError::Stalled {
                    parked_tasks: self.inner.active_tasks(),
                });
            }
        }
        Ok(())
    }

    /// The profile this runtime was built with.
    pub fn profile_kind(&self) -> ProfileKind {
        self.inner.profile_kind()
    }

    /// Escape hatch to the advanced/core API surface.
    pub fn advanced(&mut self) -> &mut ProfiledRuntime<UnifiedReactor, T> {
        &mut self.inner
    }

    fn now_ns(&self) -> u64 {
        u64::try_from(self.epoch.elapsed().as_nanos()).unwrap_or(u64::MAX)
    }
}
