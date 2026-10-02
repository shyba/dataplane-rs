use super::*;

#[test]
fn assign_round_robin_balances_domains() {
    let domains = vec![vec![0, 1], vec![8, 9]];
    let plan = assign_round_robin(8, &domains);
    let left = plan.iter().filter(|p| p.domain == 0).count();
    let right = plan.iter().filter(|p| p.domain == 1).count();
    assert_eq!(left, right);
}

#[test]
fn topology_smoke_has_requested_shards() {
    let shards = 17;
    let topology = best_shard_topology(shards);
    assert_eq!(topology.shard_count(), shards);
    assert_eq!(topology.cpu_plan().len(), shards);
    assert!(!topology.placements.is_empty());
}

#[test]
fn zero_shards_is_empty() {
    let topology = best_shard_topology(0);
    assert!(topology.placements.is_empty());
    assert_eq!(topology.domain_count, 0);
}

#[test]
fn two_shard_group_has_bidirectional_direct_send() {
    let group = two_shard_group();
    assert_eq!(group.shard_count(), 2);
    assert!(group.can_direct_send(0, 1));
    assert!(group.can_direct_send(1, 0));
    assert!(!group.can_direct_send(0, 0));
}

#[test]
fn push_path_prefers_local_domain_first() {
    let topology = ShardTopology {
        strategy: TopologyStrategy::GenericRoundRobin,
        domain_count: 2,
        placements: vec![
            ShardPlacement {
                shard: 0,
                core_id: 0,
                domain: 0,
            },
            ShardPlacement {
                shard: 1,
                core_id: 2,
                domain: 0,
            },
            ShardPlacement {
                shard: 2,
                core_id: 16,
                domain: 1,
            },
            ShardPlacement {
                shard: 3,
                core_id: 18,
                domain: 1,
            },
        ],
    };
    let group = ShardGroup::from_topology(topology);
    assert_eq!(group.push_path(0), Some(&[1, 2, 3][..]));
    assert_eq!(group.next_push_target(0, 0), Some(1));
    assert_eq!(group.next_push_target(0, 10), None);
}

#[test]
fn balanced_dual_shard_profile_resolves_to_two_shards() {
    let profile = TopologyProfile::balanced_dual_shard();
    let resolved = profile.resolve().expect("resolve balanced profile");
    assert_eq!(resolved.profile.profile_kind, ProfileKind::Balanced);
    assert_eq!(resolved.topology.shard_count(), 2);
}

#[test]
fn embedded_reference_profile_resolves_to_two_shards() {
    let profile = TopologyProfile::embedded_reference();
    let resolved = profile.resolve().expect("resolve embedded profile");
    assert_eq!(resolved.profile.profile_kind, ProfileKind::Embedded);
    assert_eq!(resolved.topology.shard_count(), 2);
}

#[test]
fn performance_dual_shard_profile_resolves_to_two_shards() {
    let profile = TopologyProfile::performance_dual_shard();
    let resolved = profile.resolve().expect("resolve performance profile");
    assert_eq!(resolved.profile.profile_kind, ProfileKind::Performance);
    assert_eq!(resolved.topology.shard_count(), 2);
}

#[test]
fn dual_shard_family_constructors_keep_profile_axes_aligned() {
    let embedded = TopologyProfile::embedded_reference();
    assert_eq!(embedded.profile_kind, ProfileKind::Embedded);

    let balanced = TopologyProfile::balanced_dual_shard();
    assert_eq!(balanced.profile_kind, ProfileKind::Balanced);

    let performance = TopologyProfile::performance_dual_shard();
    assert_eq!(performance.profile_kind, ProfileKind::Performance);
}

#[test]
fn profile_kind_parse_name_accepts_aliases() {
    assert_eq!(
        ProfileKind::parse_name("embedded"),
        Some(ProfileKind::Embedded)
    );
    assert_eq!(
        ProfileKind::parse_name("esp32"),
        Some(ProfileKind::Embedded)
    );
    assert_eq!(
        ProfileKind::parse_name("balanced"),
        Some(ProfileKind::Balanced)
    );
    assert_eq!(
        ProfileKind::parse_name("perf"),
        Some(ProfileKind::Performance)
    );
    assert_eq!(
        ProfileKind::parse_name("server"),
        Some(ProfileKind::Performance)
    );
    assert_eq!(ProfileKind::parse_name("unknown"), None);
}

#[test]
fn profile_for_kind_and_shard_count_keep_axes_aligned() {
    let profile = TopologyProfile::for_kind(ProfileKind::Performance).with_shard_count(4);
    assert_eq!(profile.profile_kind, ProfileKind::Performance);
    assert_eq!(profile.shard_count, 4);
    assert_eq!(profile.profile_kind, ProfileKind::Performance);
}

#[test]
fn profile_rejects_empty_cpu_allowlist() {
    let mut profile = TopologyProfile::balanced_dual_shard();
    profile.cpu_allowlist = Some(Vec::new());
    assert_eq!(
        profile.resolve().unwrap_err(),
        TopologyProfileError::EmptyCpuAllowlist
    );
}

#[test]
fn profile_to_shard_group_matches_topology_shard_count() {
    let profile = TopologyProfile::balanced_dual_shard();
    let group = profile.to_shard_group().expect("group from profile");
    assert_eq!(group.shard_count(), profile.shard_count);
}

#[test]
fn every_placement_and_fallback_preserves_explicit_allowlist() {
    // A nonexistent host CPU forces hwloc discovery to fail deterministically.
    // Resolution is planning only; no attempt is made to pin to this CPU.
    let cpu = usize::MAX / 2;
    for placement in [
        TopologyPlacementPolicy::Auto,
        TopologyPlacementPolicy::CoreAffinity,
        TopologyPlacementPolicy::GenericRoundRobin,
        TopologyPlacementPolicy::Hwloc {
            domain: HwlocDomainPreference::L3,
        },
    ] {
        for fallback in [
            TopologyFallbackPolicy::Auto,
            TopologyFallbackPolicy::Ordered(vec![TopologyStrategy::GenericRoundRobin]),
            TopologyFallbackPolicy::Ordered(vec![TopologyStrategy::CoreAffinityRoundRobin]),
        ] {
            let mut profile = TopologyProfile::balanced_dual_shard().with_shard_count(3);
            profile.cpu_allowlist = Some(vec![cpu, cpu]);
            profile.placement = placement.clone();
            profile.fallback = fallback;
            let resolved = profile.resolve().unwrap();
            assert_eq!(resolved.topology.cpu_plan(), vec![cpu; 3]);
        }
    }
}
