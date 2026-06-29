pub use dataplane_topology::{
    best_shard_group, best_shard_topology, cpu_plan, pin_current_to_cpu, two_shard_group,
    HwlocDomainPreference, ParkingProfile, ProfileKind, QueueProfile, ResolvedTopologyProfile,
    ShardGroup, ShardPlacement, ShardTopology, TimerProfile, TopologyFallbackPolicy,
    TopologyPlacementPolicy, TopologyProfile, TopologyProfileError, TopologyStrategy,
};

#[inline]
pub fn current_thread_shard(shard_count: usize) -> usize {
    dataplane_core_reactor::runtime_tls::current_thread_shard(shard_count)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn two_shard_group_exposes_balanced_dual_shard_surface() {
        let group = two_shard_group();

        assert_eq!(group.shard_count(), 2);
        assert_eq!(group.topology().shard_count(), 2);
        assert!(group.can_direct_send(0, 1));
        assert!(group.can_direct_send(1, 0));
        assert!(!group.can_direct_send(0, 0));
        assert!(!group.can_direct_send(1, 1));
    }

    #[test]
    fn best_shard_topology_and_cpu_plan_agree_for_two_shards() {
        let topology = best_shard_topology(2);
        let cpu_plan = cpu_plan(2);

        assert_eq!(topology.shard_count(), 2);
        assert_eq!(topology.cpu_plan(), cpu_plan);
        assert_eq!(cpu_plan.len(), 2);
    }

    #[test]
    fn runtime_topology_reexports_match_topology_crate_for_zero_shards() {
        let topology = best_shard_topology(0);

        assert_eq!(topology.shard_count(), 0);
        assert!(topology.placements.is_empty());
        assert_eq!(topology.domain_count, 0);
    }

    #[test]
    fn balanced_dual_shard_profile_resolves_through_runtime_surface() {
        let profile = TopologyProfile::balanced_dual_shard();
        let group = profile
            .to_shard_group()
            .expect("group from balanced runtime profile");

        assert_eq!(profile.profile_kind, ProfileKind::Balanced);
        assert_eq!(profile.queue_profile, QueueProfile::Balanced);
        assert_eq!(profile.timer_profile, TimerProfile::Balanced);
        assert_eq!(profile.parking_profile, ParkingProfile::Balanced);
        assert_eq!(group.shard_count(), 2);
    }

    #[test]
    fn embedded_dual_shard_profile_resolves_through_runtime_surface() {
        let profile = TopologyProfile::embedded_reference();
        let group = profile
            .to_shard_group()
            .expect("group from embedded runtime profile");

        assert_eq!(profile.profile_kind, ProfileKind::Embedded);
        assert_eq!(profile.queue_profile, QueueProfile::Embedded);
        assert_eq!(profile.timer_profile, TimerProfile::Embedded);
        assert_eq!(profile.parking_profile, ParkingProfile::Embedded);
        assert_eq!(group.shard_count(), 2);
    }

    #[test]
    fn performance_dual_shard_profile_resolves_through_runtime_surface() {
        let profile = TopologyProfile::performance_dual_shard();
        let group = profile
            .to_shard_group()
            .expect("group from performance runtime profile");

        assert_eq!(profile.profile_kind, ProfileKind::Performance);
        assert_eq!(profile.queue_profile, QueueProfile::Performance);
        assert_eq!(profile.timer_profile, TimerProfile::Performance);
        assert_eq!(profile.parking_profile, ParkingProfile::Performance);
        assert_eq!(group.shard_count(), 2);
    }
}
