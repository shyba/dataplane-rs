#[derive(Clone, Copy, Debug, PartialEq, Eq)]
/// Host-loop configuration for bounded embedded task admission and drive defaults.
pub struct EmbeddedHostLoopConfig {
    /// Maximum number of native tasks that may be admitted concurrently.
    ///
    /// This capacity is enforced by [`super::EmbeddedHostLoop::try_spawn`]. The value is an
    /// admission bound only: it is independent from per-step execution budgets in
    /// [`EmbeddedDriveConfig`] and may be set below the embedded policy task budget.
    ///
    /// Default: `64`.
    pub task_capacity: usize,
    pub drive: EmbeddedDriveConfig,
}

impl EmbeddedHostLoopConfig {
    #[inline]
    /// Returns the configured native-task admission capacity.
    pub const fn task_capacity(&self) -> usize {
        self.task_capacity
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmbeddedDriveConfig {
    pub step_limit: usize,
    pub max_events: usize,
    pub min_events: usize,
    pub task_budget: usize,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct EmbeddedDriveResult {
    pub steps_attempted: usize,
    pub tasks_run: usize,
    pub completions_observed: usize,
    pub work_remains: bool,
}

impl Default for EmbeddedHostLoopConfig {
    fn default() -> Self {
        Self {
            task_capacity: 64,
            drive: EmbeddedDriveConfig::default(),
        }
    }
}

impl Default for EmbeddedDriveConfig {
    fn default() -> Self {
        Self {
            step_limit: 1,
            max_events: 1,
            min_events: 0,
            task_budget: 1,
        }
    }
}
