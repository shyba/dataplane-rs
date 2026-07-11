use crate::balanced_profile::{
    BalancedController, BalancedHostLoopAdapter, BalancedHostPolicy, BalancedParkSlots,
    BalancedParkSlotsConfig, BalancedRecordingHostPolicy, BalancedRuntime, BalancedTimerOwner,
    BalancedTimerOwnerConfig, EmbeddedParkStore, EmbeddedTimerStore, PerformanceParkStore,
    PerformanceTimerStore,
};
use crate::host_loop::HostLoop;
use crate::native_task::NativeTask;
use crate::reactor_driver::ReactorDriver;
use dataplane_topology::{ProfileKind, ShardGroup, TopologyProfile};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BalancedShardRole {
    Control,
    Worker,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct BalancedProfileBudgets {
    pub task_budget: usize,
    pub completion_budget: usize,
    pub ingress_budget: usize,
}

impl Default for BalancedProfileBudgets {
    fn default() -> Self {
        Self {
            task_budget: 256,
            completion_budget: 256,
            ingress_budget: 128,
        }
    }
}

#[inline]
fn embedded_profile_budgets() -> BalancedProfileBudgets {
    BalancedProfileBudgets {
        task_budget: 64,
        completion_budget: 32,
        ingress_budget: 32,
    }
}

#[inline]
fn performance_profile_budgets() -> BalancedProfileBudgets {
    BalancedProfileBudgets {
        task_budget: 512,
        completion_budget: 512,
        ingress_budget: 256,
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct BalancedProfileLayout {
    profile: TopologyProfile,
    shard_group: ShardGroup,
    roles: [BalancedShardRole; 2],
    budgets: BalancedProfileBudgets,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct EmbeddedProfileLayout {
    inner: BalancedProfileLayout,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PerformanceProfileLayout {
    inner: BalancedProfileLayout,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct EmbeddedProfilePolicy {
    pub shard_count: usize,
    pub queue_profile: ProfileKind,
    pub budgets: BalancedProfileBudgets,
    pub timer_owner: BalancedTimerOwnerConfig,
    pub park_slots: BalancedParkSlotsConfig,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BalancedProfileError {
    ShardCountMismatch,
    MissingRoleSplit,
    Topology(dataplane_topology::TopologyProfileError),
}

impl BalancedProfileLayout {
    #[inline]
    pub fn new(
        profile: TopologyProfile,
        shard_group: ShardGroup,
        roles: [BalancedShardRole; 2],
        budgets: BalancedProfileBudgets,
    ) -> Self {
        Self {
            profile,
            shard_group,
            roles,
            budgets,
        }
    }

    #[inline]
    pub fn reference() -> Self {
        Self::from_profile(
            TopologyProfile::balanced_dual_shard(),
            [BalancedShardRole::Control, BalancedShardRole::Worker],
            BalancedProfileBudgets::default(),
        )
        .expect("balanced dual-shard profile should resolve")
    }

    #[inline]
    pub fn from_profile(
        profile: TopologyProfile,
        roles: [BalancedShardRole; 2],
        budgets: BalancedProfileBudgets,
    ) -> Result<Self, BalancedProfileError> {
        let shard_group = profile
            .to_shard_group()
            .map_err(BalancedProfileError::Topology)?;
        let layout = Self::new(profile, shard_group, roles, budgets);
        layout.validate()?;
        Ok(layout)
    }

    #[inline]
    pub fn validate(&self) -> Result<(), BalancedProfileError> {
        if self.profile.shard_count != 2 || self.shard_group.shard_count() != 2 {
            return Err(BalancedProfileError::ShardCountMismatch);
        }
        if self.roles == [BalancedShardRole::Control, BalancedShardRole::Control]
            || self.roles == [BalancedShardRole::Worker, BalancedShardRole::Worker]
        {
            return Err(BalancedProfileError::MissingRoleSplit);
        }
        Ok(())
    }

    #[inline]
    pub fn profile(&self) -> &TopologyProfile {
        &self.profile
    }

    #[inline]
    pub fn shard_group(&self) -> &ShardGroup {
        &self.shard_group
    }

    #[inline]
    pub fn role_for_shard(&self, shard: usize) -> Option<BalancedShardRole> {
        self.roles.get(shard).copied()
    }

    #[inline]
    pub fn budgets(&self) -> BalancedProfileBudgets {
        self.budgets
    }

    #[inline]
    pub fn timer_owner_config(&self) -> BalancedTimerOwnerConfig {
        BalancedTimerOwnerConfig {
            initial_capacity: self.budgets.completion_budget.max(64),
            wake_batch: self.budgets.completion_budget.max(1),
        }
    }

    #[inline]
    pub fn park_slots_config(&self) -> BalancedParkSlotsConfig {
        BalancedParkSlotsConfig {
            slot_count: self.budgets.task_budget.max(64),
        }
    }

    #[inline]
    pub fn make_timer_owner(&self) -> BalancedTimerOwner {
        BalancedTimerOwner::from_config(self.timer_owner_config())
    }

    #[inline]
    pub fn make_park_slots(&self) -> BalancedParkSlots {
        BalancedParkSlots::from_config(self.park_slots_config())
    }

    #[inline]
    pub fn make_controller(&self) -> BalancedController {
        BalancedController::new(self.make_timer_owner(), self.make_park_slots())
    }

    #[inline]
    pub fn make_host_adapter(&self) -> BalancedHostLoopAdapter {
        BalancedHostLoopAdapter::new(self.make_controller())
    }

    #[inline]
    pub fn build_runtime_with_task_capacity<D, T>(
        &self,
        driver: D,
        task_capacity: usize,
    ) -> BalancedRuntime<D, T, BalancedRecordingHostPolicy>
    where
        D: ReactorDriver,
        T: NativeTask,
    {
        let host = HostLoop::new(
            crate::reactor_runtime::ReactorRuntime::new(driver),
            crate::native_task::NativeTaskEngine::<T>::with_task_capacity(task_capacity),
        );
        BalancedRuntime::new(
            self.clone(),
            host,
            self.make_host_adapter(),
            BalancedRecordingHostPolicy::default(),
        )
    }

    #[inline]
    pub fn build_runtime_with_policy<D, T, P>(
        &self,
        driver: D,
        task_capacity: usize,
        policy: P,
    ) -> BalancedRuntime<D, T, P>
    where
        D: ReactorDriver,
        T: NativeTask,
        P: BalancedHostPolicy,
    {
        let host = HostLoop::new(
            crate::reactor_runtime::ReactorRuntime::new(driver),
            crate::native_task::NativeTaskEngine::<T>::with_task_capacity(task_capacity),
        );
        BalancedRuntime::new(self.clone(), host, self.make_host_adapter(), policy)
    }
}

impl EmbeddedProfileLayout {
    #[inline]
    pub fn reference() -> Self {
        Self {
            inner: BalancedProfileLayout::from_profile(
                TopologyProfile::embedded_reference(),
                [BalancedShardRole::Control, BalancedShardRole::Worker],
                embedded_profile_budgets(),
            )
            .expect("embedded reference profile should resolve"),
        }
    }

    #[inline]
    pub fn from_profile(profile: TopologyProfile) -> Result<Self, BalancedProfileError> {
        Ok(Self {
            inner: BalancedProfileLayout::from_profile(
                profile,
                [BalancedShardRole::Control, BalancedShardRole::Worker],
                embedded_profile_budgets(),
            )?,
        })
    }

    #[inline]
    pub fn inner(&self) -> &BalancedProfileLayout {
        &self.inner
    }

    #[inline]
    pub fn profile(&self) -> &TopologyProfile {
        self.inner.profile()
    }

    #[inline]
    pub fn shard_group(&self) -> &ShardGroup {
        self.inner.shard_group()
    }

    #[inline]
    pub fn role_for_shard(&self, shard: usize) -> Option<BalancedShardRole> {
        self.inner.role_for_shard(shard)
    }

    #[inline]
    pub fn budgets(&self) -> BalancedProfileBudgets {
        self.inner.budgets()
    }

    #[inline]
    pub fn timer_owner_config(&self) -> BalancedTimerOwnerConfig {
        BalancedTimerOwnerConfig {
            initial_capacity: self.budgets().completion_budget.max(32),
            wake_batch: self.budgets().completion_budget.clamp(1, 16),
        }
    }

    #[inline]
    pub fn park_slots_config(&self) -> BalancedParkSlotsConfig {
        BalancedParkSlotsConfig {
            slot_count: self.budgets().task_budget.max(32),
        }
    }

    #[inline]
    pub fn policy(&self) -> EmbeddedProfilePolicy {
        EmbeddedProfilePolicy {
            shard_count: self.profile().shard_count,
            queue_profile: self.profile().profile_kind,
            budgets: self.budgets(),
            timer_owner: self.timer_owner_config(),
            park_slots: self.park_slots_config(),
        }
    }

    #[inline]
    pub fn make_timer_owner(&self) -> BalancedTimerOwner {
        BalancedTimerOwner::from_config(self.timer_owner_config())
    }

    #[inline]
    pub fn make_timer_store(&self) -> EmbeddedTimerStore {
        EmbeddedTimerStore::from_config(self.timer_owner_config())
    }

    #[inline]
    pub fn make_park_slots(&self) -> BalancedParkSlots {
        BalancedParkSlots::from_config(self.park_slots_config())
    }

    #[inline]
    pub fn make_park_store(&self) -> EmbeddedParkStore {
        EmbeddedParkStore::from_config(self.park_slots_config())
    }

    #[inline]
    pub fn make_controller(&self) -> BalancedController<EmbeddedTimerStore, EmbeddedParkStore> {
        BalancedController::new(self.make_timer_store(), self.make_park_store())
    }

    #[inline]
    pub fn make_host_adapter(
        &self,
    ) -> BalancedHostLoopAdapter<EmbeddedTimerStore, EmbeddedParkStore> {
        BalancedHostLoopAdapter::new(self.make_controller())
    }

    #[inline]
    pub fn build_runtime_with_task_capacity<D, T>(
        &self,
        driver: D,
        task_capacity: usize,
    ) -> BalancedRuntime<D, T, BalancedRecordingHostPolicy, EmbeddedTimerStore, EmbeddedParkStore>
    where
        D: ReactorDriver,
        T: NativeTask,
    {
        let host = HostLoop::new(
            crate::reactor_runtime::ReactorRuntime::new(driver),
            crate::native_task::NativeTaskEngine::<T>::with_task_capacity_limit(
                task_capacity,
                task_capacity,
            ),
        );
        BalancedRuntime::new(
            self.inner.clone(),
            host,
            self.make_host_adapter(),
            BalancedRecordingHostPolicy::default(),
        )
    }

    #[inline]
    pub fn build_runtime_with_policy<D, T, P>(
        &self,
        driver: D,
        task_capacity: usize,
        policy: P,
    ) -> BalancedRuntime<D, T, P, EmbeddedTimerStore, EmbeddedParkStore>
    where
        D: ReactorDriver,
        T: NativeTask,
        P: BalancedHostPolicy,
    {
        let host = HostLoop::new(
            crate::reactor_runtime::ReactorRuntime::new(driver),
            crate::native_task::NativeTaskEngine::<T>::with_task_capacity_limit(
                task_capacity,
                task_capacity,
            ),
        );
        BalancedRuntime::new(self.inner.clone(), host, self.make_host_adapter(), policy)
    }
}

impl PerformanceProfileLayout {
    #[inline]
    pub fn reference() -> Self {
        Self {
            inner: BalancedProfileLayout::from_profile(
                TopologyProfile::performance_dual_shard(),
                [BalancedShardRole::Control, BalancedShardRole::Worker],
                performance_profile_budgets(),
            )
            .expect("performance dual-shard profile should resolve"),
        }
    }

    #[inline]
    pub fn from_profile(profile: TopologyProfile) -> Result<Self, BalancedProfileError> {
        Ok(Self {
            inner: BalancedProfileLayout::from_profile(
                profile,
                [BalancedShardRole::Control, BalancedShardRole::Worker],
                performance_profile_budgets(),
            )?,
        })
    }

    #[inline]
    pub fn inner(&self) -> &BalancedProfileLayout {
        &self.inner
    }

    #[inline]
    pub fn profile(&self) -> &TopologyProfile {
        self.inner.profile()
    }

    #[inline]
    pub fn shard_group(&self) -> &ShardGroup {
        self.inner.shard_group()
    }

    #[inline]
    pub fn role_for_shard(&self, shard: usize) -> Option<BalancedShardRole> {
        self.inner.role_for_shard(shard)
    }

    #[inline]
    pub fn budgets(&self) -> BalancedProfileBudgets {
        self.inner.budgets()
    }

    #[inline]
    pub fn timer_owner_config(&self) -> BalancedTimerOwnerConfig {
        BalancedTimerOwnerConfig {
            initial_capacity: self.budgets().completion_budget.max(512),
            wake_batch: self.budgets().completion_budget.max(128),
        }
    }

    #[inline]
    pub fn park_slots_config(&self) -> BalancedParkSlotsConfig {
        BalancedParkSlotsConfig {
            slot_count: self.budgets().task_budget.max(512),
        }
    }

    #[inline]
    pub fn make_timer_owner(&self) -> BalancedTimerOwner {
        BalancedTimerOwner::from_config(self.timer_owner_config())
    }

    #[inline]
    pub fn make_timer_store(&self) -> PerformanceTimerStore {
        PerformanceTimerStore::from_config(self.timer_owner_config())
    }

    #[inline]
    pub fn make_park_slots(&self) -> BalancedParkSlots {
        BalancedParkSlots::from_config(self.park_slots_config())
    }

    #[inline]
    pub fn make_park_store(&self) -> PerformanceParkStore {
        PerformanceParkStore::from_config(self.park_slots_config())
    }

    #[inline]
    pub fn make_controller(
        &self,
    ) -> BalancedController<PerformanceTimerStore, PerformanceParkStore> {
        BalancedController::new(self.make_timer_store(), self.make_park_store())
    }

    #[inline]
    pub fn make_host_adapter(
        &self,
    ) -> BalancedHostLoopAdapter<PerformanceTimerStore, PerformanceParkStore> {
        BalancedHostLoopAdapter::new(self.make_controller())
    }

    #[inline]
    pub fn build_runtime_with_task_capacity<D, T>(
        &self,
        driver: D,
        task_capacity: usize,
    ) -> BalancedRuntime<
        D,
        T,
        BalancedRecordingHostPolicy,
        PerformanceTimerStore,
        PerformanceParkStore,
    >
    where
        D: ReactorDriver,
        T: NativeTask,
    {
        let host = HostLoop::new(
            crate::reactor_runtime::ReactorRuntime::new(driver),
            crate::native_task::NativeTaskEngine::<T>::with_task_capacity(task_capacity),
        );
        BalancedRuntime::new(
            self.inner.clone(),
            host,
            self.make_host_adapter(),
            BalancedRecordingHostPolicy::default(),
        )
    }

    #[inline]
    pub fn build_runtime_with_policy<D, T, P>(
        &self,
        driver: D,
        task_capacity: usize,
        policy: P,
    ) -> BalancedRuntime<D, T, P, PerformanceTimerStore, PerformanceParkStore>
    where
        D: ReactorDriver,
        T: NativeTask,
        P: BalancedHostPolicy,
    {
        let host = HostLoop::new(
            crate::reactor_runtime::ReactorRuntime::new(driver),
            crate::native_task::NativeTaskEngine::<T>::with_task_capacity(task_capacity),
        );
        BalancedRuntime::new(self.inner.clone(), host, self.make_host_adapter(), policy)
    }
}
