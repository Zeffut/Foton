//! Animals placed as the world is made.
//!
//! Vanilla parity: `ChunkStatus.SPAWN`, which runs
//! `ChunkGenerator.spawnOriginalMobs` into
//! `NaturalSpawner.spawnMobsForChunkGeneration`. This is where a new world gets
//! its animals: the per-tick spawner keeps a trickle going near players, but it
//! is this pass that puts a herd in a field before anyone has seen it. Without
//! it a freshly generated world is silent until something wanders in.
//!
//! Only the creature category is placed here, which is vanilla's rule and the
//! reason cows are everywhere on day one while squid are not.

use std::sync::Arc;

use steel_registry::REGISTRY;
use steel_registry::RegistryExt as _;
use steel_registry::biome::{BiomeRef, SpawnerData};
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::entity_type::MobCategory;
use steel_registry::vanilla_game_rules::SPAWN_MOBS;
use steel_utils::random::Random as _;
use steel_utils::random::legacy_random::LegacyRandom;
use steel_utils::{BlockPos, BlockStateId, ChunkPos};

use crate::chunk::heightmap::HeightmapType;
use crate::chunk::{
    chunk_generation_task::StaticCache2D, chunk_holder::ChunkHolder, chunk_pyramid::ChunkStep,
};
use crate::entity::{ENTITIES, EntitySpawnReason, SpawnGroupData, next_entity_id};
use crate::world::spawn_placement::{SpawnBlockSource, spawn_placement_for};
use crate::world::{LevelReader as _, World};
use crate::worldgen::generator::ChunkGenerator as _;
use crate::worldgen::generator::context::WorldGenContext;
use crate::worldgen::region::WorldGenRegion;

/// Tries per animal before the pack gives up on it.
///
/// Vanilla parity: the `attempt < 4` of `spawnMobsForChunkGeneration`.
const PLACEMENT_ATTEMPTS: u32 = 4;

/// How far each animal shuffles from the last one.
///
/// Vanilla parity: the `nextInt(5) - nextInt(5)` that walks the pack across the
/// chunk, which is what makes a herd stand together rather than in a stack.
const PACK_SCATTER: i32 = 5;

/// Blocks along one edge of a chunk.
const CHUNK_SIZE: i32 = 16;

impl SpawnBlockSource for WorldGenRegion<'_> {
    fn spawn_block_state(&self, pos: BlockPos) -> BlockStateId {
        self.block_state(pos)
    }
}

pub(crate) fn generate(
    context: Arc<WorldGenContext>,
    step: &ChunkStep,
    cache: &Arc<StaticCache2D<Arc<ChunkHolder>>>,
    holder: Arc<ChunkHolder>,
) {
    let center = holder.get_pos();
    let world = context.world();
    if !world.get_game_rule(&SPAWN_MOBS) {
        return;
    }

    let world_seed = world.seed();
    let region_random = context
        .generator
        .create_worldgen_region_random(world_seed, center);
    let region = WorldGenRegion::new(context.as_ref(), step, cache, center, region_random);

    spawn_mobs_for_chunk_generation(&region, &world, center, world_seed);
}

/// Fills one freshly generated chunk with the animals its biome calls for.
///
/// Vanilla parity: `NaturalSpawner.spawnMobsForChunkGeneration`.
fn spawn_mobs_for_chunk_generation(
    region: &WorldGenRegion<'_>,
    world: &Arc<World>,
    chunk_pos: ChunkPos,
    world_seed: i64,
) {
    let min_x = chunk_pos.0.x * CHUNK_SIZE;
    let min_z = chunk_pos.0.y * CHUNK_SIZE;

    let Some(biome) = biome_for_chunk(region, world, chunk_pos) else {
        return;
    };
    let Some(candidates) = biome.spawners.get(MobCategory::Creature.name()) else {
        return;
    };
    if candidates.is_empty() {
        return;
    }

    // Vanilla parity: `spawnOriginalMobs` seeds a fresh random from the chunk
    // corner, so the same seed always stocks the same chunk the same way. It
    // builds that random over a `LegacyRandomSource`, not the Xoroshiro one
    // feature decoration uses, and the two produce different sequences -- so
    // this has to be the legacy generator to place animals where vanilla does.
    // The seed it starts from is discarded by `set_decoration_seed`.
    let mut random = LegacyRandom::from_seed(0);
    random.set_decoration_seed(world_seed, min_x, min_z);

    while random.next_f32() < biome.creature_spawn_probability {
        let Some(entry) = pick_weighted(candidates, &mut random) else {
            return;
        };
        place_pack(region, world, entry, min_x, min_z, &mut random);
    }
}

