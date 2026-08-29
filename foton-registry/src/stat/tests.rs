use crate::stat::{Stat, StatValueRegistry};
use crate::{REGISTRY, RegistryExt as _, init_vanilla_registry, vanilla_custom_stats};
use foton_utils::Identifier;

/// The stat type ids are protocol-visible: `ClientboundAwardStatsPacket` writes
/// them straight out, so an ordering change silently relabels every statistic a
/// client is holding.
#[test]
fn the_stat_types_keep_their_registry_ids() {
    init_vanilla_registry();

    let expected = [
        ("mined", StatValueRegistry::Block),
        ("crafted", StatValueRegistry::Item),
        ("used", StatValueRegistry::Item),
        ("broken", StatValueRegistry::Item),
        ("picked_up", StatValueRegistry::Item),
        ("dropped", StatValueRegistry::Item),
        ("killed", StatValueRegistry::EntityType),
        ("killed_by", StatValueRegistry::EntityType),
        ("custom", StatValueRegistry::CustomStat),
    ];
    assert_eq!(REGISTRY.stat_types.len(), expected.len());

    for (id, (name, value_registry)) in expected.into_iter().enumerate() {
        let stat_type = REGISTRY
            .stat_types
            .by_id(id)
            .unwrap_or_else(|| panic!("stat type {id} should exist"));
        assert_eq!(stat_type.key, Identifier::vanilla_static(name));
        assert_eq!(stat_type.value_registry, value_registry);
    }
}

/// Every custom stat has to be registered, because `minecraft:custom` addresses
/// its values by registry id and a missing one shifts every id after it.
#[test]
fn every_custom_stat_is_registered() {
    init_vanilla_registry();

    assert_eq!(REGISTRY.custom_stats.len(), 77);
    assert_eq!(
        REGISTRY
            .custom_stats
            .by_id(0)
            .expect("the first custom stat")
            .key,
        Identifier::vanilla_static("leave_game")
    );
    assert!(
        REGISTRY
            .custom_stats
            .by_key(&Identifier::vanilla_static("play_time"))
            .is_some()
    );
}

/// A stat is a pair of registry ids, and the generated references have to
/// resolve to the same ids the registry hands out.
#[test]
fn a_custom_stat_resolves_to_its_registry_id() {
    init_vanilla_registry();

    let stat = Stat::custom(&vanilla_custom_stats::JUMP);
    assert_eq!(
        REGISTRY.stat_types.by_id(stat.stat_type).map(|t| &t.key),
        Some(&Identifier::vanilla_static("custom"))
    );
    assert_eq!(
        REGISTRY.custom_stats.by_id(stat.value).map(|s| &s.key),
        Some(&Identifier::vanilla_static("jump"))
    );
}
