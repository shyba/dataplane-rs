//! CPU/topology-aware shard placement for the dataplane runtime.
//!
//! Discovers the host CPU layout via hwloc (cache/package/core domains) with
//! `core_affinity` and generic round-robin fallbacks, and maps runtime shards
//! onto concrete CPU cores. Also defines the profile-kind vocabulary
//! ([`ProfileKind`]: Embedded/Balanced/Performance) plus the per-subsystem
//! queue, timer, and parking profile knobs consumed by the runtime builders.

#![forbid(unsafe_code)]

use core_affinity::CoreId;
use hwlocality::{
    object::{types::ObjectType, TopologyObject},
    Topology,
};
use std::collections::HashSet;

/// Named runtime tuning tier selecting shard layout and subsystem defaults.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ProfileKind {
    /// Minimal-footprint tier for constrained targets.
    Embedded,
    /// Default tier balancing throughput and resource usage.
    Balanced,
    /// Throughput-oriented tier for server-class hosts.
    Performance,
}

impl ProfileKind {
    /// Parses a case-insensitive profile name, accepting aliases
    /// (`"esp32"` → Embedded, `""` → Balanced, `"perf"`/`"server"` → Performance).
    /// Returns `None` for unrecognized names.
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

/// How shard-to-core placement is decided when resolving a [`TopologyProfile`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TopologyPlacementPolicy {
    /// Pick the best available strategy: hwloc (L3, then package, then core),
    /// falling back to core-affinity round-robin, then generic round-robin.
    Auto,
    /// Use hwloc placement grouped by the given domain, honoring the
    /// profile's [`TopologyFallbackPolicy`] if that domain is unavailable.
    Hwloc {
        /// Preferred hwloc grouping domain.
        domain: HwlocDomainPreference,
    },
    /// Round-robin shards over the CPUs reported by `core_affinity`
    /// (or the profile's allowlist).
    CoreAffinity,
    /// Round-robin shards over `0..available_parallelism` with no
    /// topology awareness.
    GenericRoundRobin,
}

/// hwloc object type used to group CPUs into placement domains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum HwlocDomainPreference {
    /// Group CPUs sharing an L3 cache.
    L3,
    /// Group CPUs by physical package (socket).
    Package,
    /// Group CPUs by physical core (SMT siblings together).
    Core,
}

/// What to do when the preferred hwloc placement domain is unavailable.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TopologyFallbackPolicy {
    /// Fall back to [`best_shard_topology`] (hwloc auto, then core-affinity,
    /// then generic round-robin).
    Auto,
    /// Try the given strategies in order; error with
    /// [`TopologyProfileError::NoFallbackTopology`] if none succeeds.
    Ordered(Vec<TopologyStrategy>),
    /// No fallback: fail with [`TopologyProfileError::NoFallbackTopology`].
    None,
}

/// Declarative shard-placement and subsystem-tuning specification,
/// resolved into a concrete [`ShardTopology`] via [`TopologyProfile::resolve`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TopologyProfile {
    /// Tuning tier this profile belongs to.
    pub profile_kind: ProfileKind,
    /// Number of shards to place.
    pub shard_count: usize,
    /// Shard-to-core placement policy.
    pub placement: TopologyPlacementPolicy,
    /// Fallback behavior when hwloc placement fails.
    pub fallback: TopologyFallbackPolicy,
    /// Optional restriction of placement to these CPU ids; `None` uses all
    /// CPUs visible to `core_affinity`. Must be non-empty if set.
    pub cpu_allowlist: Option<Vec<usize>>,
}

impl TopologyProfile {
    /// Returns the default profile for a tier: two shards, auto
    /// placement/fallback, and no allowlist. Subsystem tuning (queues,
    /// timers, parking) derives from `profile_kind`.
    #[inline]
    pub fn for_kind(profile_kind: ProfileKind) -> Self {
        Self {
            profile_kind,
            shard_count: 2,
            placement: TopologyPlacementPolicy::Auto,
            fallback: TopologyFallbackPolicy::Auto,
            cpu_allowlist: None,
        }
    }