/// Returns the biome deciding what this chunk is stocked with.
///
/// Vanilla parity: `spawnOriginalMobs` reads one biome, at the chunk corner and
/// at the top of the world, and stocks the whole chunk from it.
fn biome_for_chunk(
    region: &WorldGenRegion<'_>,
    world: &Arc<World>,
    chunk_pos: ChunkPos,
) -> Option<BiomeRef> {
    let top = world.max_y_exclusive() - 1;
    let quart = BlockPos::new(chunk_pos.0.x * CHUNK_SIZE, top, chunk_pos.0.y * CHUNK_SIZE);
    let biome_id = region.noise_biome_id(quart.x() >> 2, quart.y() >> 2, quart.z() >> 2);
    REGISTRY.biomes.by_id(usize::from(biome_id))
}

/// Picks one entry out of a biome's weighted list.
///
/// Vanilla parity: `WeightedList.getRandom`.
fn pick_weighted<'entries>(
    candidates: &'entries [SpawnerData],
    random: &mut LegacyRandom,
) -> Option<&'entries SpawnerData> {
    let total: i32 = candidates.iter().map(|entry| entry.weight).sum();
    if total <= 0 {
        return None;
    }

    let mut roll = random.next_i32_bounded(total);
    for entry in candidates {
        roll -= entry.weight;
        if roll < 0 {
            return Some(entry);
        }
    }
    None
}

/// Places one group of animals somewhere in the chunk.
///
/// Vanilla parity: the body of the `while` in `spawnMobsForChunkGeneration`.
/// The group data threads from one animal to the next, which is what decides
/// how many of a herd are calves.
fn place_pack(
    region: &WorldGenRegion<'_>,
    world: &Arc<World>,
    entry: &SpawnerData,
    min_x: i32,
    min_z: i32,
    random: &mut LegacyRandom,
) {
    let Some(entity_type) = REGISTRY.entity_types.by_key(&entry.entity_type) else {
        // Only entity types Steel implements can spawn; the rest wait for their
        // entity to exist.
        return;
    };
    if !entity_type.summonable {
        return;
    }

    let span = 1 + entry.max_count - entry.min_count;
    let count = entry.min_count + random.next_i32_bounded(span.max(1));

    let start_x = min_x + random.next_i32_bounded(CHUNK_SIZE);
    let start_z = min_z + random.next_i32_bounded(CHUNK_SIZE);
    let mut x = start_x;
    let mut z = start_z;

    let mut group_data: Option<SpawnGroupData> = None;
    let placement = spawn_placement_for(entity_type);

    for _ in 0..count {
        let mut placed = false;

        for _ in 0..PLACEMENT_ATTEMPTS {
            if placed {
                break;
            }

            let pos = top_non_colliding_pos(region, world, x, z, entity_type);
            if placement.is_spawn_position_ok(region, pos, entity_type) {
                let position =
                    glam::DVec3::new(f64::from(x) + 0.5, f64::from(pos.y()), f64::from(z) + 0.5);
                if let Some(entity) = ENTITIES.create(
                    entity_type,
                    next_entity_id(),
                    position,
                    Arc::downgrade(world),
                ) {
                    entity.set_rotation((random.next_f32() * 360.0, 0.0));
                    if let Some(mob) = entity.as_mob() {
                        group_data = mob.finalize_spawn(
                            world,
                            EntitySpawnReason::ChunkGeneration,
                            group_data,
                        );
                    }
                    let _ = region.add_fresh_entity(entity);
                    placed = true;
                }
            }

            x += random.next_i32_bounded(PACK_SCATTER) - random.next_i32_bounded(PACK_SCATTER);
            z += random.next_i32_bounded(PACK_SCATTER) - random.next_i32_bounded(PACK_SCATTER);

            while x < min_x || x >= min_x + CHUNK_SIZE || z < min_z || z >= min_z + CHUNK_SIZE {
                x = start_x + random.next_i32_bounded(PACK_SCATTER)
                    - random.next_i32_bounded(PACK_SCATTER);
                z = start_z + random.next_i32_bounded(PACK_SCATTER)
                    - random.next_i32_bounded(PACK_SCATTER);
            }
        }
    }
}

