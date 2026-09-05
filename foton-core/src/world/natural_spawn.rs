//! Natural mob spawning.
//!
//! Vanilla parity: `NaturalSpawner`, with one documented structural difference.
//! Vanilla walks every ticking chunk and applies a per-chunk cap; Foton samples
//! candidate positions in the ring around each player instead, because chunk
//! ticking does not expose the per-chunk mob accounting vanilla relies on. The
//! observable rules that matter are reproduced: the 24-block exclusion around
//! players, the 128-block outer limit, the per-category budget, where each kind
//! of mob is allowed to stand, and the spawn rules the mob itself imposes.

use std::f64::consts::TAU;
use std::sync::Arc;

use foton_registry::REGISTRY;
use foton_registry::RegistryExt as _;
use foton_registry::biome::SpawnerData;
use foton_registry::entity_type::{EntityTypeRef, MobCategory};
use foton_registry::vanilla_game_rules::{SPAWN_MOBS, SPAWN_MONSTERS};
use foton_utils::{BlockPos, WorldAabb};
use glam::DVec3;

use crate::entity::ENTITIES;
use crate::entity::{Entity, EntitySpawnReason, SpawnGroupData, next_entity_id};
use crate::event::{CreatureSpawnEvent, PreCreatureSpawnEvent};
use crate::world::World;
use crate::world::spawn_placement::spawn_placement_for;

/// Closest a mob may spawn to a player.
///
/// Vanilla parity: `NaturalSpawner.MIN_SPAWN_DISTANCE`.
const MIN_SPAWN_DISTANCE: f64 = 24.0;

/// Furthest a mob may spawn from a player.
///
/// Vanilla parity: `NaturalSpawner.SPAWN_DISTANCE_BLOCK`.
const MAX_SPAWN_DISTANCE: f64 = 128.0;

/// Candidate positions tried per category, per player, each spawn cycle.
const ATTEMPTS_PER_PLAYER: u32 = 16;

/// Groups attempted at one sampled position.
///
/// Vanilla parity: the `for (groupCount = 0; groupCount < 3; groupCount++)` of
/// `NaturalSpawner.spawnCategoryForPosition`.
const GROUPS_PER_POSITION: u32 = 3;

/// Ticks between spawn cycles.
///
/// Vanilla runs the spawner every tick over many chunks; Foton samples fewer
/// positions less often for the same rough rate.
const SPAWN_INTERVAL_TICKS: u64 = 20;

/// Ticks between attempts at the categories that stay loaded.
///
/// Vanilla parity: the `getGameTime() % 400L == 0L` of
/// `ServerChunkCache.tickChunks`. Creatures are persistent, so vanilla offers
/// them a turn only every four hundred ticks; that slow cadence is why the
/// countryside does not silt up with cows, and why most animals a player meets
/// were placed when the chunk was made rather than spawned later.
const PERSISTENT_SPAWN_INTERVAL_TICKS: u64 = 400;

/// Chunks covered by the box [`World::mobs_near`] counts mobs in.
///
/// The box is `MAX_SPAWN_DISTANCE` in every direction, so 256 blocks on a side,
/// so sixteen chunks on a side.
const SAMPLED_CHUNK_COUNT: i32 = 16 * 16;

/// Vanilla's reference area for a category budget.
///
/// Vanilla parity: `NaturalSpawner.MAGIC_NUMBER`, seventeen chunks squared.
/// `maxInstancesPerChunk` is a count per that area, not per chunk, which is why
/// dividing by it is what turns the category figure into a density.
const SPAWN_CAP_REFERENCE_CHUNKS: i32 = 17 * 17;

/// Categories the spawner populates, in vanilla's declaration order.
///
/// `Misc` is absent: it is the category of boats and item frames, and vanilla
/// gives it a negative per-chunk maximum precisely so that nothing spawns it.
const SPAWNABLE_CATEGORIES: [MobCategory; 7] = [
    MobCategory::Monster,
    MobCategory::Creature,
    MobCategory::Ambient,
    MobCategory::Axolotls,
    MobCategory::UndergroundWaterCreature,
    MobCategory::WaterCreature,
    MobCategory::WaterAmbient,
];