    /// Returns the profile with `shard_count` overridden.
    #[inline]
    pub fn with_shard_count(mut self, shard_count: usize) -> Self {
        self.shard_count = shard_count;
        self
    }

    /// Default Embedded-tier profile (two shards).
    #[inline]
    pub fn embedded_reference() -> Self {
        Self::for_kind(ProfileKind::Embedded)
    }

    /// Alias for [`TopologyProfile::embedded_reference`].
    #[inline]
    pub fn embedded_dual_shard() -> Self {
        Self::embedded_reference()
    }

    /// Default Balanced-tier profile (two shards).
    #[inline]
    pub fn balanced_dual_shard() -> Self {
        Self::for_kind(ProfileKind::Balanced)
    }

    /// Default Performance-tier profile (two shards).
    #[inline]
    pub fn performance_dual_shard() -> Self {
        Self::for_kind(ProfileKind::Performance)
    }

    /// Checks profile invariants (currently: a set `cpu_allowlist` must be
    /// non-empty) without touching the host topology.
    #[inline]
    pub fn validate(&self) -> Result<(), TopologyProfileError> {
        if matches!(self.cpu_allowlist.as_ref(), Some(list) if list.is_empty()) {
            return Err(TopologyProfileError::EmptyCpuAllowlist);
        }
        Ok(())
    }

    /// Validates the profile and computes a concrete shard-to-core placement
    /// on the current host according to the placement and fallback policies.
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

    /// Resolves the profile and wraps the resulting topology in a
    /// [`ShardGroup`] with precomputed push paths.
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

/// A [`TopologyProfile`] paired with the concrete [`ShardTopology`] it
/// resolved to on this host.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ResolvedTopologyProfile {
    /// The profile that was resolved.
    pub profile: TopologyProfile,
    /// The computed shard-to-core placement.
    pub topology: ShardTopology,
}

/// Errors from validating or resolving a [`TopologyProfile`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopologyProfileError {
    /// `cpu_allowlist` was set but contained no CPUs.
    EmptyCpuAllowlist,
    /// The preferred placement failed and the fallback policy yielded no
    /// usable topology.
    NoFallbackTopology,
}

/// Concrete placement strategy recorded in a resolved [`ShardTopology`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TopologyStrategy {
    /// hwloc placement with L3-cache domains.
    HwlocL3,
    /// hwloc placement with package (socket) domains.
    HwlocPackage,
    /// hwloc placement with physical-core domains.
    HwlocCore,
    /// Round-robin over CPUs reported by `core_affinity` (single domain).
    CoreAffinityRoundRobin,
    /// Round-robin over `0..available_parallelism` (single domain).
    GenericRoundRobin,
}

/// Assignment of one shard to a CPU core within a placement domain.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct ShardPlacement {
    /// Shard index.
    pub shard: usize,
    /// OS CPU id the shard is placed on.
    pub core_id: usize,
    /// Index of the placement domain (L3/package/core group) containing the CPU.
    pub domain: usize,
}

/// Resolved shard-to-core placement: the strategy used, the number of
/// placement domains, and one [`ShardPlacement`] per shard.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShardTopology {
    /// Strategy that produced this placement.
    pub strategy: TopologyStrategy,
    /// Number of placement domains shards were spread across.
    pub domain_count: usize,
    /// Per-shard core assignments, indexed by shard.
    pub placements: Vec<ShardPlacement>,
}

impl ShardTopology {
    /// Number of placed shards.
    #[inline]
    pub fn shard_count(&self) -> usize {
        self.placements.len()
    }

    /// CPU id assigned to `shard`, or `None` if the shard index is out of range.
    #[inline]
    pub fn core_for_shard(&self, shard: usize) -> Option<usize> {
        self.placements.get(shard).map(|p| p.core_id)
    }

