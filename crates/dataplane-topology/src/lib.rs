#![forbid(unsafe_code)]

use core_affinity::CoreId;
use hwlocality::{
    object::{types::ObjectType, TopologyObject},
    Topology,
};
use std::collections::HashSet;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileKind {
    Embedded,
    Balanced,
    Performance,
}

impl ProfileKind {
    #[inline]
    pub fn parse_name(name: &str) -> Option<Self> {
        match name.trim().to_ascii_lowercase().as_str() {
            "embedded" | "esp32" => Some(Self::Embedded),
            "balanced" | "" => Some(Self::Balanced),
            "performance" | "perf" | "server" => Some(Self::Performance),
            _ => None,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum QueueProfile {
    Embedded,
    Balanced,
    Performance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TimerProfile {
    Embedded,
    Balanced,
    Performance,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ParkingProfile {
    Embedded,
    Balanced,
    Performance,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TopologyPlacementPolicy {
    Auto,
    Hwloc { domain: HwlocDomainPreference },
    CoreAffinity,
    GenericRoundRobin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HwlocDomainPreference {
    L3,
    Package,
    Core,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TopologyFallbackPolicy {
    Auto,
    Ordered(Vec<TopologyStrategy>),
    None,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopologyProfile {
    pub profile_kind: ProfileKind,
    pub shard_count: usize,
    pub placement: TopologyPlacementPolicy,
    pub fallback: TopologyFallbackPolicy,
    pub cpu_allowlist: Option<Vec<usize>>,
    pub queue_profile: QueueProfile,
    pub timer_profile: TimerProfile,
    pub parking_profile: ParkingProfile,
}

impl TopologyProfile {
    #[inline]
    pub fn for_kind(profile_kind: ProfileKind) -> Self {
        match profile_kind {
            ProfileKind::Embedded => Self {
                profile_kind,
                shard_count: 2,
                placement: TopologyPlacementPolicy::Auto,
                fallback: TopologyFallbackPolicy::Auto,
                cpu_allowlist: None,
                queue_profile: QueueProfile::Embedded,
                timer_profile: TimerProfile::Embedded,
                parking_profile: ParkingProfile::Embedded,
            },
            ProfileKind::Balanced => Self {
                profile_kind,
                shard_count: 2,
                placement: TopologyPlacementPolicy::Auto,
                fallback: TopologyFallbackPolicy::Auto,
                cpu_allowlist: None,
                queue_profile: QueueProfile::Balanced,
                timer_profile: TimerProfile::Balanced,
                parking_profile: ParkingProfile::Balanced,
            },
            ProfileKind::Performance => Self {
                profile_kind,
                shard_count: 2,
                placement: TopologyPlacementPolicy::Auto,
                fallback: TopologyFallbackPolicy::Auto,
                cpu_allowlist: None,
                queue_profile: QueueProfile::Performance,
                timer_profile: TimerProfile::Performance,
                parking_profile: ParkingProfile::Performance,
            },
        }
    }

    #[inline]
    pub fn with_shard_count(mut self, shard_count: usize) -> Self {
        self.shard_count = shard_count;
        self
    }

    #[inline]
    pub fn embedded_reference() -> Self {
        Self::for_kind(ProfileKind::Embedded)
    }

    #[inline]
    pub fn embedded_dual_shard() -> Self {
        Self::embedded_reference()
    }

    #[inline]
    pub fn balanced_dual_shard() -> Self {
        Self::for_kind(ProfileKind::Balanced)
    }

    #[inline]
    pub fn performance_dual_shard() -> Self {
        Self::for_kind(ProfileKind::Performance)
    }

    #[inline]
    pub fn validate(&self) -> Result<(), TopologyProfileError> {
        if matches!(self.cpu_allowlist.as_ref(), Some(list) if list.is_empty()) {
            return Err(TopologyProfileError::EmptyCpuAllowlist);
        }
        Ok(())
    }

    #[inline]
    pub fn resolve(&self) -> Result<ResolvedTopologyProfile, TopologyProfileError> {
        self.validate()?;
        let topology = match &self.placement {
            TopologyPlacementPolicy::Auto => best_shard_topology(self.shard_count),
            TopologyPlacementPolicy::Hwloc { domain } => self.resolve_with_hwloc(*domain)?,
            TopologyPlacementPolicy::CoreAffinity => {
                let cpus = self.allowed_cpu_ids()?;
                core_affinity_round_robin(self.shard_count, &cpus)
            }
            TopologyPlacementPolicy::GenericRoundRobin => generic_round_robin(self.shard_count),
        };
        Ok(ResolvedTopologyProfile {
            profile: self.clone(),
            topology,
        })
    }

    #[inline]
    pub fn to_shard_group(&self) -> Result<ShardGroup, TopologyProfileError> {
        Ok(ShardGroup::from_topology(self.resolve()?.topology))
    }

    fn allowed_cpu_ids(&self) -> Result<Vec<usize>, TopologyProfileError> {
        if let Some(cpus) = &self.cpu_allowlist {
            let mut cpus = cpus.clone();
            cpus.sort_unstable();
            cpus.dedup();
            if cpus.is_empty() {
                return Err(TopologyProfileError::EmptyCpuAllowlist);
            }
            return Ok(cpus);
        }

        let mut allowed = allowed_core_ids();
        if allowed.is_empty() {
            allowed = generic_cpu_ids();
        }
        allowed.sort_unstable();
        allowed.dedup();
        Ok(allowed)
    }

    fn resolve_with_hwloc(
        &self,
        domain: HwlocDomainPreference,
    ) -> Result<ShardTopology, TopologyProfileError> {
        let allowed = self.allowed_cpu_ids()?;
        let preferred = match domain {
            HwlocDomainPreference::L3 => TopologyStrategy::HwlocL3,
            HwlocDomainPreference::Package => TopologyStrategy::HwlocPackage,
            HwlocDomainPreference::Core => TopologyStrategy::HwlocCore,
        };

        if let Some(topology) = topology_for_strategy(self.shard_count, &allowed, preferred) {
            return Ok(topology);
        }

        match &self.fallback {
            TopologyFallbackPolicy::Auto => Ok(best_shard_topology(self.shard_count)),
            TopologyFallbackPolicy::Ordered(strategies) => strategies
                .iter()
                .find_map(|strategy| topology_for_strategy(self.shard_count, &allowed, *strategy))
                .ok_or(TopologyProfileError::NoFallbackTopology),
            TopologyFallbackPolicy::None => Err(TopologyProfileError::NoFallbackTopology),
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedTopologyProfile {
    pub profile: TopologyProfile,
    pub topology: ShardTopology,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopologyProfileError {
    EmptyCpuAllowlist,
    NoFallbackTopology,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopologyStrategy {
    HwlocL3,
    HwlocPackage,
    HwlocCore,
    CoreAffinityRoundRobin,
    GenericRoundRobin,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShardPlacement {
    pub shard: usize,
    pub core_id: usize,
    pub domain: usize,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShardTopology {
    pub strategy: TopologyStrategy,
    pub domain_count: usize,
    pub placements: Vec<ShardPlacement>,
}

impl ShardTopology {
    #[inline]
    pub fn shard_count(&self) -> usize {
        self.placements.len()
    }

    #[inline]
    pub fn core_for_shard(&self, shard: usize) -> Option<usize> {
        self.placements.get(shard).map(|p| p.core_id)
    }

    #[inline]
    pub fn cpu_plan(&self) -> Vec<usize> {
        self.placements.iter().map(|p| p.core_id).collect()
    }

    #[inline]
    pub fn pin_current_to_shard(&self, shard: usize) -> bool {
        self.core_for_shard(shard)
            .map(pin_current_to_cpu)
            .unwrap_or(false)
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShardGroup {
    topology: ShardTopology,
    push_paths: Vec<Vec<usize>>,
}

impl ShardGroup {
    #[inline]
    pub fn from_topology(topology: ShardTopology) -> Self {
        let shard_count = topology.shard_count();
        let mut push_paths = Vec::with_capacity(shard_count);
        for shard in 0..shard_count {
            push_paths.push(build_push_path_for_shard(shard, &topology.placements));
        }
        Self {
            topology,
            push_paths,
        }
    }

    #[inline]
    pub fn for_shards(shard_count: usize) -> Self {
        Self::from_topology(best_shard_topology(shard_count))
    }

    #[inline]
    pub fn two_shards() -> Self {
        Self::for_shards(2)
    }

    #[inline]
    pub fn topology(&self) -> &ShardTopology {
        &self.topology
    }

    #[inline]
    pub fn shard_count(&self) -> usize {
        self.topology.shard_count()
    }

    #[inline]
    pub fn placements(&self) -> &[ShardPlacement] {
        &self.topology.placements
    }

    #[inline]
    pub fn can_direct_send(&self, src: usize, dst: usize) -> bool {
        src < self.shard_count() && dst < self.shard_count() && src != dst
    }

    #[inline]
    pub fn push_path(&self, src: usize) -> Option<&[usize]> {
        self.push_paths.get(src).map(Vec::as_slice)
    }

    #[inline]
    pub fn next_push_target(&self, src: usize, attempt: usize) -> Option<usize> {
        self.push_path(src)
            .and_then(|path| path.get(attempt))
            .copied()
    }
}

#[inline]
pub fn best_shard_group(shard_count: usize) -> ShardGroup {
    ShardGroup::from_topology(best_shard_topology(shard_count))
}

#[inline]
pub fn two_shard_group() -> ShardGroup {
    ShardGroup::two_shards()
}

pub fn best_shard_topology(shard_count: usize) -> ShardTopology {
    if shard_count == 0 {
        return ShardTopology {
            strategy: TopologyStrategy::GenericRoundRobin,
            domain_count: 0,
            placements: Vec::new(),
        };
    }

    let mut allowed = allowed_core_ids();
    if allowed.is_empty() {
        return generic_round_robin(shard_count);
    }
    allowed.sort_unstable();
    allowed.dedup();

    if let Some(topology) = hwloc_topology(shard_count, &allowed) {
        return topology;
    }

    core_affinity_round_robin(shard_count, &allowed)
}

fn topology_for_strategy(
    shard_count: usize,
    allowed: &[usize],
    strategy: TopologyStrategy,
) -> Option<ShardTopology> {
    match strategy {
        TopologyStrategy::HwlocL3 => hwloc_topology_for_type(
            shard_count,
            allowed,
            ObjectType::L3Cache,
            TopologyStrategy::HwlocL3,
        ),
        TopologyStrategy::HwlocPackage => hwloc_topology_for_type(
            shard_count,
            allowed,
            ObjectType::Package,
            TopologyStrategy::HwlocPackage,
        ),
        TopologyStrategy::HwlocCore => hwloc_topology_for_type(
            shard_count,
            allowed,
            ObjectType::Core,
            TopologyStrategy::HwlocCore,
        ),
        TopologyStrategy::CoreAffinityRoundRobin => {
            Some(core_affinity_round_robin(shard_count, allowed))
        }
        TopologyStrategy::GenericRoundRobin => Some(generic_round_robin(shard_count)),
    }
}

#[inline]
pub fn cpu_plan(shard_count: usize) -> Vec<usize> {
    best_shard_topology(shard_count).cpu_plan()
}

impl ResolvedTopologyProfile {
    #[inline]
    pub fn to_shard_group(self) -> ShardGroup {
        ShardGroup::from_topology(self.topology)
    }
}

#[inline]
pub fn pin_current_to_cpu(core_id: usize) -> bool {
    core_affinity::set_for_current(CoreId { id: core_id })
}

fn hwloc_topology(shard_count: usize, allowed: &[usize]) -> Option<ShardTopology> {
    let topology = Topology::new().ok()?;
    let allowed_set: HashSet<usize> = allowed.iter().copied().collect();

    let candidates = [
        (ObjectType::L3Cache, TopologyStrategy::HwlocL3),
        (ObjectType::Package, TopologyStrategy::HwlocPackage),
        (ObjectType::Core, TopologyStrategy::HwlocCore),
    ];

    for (object_type, strategy) in candidates {
        let mut domains = collect_domains(&topology, object_type, &allowed_set);
        if domains.is_empty() {
            continue;
        }

        domains = domains
            .into_iter()
            .map(|domain| order_domain_cpus(&topology, &domain, &allowed_set))
            .filter(|domain| !domain.is_empty())
            .collect();

        if domains.is_empty() {
            continue;
        }

        return Some(build_topology(shard_count, strategy, domains));
    }

    None
}

fn hwloc_topology_for_type(
    shard_count: usize,
    allowed: &[usize],
    object_type: ObjectType,
    strategy: TopologyStrategy,
) -> Option<ShardTopology> {
    let topology = Topology::new().ok()?;
    let allowed_set: HashSet<usize> = allowed.iter().copied().collect();
    let mut domains = collect_domains(&topology, object_type, &allowed_set);
    if domains.is_empty() {
        return None;
    }

    domains = domains
        .into_iter()
        .map(|domain| order_domain_cpus(&topology, &domain, &allowed_set))
        .filter(|domain| !domain.is_empty())
        .collect();

    if domains.is_empty() {
        return None;
    }

    Some(build_topology(shard_count, strategy, domains))
}

fn build_push_path_for_shard(shard: usize, placements: &[ShardPlacement]) -> Vec<usize> {
    if placements.is_empty() || shard >= placements.len() {
        return Vec::new();
    }

    let me = placements[shard];
    let mut local_domain: Vec<(usize, usize)> = Vec::new();
    let mut remote_domains: Vec<(usize, usize, usize)> = Vec::new();

    for placement in placements {
        if placement.shard == shard {
            continue;
        }
        let core_delta = me.core_id.abs_diff(placement.core_id);
        if placement.domain == me.domain {
            local_domain.push((core_delta, placement.shard));
        } else {
            let domain_delta = me.domain.abs_diff(placement.domain);
            remote_domains.push((domain_delta, core_delta, placement.shard));
        }
    }

    local_domain.sort_unstable();
    remote_domains.sort_unstable();

    let mut route = Vec::with_capacity(placements.len().saturating_sub(1));
    route.extend(local_domain.into_iter().map(|(_, target)| target));
    route.extend(remote_domains.into_iter().map(|(_, _, target)| target));
    route
}

fn collect_domains(
    topology: &Topology,
    object_type: ObjectType,
    allowed: &HashSet<usize>,
) -> Vec<Vec<usize>> {
    let mut out = Vec::new();
    for object in topology.objects_with_type(object_type) {
        let mut cpu_ids = object_cpu_ids(object, allowed);
        if cpu_ids.is_empty() {
            continue;
        }
        cpu_ids.sort_unstable();
        cpu_ids.dedup();
        out.push(cpu_ids);
    }
    out
}

fn order_domain_cpus(
    topology: &Topology,
    domain: &[usize],
    allowed: &HashSet<usize>,
) -> Vec<usize> {
    if domain.len() <= 1 {
        return domain.to_vec();
    }

    let domain_set: HashSet<usize> = domain.iter().copied().collect();
    let mut primary = Vec::with_capacity(domain.len());
    let mut siblings = Vec::with_capacity(domain.len());

    for core in topology.objects_with_type(ObjectType::Core) {
        let mut core_cpus = object_cpu_ids(core, allowed);
        core_cpus.retain(|id| domain_set.contains(id));
        if core_cpus.is_empty() {
            continue;
        }
        core_cpus.sort_unstable();
        core_cpus.dedup();
        primary.push(core_cpus[0]);
        siblings.extend(core_cpus.into_iter().skip(1));
    }

    if primary.is_empty() {
        let mut ordered = domain.to_vec();
        ordered.sort_unstable();
        ordered.dedup();
        return ordered;
    }

    let mut seen = HashSet::with_capacity(domain.len());
    let mut ordered = Vec::with_capacity(domain.len());
    for cpu in primary
        .into_iter()
        .chain(siblings)
        .chain(domain.iter().copied())
    {
        if seen.insert(cpu) {
            ordered.push(cpu);
        }
    }
    ordered
}

fn object_cpu_ids(object: &TopologyObject, allowed: &HashSet<usize>) -> Vec<usize> {
    object
        .cpuset()
        .map(|cpuset| {
            cpuset
                .iter_set()
                .map(usize::from)
                .filter(|cpu| allowed.contains(cpu))
                .collect()
        })
        .unwrap_or_default()
}

fn build_topology(
    shard_count: usize,
    strategy: TopologyStrategy,
    domains: Vec<Vec<usize>>,
) -> ShardTopology {
    let placements = assign_round_robin(shard_count, &domains);
    ShardTopology {
        strategy,
        domain_count: domains.len(),
        placements,
    }
}

fn core_affinity_round_robin(shard_count: usize, cpus: &[usize]) -> ShardTopology {
    let domains = vec![cpus.to_vec()];
    build_topology(
        shard_count,
        TopologyStrategy::CoreAffinityRoundRobin,
        domains,
    )
}

fn generic_round_robin(shard_count: usize) -> ShardTopology {
    let cpus = generic_cpu_ids();
    let domains = vec![cpus];
    build_topology(shard_count, TopologyStrategy::GenericRoundRobin, domains)
}

fn generic_cpu_ids() -> Vec<usize> {
    let count = std::thread::available_parallelism()
        .map(|v| v.get())
        .unwrap_or(1)
        .max(1);
    (0..count).collect()
}

fn allowed_core_ids() -> Vec<usize> {
    core_affinity::get_core_ids()
        .unwrap_or_default()
        .into_iter()
        .map(|core| core.id)
        .collect()
}

fn assign_round_robin(shard_count: usize, domains: &[Vec<usize>]) -> Vec<ShardPlacement> {
    if shard_count == 0 || domains.is_empty() {
        return Vec::new();
    }

    let mut next = vec![0usize; domains.len()];
    let mut placements = Vec::with_capacity(shard_count);

    for shard in 0..shard_count {
        let domain = shard % domains.len();
        let cpus = &domains[domain];
        let idx = next[domain] % cpus.len();
        let core_id = cpus[idx];
        next[domain] = next[domain].wrapping_add(1);
        placements.push(ShardPlacement {
            shard,
            core_id,
            domain,
        });
    }

    placements
}

#[cfg(test)]
mod tests;
