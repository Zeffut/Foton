//! The look a villager takes from the ground it appeared on.
//!
//! Vanilla assigns a villager's variant once, from the biome it spawns in, and
//! then never again -- which is why a plains villager led into a desert stays a
//! plains one. Both halves are asked for here, because getting the second wrong
//! would look right in a screenshot and be wrong the moment anybody walks.

use std::sync::Arc;

use glam::DVec3;
use steel_registry::biome::BiomeRef;
use steel_registry::villager_type::VillagerType;
use steel_registry::{
    init_vanilla_registry, vanilla_biomes, vanilla_entities, vanilla_villager_professions,
    vanilla_villager_types,
};
use steel_utils::ChunkPos;

use crate::behavior::init_behaviors;
use crate::block_entity::init_block_entities;
use crate::entity::entities::VillagerEntity;
use crate::entity::mob::Mob;
use crate::entity::{EntitySpawnReason, SharedEntity, next_entity_id};
use crate::test_support::{fresh_test_world, insert_ready_full_chunk_in_biome};
use crate::world::World;

const SPAWN: DVec3 = DVec3::new(8.5, 64.0, 8.5);

/// A world whose one loaded chunk is entirely one biome.
fn world_in_biome(key: &'static str, biome: BiomeRef) -> Arc<World> {
    init_vanilla_registry();
    init_behaviors();
    init_block_entities();
    let world = fresh_test_world(key);
    insert_ready_full_chunk_in_biome(&world, ChunkPos::new(0, 0), biome);
    world
}

fn spawn_villager(world: &Arc<World>) -> Arc<VillagerEntity> {
    let villager = Arc::new(VillagerEntity::new(
        &vanilla_entities::VILLAGER,
        next_entity_id(),
        SPAWN,
        Arc::downgrade(world),
    ));
    world
        .try_add_entity(Arc::clone(&villager) as SharedEntity)
        .expect("the test chunk is loaded, so the villager should attach");
    villager
}

/// The whole feature in one line: what a villager looks like is decided by
/// where it appeared. This runs through `finalize_spawn`, which is the hook
/// every spawn path -- the natural spawner, a spawn egg, `/summon`, worldgen --
/// goes through.
#[test]
fn a_villager_spawned_in_a_desert_is_a_desert_villager() {
    let world = world_in_biome("villager_variant_desert", &vanilla_biomes::DESERT);
    let villager = spawn_villager(&world);
    assert_eq!(
        villager.villager_type().key,
        vanilla_villager_types::PLAINS.key,
        "a villager starts as the default before anything looks at the ground"
    );

    Mob::finalize_spawn(villager.as_ref(), &world, EntitySpawnReason::Natural, None);

    assert_eq!(
        villager.villager_type().key,
        vanilla_villager_types::DESERT.key
    );
    assert!(
        villager.villager_data_finalized(),
        "and the variant is settled, so nothing looks again"
    );
}

/// A biome the mapping does not name is a plains villager. This is the arm the
/// mapping reads as a set of exceptions around, and forest is the ordinary case
/// -- most of the overworld is not on that list.
#[test]
fn a_villager_spawned_somewhere_unlisted_is_a_plains_villager() {
    let world = world_in_biome("villager_variant_forest", &vanilla_biomes::FOREST);
    let villager = spawn_villager(&world);
    villager.set_villager_type(&vanilla_villager_types::SNOW);

    Mob::finalize_spawn(villager.as_ref(), &world, EntitySpawnReason::Natural, None);

    assert_eq!(
        villager.villager_type().key,
        vanilla_villager_types::PLAINS.key,
        "a forest is not on the list, so the fallback decides"
    );
}

/// Once settled, the variant does not move. Without the finalized flag a
/// villager would take on the look of wherever it happened to be respawned or
/// re-finalized, and a cured or bred one would lose the variant it was given.
#[test]
fn a_villager_that_already_has_its_variant_keeps_it() {
    let world = world_in_biome("villager_variant_settled", &vanilla_biomes::DESERT);
    let villager = spawn_villager(&world);
    villager.set_villager_type(&vanilla_villager_types::SNOW);
    villager.set_villager_data_finalized(true);

    Mob::finalize_spawn(villager.as_ref(), &world, EntitySpawnReason::Natural, None);

    assert_eq!(
        villager.villager_type().key,
        vanilla_villager_types::SNOW.key,
        "a settled villager keeps its variant even standing in a desert"
    );
}

/// A newborn arrives with no trade, whatever its parents did for a living.
///
/// Vanilla clears the profession on `EntitySpawnReason.BREEDING` rather than at
/// the birth itself, which is the arm of `finalizeSpawn` this covers.
#[test]
fn a_villager_finalized_as_a_newborn_has_no_trade() {
    let world = world_in_biome("villager_variant_newborn", &vanilla_biomes::DESERT);
    let villager = spawn_villager(&world);
    villager.set_profession(&vanilla_villager_professions::FARMER);

    Mob::finalize_spawn(villager.as_ref(), &world, EntitySpawnReason::Breeding, None);

    assert_eq!(
        villager.profession().key,
        vanilla_villager_professions::NONE.key,
        "a villager finalized as a newborn is unemployed"
    );
}

/// The transcribed half of `VillagerType.BY_BIOME`. A hand-copied table is
/// exactly the kind of thing that goes wrong one line at a time, so one biome
/// from each of its six groups is checked, plus the fallback.
#[test]
fn the_biome_mapping_matches_vanillas() {
    init_vanilla_registry();
    let pairs = [
        (&vanilla_biomes::BADLANDS, &vanilla_villager_types::DESERT),
        (
            &vanilla_biomes::BAMBOO_JUNGLE,
            &vanilla_villager_types::JUNGLE,
        ),
        (
            &vanilla_biomes::WINDSWEPT_SAVANNA,
            &vanilla_villager_types::SAVANNA,
        ),
        (&vanilla_biomes::JAGGED_PEAKS, &vanilla_villager_types::SNOW),
        (
            &vanilla_biomes::MANGROVE_SWAMP,
            &vanilla_villager_types::SWAMP,
        ),
        (
            &vanilla_biomes::WINDSWEPT_FOREST,
            &vanilla_villager_types::TAIGA,
        ),
        // Not on the list, and the one most villagers actually live in.
        (&vanilla_biomes::PLAINS, &vanilla_villager_types::PLAINS),
        (&vanilla_biomes::FOREST, &vanilla_villager_types::PLAINS),
    ];
    for (biome, expected) in pairs {
        assert_eq!(
            VillagerType::by_biome(biome).key,
            expected.key,
            "{} should make a {} villager",
            biome.key,
            expected.key
        );
    }
}