/// Returns how many mobs of `category` may live in one player's spawn ring.
///
/// Vanilla parity: `SpawnState.canSpawnForCategoryGlobal` allows
/// `maxInstancesPerChunk * spawnableChunkCount / MAGIC_NUMBER` mobs across
/// every chunk that can spawn. That is a density, and Foton applies the same
/// density to the box it actually counts in.
///
/// The figure this replaced was a flat twenty-four monsters, which is 0.12 per
/// chunk against vanilla's 0.24 -- half the ceiling. In the End that is plainly
/// visible, because the enderman is the whole monster table there and nothing
/// else takes up the slack.
fn default_category_cap(category: MobCategory) -> usize {
    let max = category.max_instances_per_chunk();
    if max <= 0 {
        return 0;
    }
    let scaled = max * SAMPLED_CHUNK_COUNT / SPAWN_CAP_REFERENCE_CHUNKS;
    scaled.max(1) as usize
}

/// Rolls how many mobs one group asks for, from the biome entry's own counts.
///
/// Vanilla parity: the `max = spawnerData.minCount() + random.nextInt(1 +
/// spawnerData.maxCount() - spawnerData.minCount())` of
/// `NaturalSpawner.spawnCategoryForPosition`.
///
/// These two counts sit in every biome file and were parsed into
/// [`SpawnerData`] all along; the spawner simply never read them and spawned
/// one mob per success. The End states `minCount: 4, maxCount: 4` for the
/// enderman, so a quarter of the endermen vanilla places were appearing.
fn roll_pack_size(min_count: i32, max_count: i32) -> i32 {
    let span = (max_count - min_count).max(0) + 1;
    min_count + rand::random_range(0..span)
}

impl World {
    /// Runs one natural spawning cycle.
    ///
    /// Vanilla parity: `NaturalSpawner.spawnForChunk`, applied per player.
    pub fn tick_natural_spawn(self: &Arc<Self>, tick_count: u64) {
        if !tick_count.is_multiple_of(SPAWN_INTERVAL_TICKS) {
            return;
        }
        if !self.get_game_rule(&SPAWN_MOBS) {
            return;
        }
        let spawn_monsters = self.get_game_rule(&SPAWN_MONSTERS);
        let spawn_persistent = tick_count.is_multiple_of(PERSISTENT_SPAWN_INTERVAL_TICKS);

        let mut player_positions = Vec::new();
        self.players.iter_players(|_, player| {
            player_positions.push(player.position());
            true
        });

        for origin in player_positions {
            for category in SPAWNABLE_CATEGORIES {
                // Vanilla parity: `NaturalSpawner.getFilteredSpawningCategories`
                // drops the monsters when `doMobSpawning` is off, and drops the
                // persistent categories on every tick but the four-hundredth.
                if (!category.is_friendly() && (!spawn_monsters || !self.allow_monsters()))
                    || (category.is_friendly() && !self.allow_animals())
                {
                    continue;
                }
                if category.is_persistent() && !spawn_persistent {
                    continue;
                }
                let interval = self
                    .spawn_ticks(category)
                    .unwrap_or(if category.is_persistent() { 400 } else { 20 });
                if interval == 0 || !tick_count.is_multiple_of(interval as u64) {
                    continue;
                }
                let cap = self
                    .spawn_limit(category)
                    .unwrap_or_else(|| default_category_cap(category) as i32)
                    .max(0) as usize;
                // Deliberately stricter than vanilla, which reads the category
                // ceiling once per tick in `getFilteredSpawningCategories` and
                // then lets that tick overshoot it freely -- its `state::canSpawn`
                // is the spawn-cost predicate, not the ceiling. Vanilla can afford
                // that because it walks chunks; Foton samples a ring per player,
                // so an unchecked cycle would place a pack at every one of its
                // sixteen positions and blow past the ceiling in one go.
                let mut budget = cap.saturating_sub(self.mobs_near(origin, category));
                if budget == 0 {
                    continue;
                }
                for _ in 0..ATTEMPTS_PER_PLAYER {
                    if budget == 0 {
                        break;
                    }
                    if let Some(pos) = self.pick_spawn_position(origin) {
                        self.try_spawn_at(pos, category, &mut budget);
                    }
                }
            }
        }
    }