/// Returns the block an animal would stand on in this column.
///
/// Vanilla parity: `NaturalSpawner.getTopNonCollidingPos`. Under a ceiling the
/// heightmap points at the roof, so vanilla digs down to the first open space
/// and then to its floor; that is how a nether dimension gets mobs on the
/// ground rather than embedded in the bedrock lid.
fn top_non_colliding_pos(
    region: &WorldGenRegion<'_>,
    world: &Arc<World>,
    x: i32,
    z: i32,
    entity_type: steel_registry::entity_type::EntityTypeRef,
) -> BlockPos {
    let y = region.height_at(HeightmapType::MotionBlockingNoLeaves, x, z);
    let mut pos = BlockPos::new(x, y, z);

    if world.dimension_type.has_ceiling {
        loop {
            pos = pos.below();
            if region.block_state(pos).is_air() {
                break;
            }
        }
        while region.block_state(pos.below()).is_air() && pos.y() > world.get_min_y() {
            pos = pos.below();
        }
    }

    spawn_placement_for(entity_type).adjust_spawn_position(region, pos)
}

#[cfg(test)]
mod tests {
    use std::borrow::Cow;

    use steel_utils::Identifier;

    use super::*;

    fn entry(path: &'static str, weight: i32) -> SpawnerData {
        SpawnerData {
            entity_type: Identifier {
                namespace: Cow::Borrowed("minecraft"),
                path: Cow::Borrowed(path),
            },
            weight,
            min_count: 1,
            max_count: 1,
        }
    }

    #[test]
    fn weightless_lists_pick_nothing() {
        let candidates = [entry("cow", 0), entry("pig", 0)];
        let mut random = LegacyRandom::from_seed(1);
        assert!(pick_weighted(&candidates, &mut random).is_none());
    }

    #[test]
    fn empty_lists_pick_nothing() {
        let mut random = LegacyRandom::from_seed(1);
        assert!(pick_weighted(&[], &mut random).is_none());
    }

    #[test]
    fn weight_decides_how_often_each_entry_comes_up() {
        // Nine to one, so the split has to be visible rather than merely
        // possible: a picker that ignored weights would land near even.
        let candidates = [entry("cow", 90), entry("pig", 10)];
        let mut random = LegacyRandom::from_seed(7);

        let mut cows = 0;
        for _ in 0..1000 {
            let picked = pick_weighted(&candidates, &mut random).expect("a weighted list picks");
            if picked.entity_type.path == "cow" {
                cows += 1;
            }
        }

        assert!(
            (820..=980).contains(&cows),
            "expected roughly nine in ten cows, got {cows}"
        );
    }

    #[test]
    fn the_same_chunk_is_always_stocked_the_same_way() {
        // Vanilla seeds this pass from the chunk corner so a world regenerates
        // identically. Two randoms seeded alike must agree.
        let mut left = LegacyRandom::from_seed(0);
        let mut right = LegacyRandom::from_seed(0);
        left.set_decoration_seed(1_234, 48, -96);
        right.set_decoration_seed(1_234, 48, -96);

        for _ in 0..16 {
            assert_eq!(left.next_i32_bounded(1_000), right.next_i32_bounded(1_000));
        }
    }
}
