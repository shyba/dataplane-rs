/// Public runtime defaults for the DataPlane local execution path.
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
    /// Number of runnable tasks to keep in the native engine's hot set.
    ///
    /// This is not a global execution budget. It only controls the native
    /// engine's in-place hot-loop scheduling path.
    native_hot_task_budget: NativeHotTaskBudget,
}

impl DataPlaneSettings {
    /// Construct the public default runtime settings.
    pub const fn new() -> Self {
        Self {
            native_hot_task_budget: NativeHotTaskBudget::new(0),
        }
    }

    /// Return the configured native hot-set size.
    #[inline(always)]
    pub const fn native_hot_task_budget(self) -> usize {
        self.native_hot_task_budget.get()
    }

    /// Return the configured native hot-set budget wrapper.
    #[inline(always)]
    pub const fn native_hot_task_budget_value(self) -> NativeHotTaskBudget {
        self.native_hot_task_budget
    }

    /// Set the native hot-set size used by the local fast-task engine.
    #[inline(always)]
    pub const fn with_native_hot_task_budget(mut self, budget: usize) -> Self {
        self.native_hot_task_budget = NativeHotTaskBudget::new(budget);
        self
    }

    /// Set the native hot-set size using the explicit alloc-side budget type.
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

#[cfg(test)]
mod tests {
    use super::{DataPlaneSettings, NativeHotTaskBudget};

    #[test]
    fn default_settings_match_runtime_baseline() {
        let settings = DataPlaneSettings::default();
        assert_eq!(settings.native_hot_task_budget(), 0);
        assert_eq!(
            settings.native_hot_task_budget_value(),
            NativeHotTaskBudget::default()
        );
    }

    #[test]
    fn builder_updates_hot_task_budget() {
        let settings = DataPlaneSettings::default().with_native_hot_task_budget(4);
        assert_eq!(settings.native_hot_task_budget(), 4);
    }

    #[test]
    fn explicit_budget_type_roundtrips_through_settings() {
        let budget = NativeHotTaskBudget::new(8);
        let settings = DataPlaneSettings::default().with_native_hot_task_budget_value(budget);
        assert_eq!(settings.native_hot_task_budget_value(), budget);
        assert_eq!(settings.native_hot_task_budget(), 8);
    }
}