    /// Counts mobs of `category` already alive within the sampling radius.
    fn mobs_near(self: &Arc<Self>, origin: DVec3, category: MobCategory) -> usize {
        let aabb = WorldAabb::new(
            origin.x - MAX_SPAWN_DISTANCE,
            origin.y - MAX_SPAWN_DISTANCE,
            origin.z - MAX_SPAWN_DISTANCE,
            origin.x + MAX_SPAWN_DISTANCE,
            origin.y + MAX_SPAWN_DISTANCE,
            origin.z + MAX_SPAWN_DISTANCE,
        );
        self.get_entities_in_aabb(&aabb)
            .iter()
            .filter(|entity| entity.entity_type().mob_category == category)
            .count()
    }

    /// Picks a candidate position in the spawn ring around `origin`.
    ///
    /// Vanilla parity: `NaturalSpawner.getRandomPosWithin`, which picks a column
    /// and a height without judging either. Whether the spot suits the mob is
    /// decided later, by that mob's placement type, because the answer differs
    /// for a cow and for a cod.
    fn pick_spawn_position(self: &Arc<Self>, origin: DVec3) -> Option<BlockPos> {
        let angle = rand::random::<f64>() * TAU;
        let distance = (MAX_SPAWN_DISTANCE - MIN_SPAWN_DISTANCE)
            .mul_add(rand::random::<f64>(), MIN_SPAWN_DISTANCE);
        let x = distance.mul_add(angle.cos(), origin.x).floor() as i32;
        let z = distance.mul_add(angle.sin(), origin.z).floor() as i32;

        // Search a vertical band around the player rather than the whole column.
        let center_y = origin.y.floor() as i32;
        let y = center_y + rand::random_range(-16..=16);
        if self.is_outside_build_height(y) {
            return None;
        }
        Some(BlockPos::new(x, y, z))
    }