    /// CPU ids in shard order (index = shard, value = core id).
    #[inline]
    pub fn cpu_plan(&self) -> Vec<usize> {
        self.placements.iter().map(|p| p.core_id).collect()
    }

    /// Pins the current thread to `shard`'s assigned CPU; returns `false`
    /// if the shard is unknown or pinning failed.
    #[inline]
    pub fn pin_current_to_shard(&self, shard: usize) -> bool {
        self.core_for_shard(shard)
            .map(pin_current_to_cpu)
            .unwrap_or(false)
    }
}

/// A [`ShardTopology`] plus precomputed locality-ordered push routes,
/// used for shard-to-shard message routing.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ShardGroup {
    topology: ShardTopology,
    push_paths: Vec<Vec<usize>>,
}

impl ShardGroup {
    /// Builds a group from a resolved topology, precomputing each shard's
    /// push path (same-domain peers first, ordered by core distance, then
    /// remote-domain peers by domain and core distance).
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

    /// Builds a group of `shard_count` shards using [`best_shard_topology`].
    #[inline]
    pub fn for_shards(shard_count: usize) -> Self {
        Self::from_topology(best_shard_topology(shard_count))
    }

    /// Convenience for [`ShardGroup::for_shards`] with two shards.
    #[inline]
    pub fn two_shards() -> Self {
        Self::for_shards(2)
    }

    /// The underlying resolved topology.
    #[inline]
    pub fn topology(&self) -> &ShardTopology {
        &self.topology
    }

    /// Number of shards in the group.
    #[inline]
    pub fn shard_count(&self) -> usize {
        self.topology.shard_count()
    }

    /// Per-shard core assignments.
    #[inline]
    pub fn placements(&self) -> &[ShardPlacement] {
        &self.topology.placements
    }

    /// Whether `src` may send directly to `dst`: both indices in range and
    /// distinct.
    #[inline]
    pub fn can_direct_send(&self, src: usize, dst: usize) -> bool {
        src < self.shard_count() && dst < self.shard_count() && src != dst
    }

    /// Locality-ordered list of target shards for pushes from `src`
    /// (closest peers first), or `None` if `src` is out of range.
    #[inline]
    pub fn push_path(&self, src: usize) -> Option<&[usize]> {
        self.push_paths.get(src).map(Vec::as_slice)
    }

    /// The `attempt`-th target on `src`'s push path, or `None` when the
    /// path is exhausted or `src` is out of range.
    #[inline]
    pub fn next_push_target(&self, src: usize, attempt: usize) -> Option<usize> {
        self.push_path(src)
            .and_then(|path| path.get(attempt))
            .copied()
    }
}

/// Builds a [`ShardGroup`] over the best available topology for `shard_count` shards.
#[inline]
pub fn best_shard_group(shard_count: usize) -> ShardGroup {
    ShardGroup::from_topology(best_shard_topology(shard_count))
}

/// Convenience for [`best_shard_group`] with two shards.
#[inline]
pub fn two_shard_group() -> ShardGroup {
    ShardGroup::two_shards()
}

/// Computes the best shard placement the host supports: hwloc domains
/// (L3, then package, then core), then core-affinity round-robin, then
/// generic round-robin. A zero `shard_count` yields an empty topology.
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

/// CPU ids for `shard_count` shards from [`best_shard_topology`], in shard order.
#[inline]
pub fn cpu_plan(shard_count: usize) -> Vec<usize> {
    best_shard_topology(shard_count).cpu_plan()
}

impl ResolvedTopologyProfile {
    /// Consumes the resolution and builds a [`ShardGroup`] from its topology.
    #[inline]
    pub fn to_shard_group(self) -> ShardGroup {
        ShardGroup::from_topology(self.topology)
    }
}

/// Pins the current thread to the given OS CPU id; returns `false` on failure.
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