    /// Fills one sampled position with a pack, the way vanilla fills a chosen one.
    ///
    /// Vanilla parity: `NaturalSpawner.spawnCategoryForPosition`. Three groups
    /// are attempted at the position. Each draws one biome entry, takes its pack
    /// size from that entry, and walks a short random offset per mob so the pack
    /// lands as a cluster. `group_data` is threaded from each mob to the next,
    /// which is what lets a pack share one variant or one leader.
    ///
    /// `budget` is the category's remaining headroom and is spent as mobs join,
    /// standing in for the running count vanilla keeps in `SpawnState`.
    fn try_spawn_at(self: &Arc<Self>, pos: BlockPos, category: MobCategory, budget: &mut usize) {
        let mut cluster_size = 0;

        for _ in 0..GROUPS_PER_POSITION {
            let mut x = pos.x();
            let mut z = pos.z();
            let mut entry: Option<(EntityTypeRef, i32, i32)> = None;
            let mut group_data: Option<SpawnGroupData> = None;
            let mut group_size = 0;
            // Vanilla opens with a one-to-four budget and replaces it with the
            // biome entry's own count the moment an entry is drawn.
            let mut remaining = (rand::random::<f32>() * 4.0).ceil() as i32;
            let mut attempt = 0;

            while attempt < remaining {
                attempt += 1;
                x += rand::random_range(0..6) - rand::random_range(0..6);
                z += rand::random_range(0..6) - rand::random_range(0..6);
                let candidate = BlockPos::new(x, pos.y(), z);
                let (center_x, center_y, center_z) = candidate.get_bottom_center();
                let center = DVec3::new(center_x, center_y, center_z);

                // Vanilla refuses any position closer than 24 blocks to a player.
                if let Some(distance_sqr) = self.nearest_player_distance_sqr(center)
                    && distance_sqr < MIN_SPAWN_DISTANCE * MIN_SPAWN_DISTANCE
                {
                    continue;
                }

                if entry.is_none() {
                    let Some(drawn) = self.pick_spawner_entry(candidate, category) else {
                        break;
                    };
                    let (_, min_count, max_count) = drawn;
                    remaining = roll_pack_size(min_count, max_count);
                    entry = Some(drawn);
                }
                let Some((entity_type, _, _)) = entry else {
                    break;
                };

                if !spawn_placement_for(entity_type).is_spawn_position_ok(
                    self,
                    candidate,
                    entity_type,
                ) {
                    continue;
                }

                let mut pre_spawn = PreCreatureSpawnEvent::new(
                    self.key.to_string(),
                    center.x,
                    center.y,
                    center.z,
                    entity_type.key.to_string(),
                    "Natural".to_owned(),
                );
                self.fire_event(&mut pre_spawn);
                if pre_spawn.is_cancelled() {
                    continue;
                }

                // Vanilla parity: a `getMobForSpawn` that comes back null ends the
                // whole attempt, not just this group -- a type that cannot be built
                // will not build on the next try either.
                let Some(entity) =
                    ENTITIES.create(entity_type, next_entity_id(), center, Arc::downgrade(self))
                else {
                    return;
                };

                // Vanilla asks the entity type's registered predicate before creating
                // anything. Foton has no path from a type to its behavior, so the mob is
                // created and asked; one that answers no is dropped here, unspawned.
                let Some(mob) = entity.as_mob() else {
                    return;
                };
                if !mob.check_spawn_rules(self, EntitySpawnReason::Natural, candidate) {
                    continue;
                }

                group_data = mob.finalize_spawn(self, EntitySpawnReason::Natural, group_data);

                let mut spawn_event = CreatureSpawnEvent::new(
                    entity.uuid(),
                    self.key.to_string(),
                    entity.position().x,
                    entity.position().y,
                    entity.position().z,
                    "Natural".to_owned(),
                );
                self.begin_pending_spawn(Arc::clone(&entity));
                self.fire_event(&mut spawn_event);
                self.end_pending_spawn(&entity.uuid());
                if spawn_event.is_cancelled() {
                    continue;
                }
                if let Err(error) = self.try_add_entity(Arc::clone(&entity)) {
                    log::debug!("natural spawn rejected: {error}");
                    continue;
                }

                cluster_size += 1;
                group_size += 1;
                *budget -= 1;
                if *budget == 0 || cluster_size >= mob.max_spawn_cluster_size() {
                    return;
                }
                if mob.is_max_group_size_reached(group_size) {
                    break;
                }
            }
        }
    }

    /// Picks a mob of `category` from the biome's weighted spawn list.
    ///
    /// Vanilla parity: `NaturalSpawner.getRandomSpawnMobAt`.
    /// Chooses one biome spawn entry for `pos`, weighted as vanilla weights it.
    ///
    /// Vanilla parity: `NaturalSpawner.getRandomSpawnMobAt`. The pack size
    /// travels with the entry: vanilla reads `minCount`/`maxCount` off the very
    /// entry it just drew, and spawning one mob where the biome asked for four
    /// is what made the End look empty.
    fn pick_spawner_entry(
        self: &Arc<Self>,
        pos: BlockPos,
        category: MobCategory,
    ) -> Option<(EntityTypeRef, i32, i32)> {
        let biome = self.biome_at(pos)?;
        let candidates: &Vec<SpawnerData> = biome.spawners.get(category.name())?;

        let total: i32 = candidates.iter().map(|entry| entry.weight).sum();
        if total <= 0 {
            return None;
        }

        let mut roll = rand::random_range(0..total);
        for entry in candidates {
            roll -= entry.weight;
            if roll < 0 {
                // Only entity types Foton actually implements can spawn; the rest
                // are skipped until their entity exists.
                let entity_type = REGISTRY.entity_types.by_key(&entry.entity_type)?;
                return Some((entity_type, entry.min_count, entry.max_count));
            }
        }
        None
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_spawnable_category_gets_a_budget() {
        for category in SPAWNABLE_CATEGORIES {
            assert!(
                default_category_cap(category) > 0,
                "{category:?} would never spawn anything"
            );
        }
    }

    #[test]
    fn misc_never_spawns() {
        assert_eq!(default_category_cap(MobCategory::Misc), 0);
        assert!(!SPAWNABLE_CATEGORIES.contains(&MobCategory::Misc));
    }

    #[test]
    fn persistent_categories_wait_four_hundred_ticks() {
        // Creatures are the persistent category, and offering them a turn every
        // cycle rather than every four hundred ticks would spawn twenty times
        // the animals vanilla does.
        assert!(MobCategory::Creature.is_persistent());
        assert!(!MobCategory::Monster.is_persistent());
        assert_eq!(
            PERSISTENT_SPAWN_INTERVAL_TICKS % SPAWN_INTERVAL_TICKS,
            0,
            "the persistent cadence has to land on a spawn cycle or it never fires"
        );
    }

    #[test]
    fn every_category_is_either_friendly_or_a_monster() {
        // The monster gate is written against `is_friendly` so that a category
        // added later is refused by default rather than silently let through.
        for category in SPAWNABLE_CATEGORIES {
            let gated = !category.is_friendly();
            assert_eq!(gated, category == MobCategory::Monster);
        }
    }

    #[test]
    fn pack_size_honors_the_biome_entry() {
        // The End asks for `minCount: 4, maxCount: 4` endermen. A spawner that
        // ignores the entry and places one mob per success -- which is what
        // Foton did -- puts a quarter of vanilla's endermen in the world, and
        // the enderman is the whole monster table there.
        for _ in 0..64 {
            assert_eq!(roll_pack_size(4, 4), 4);
        }

        // A range still has to stay inside itself, both ends included.
        let mut seen_low = false;
        let mut seen_high = false;
        for _ in 0..512 {
            let rolled = roll_pack_size(1, 4);
            assert!(
                (1..=4).contains(&rolled),
                "pack size {rolled} escaped the entry's range"
            );
            seen_low |= rolled == 1;
            seen_high |= rolled == 4;
        }
        assert!(seen_low && seen_high, "the range's ends are unreachable");
    }

    #[test]
    fn budgets_match_vanillas_density() {
        // Vanilla allows `maxInstancesPerChunk` mobs per 289 chunks. Foton
        // counts mobs in a fixed box instead of walking chunks, so the budget
        // has to carry that same density across -- the flat twenty-four
        // monsters this replaced was half of it, and half a ceiling is what a
        // player reads as an empty world.
        for category in SPAWNABLE_CATEGORIES {
            let vanilla = f64::from(category.max_instances_per_chunk())
                / f64::from(SPAWN_CAP_REFERENCE_CHUNKS);
            let foton = default_category_cap(category) as f64 / f64::from(SAMPLED_CHUNK_COUNT);
            assert!(
                (foton - vanilla).abs() < 0.01,
                "{category:?} holds {foton:.3} mobs per chunk against vanilla's {vanilla:.3}"
            );
        }
    }

    #[test]
    fn budgets_keep_vanillas_ordering_between_categories() {
        // Vanilla lets a chunk hold seven times as many monsters as animals, and
        // twice as many drifting fish as animals. The ring budgets are smaller
        // but must not reshuffle that ordering.
        assert!(
            default_category_cap(MobCategory::Monster) > default_category_cap(MobCategory::Ambient)
        );
        assert!(
            default_category_cap(MobCategory::Ambient)
                > default_category_cap(MobCategory::Creature)
        );
        assert!(
            default_category_cap(MobCategory::WaterAmbient)
                > default_category_cap(MobCategory::WaterCreature)
        );
    }
}
