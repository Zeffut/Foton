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
use crate::entity::{Entity, EntitySpawnReason, next_entity_id};
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

/// Monsters allowed within the sampling radius of one player.
///
/// Every other category's budget is derived from this one, so that the ratios
/// between categories stay vanilla's even though the absolute figure is Foton's
/// own. See [`category_cap`].
const MONSTER_CAP_PER_PLAYER: i32 = 24;

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
/// Vanilla scales `MobCategory.getMaxInstancesPerChunk` by the number of
/// spawnable chunks around the players. Foton samples a fixed ring instead, so
/// the per-chunk density becomes a flat local budget: the monster figure is the
/// one Foton already used, and every other category keeps vanilla's ratio to
/// it. That is what makes a ring hold far more zombies than squid.
fn category_cap(category: MobCategory) -> usize {
    let max = category.max_instances_per_chunk();
    if max <= 0 {
        return 0;
    }
    let scaled = max * MONSTER_CAP_PER_PLAYER / MobCategory::Monster.max_instances_per_chunk();
    scaled.max(1) as usize
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
                if !category.is_friendly() && !spawn_monsters {
                    continue;
                }
                if category.is_persistent() && !spawn_persistent {
                    continue;
                }
                if self.mobs_near(origin, category) >= category_cap(category) {
                    continue;
                }
                for _ in 0..ATTEMPTS_PER_PLAYER {
                    if let Some(pos) = self.pick_spawn_position(origin) {
                        self.try_spawn_at(pos, category);
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

    /// Spawns one mob of `category` at `pos` when every rule allows it.
    ///
    /// Vanilla parity: `NaturalSpawner.spawnCategoryForPosition`, in the same
    /// order: reject the position, pick the mob, then ask the mob.
    fn try_spawn_at(self: &Arc<Self>, pos: BlockPos, category: MobCategory) {
        let (center_x, center_y, center_z) = pos.get_bottom_center();
        let center = DVec3::new(center_x, center_y, center_z);

        // Vanilla refuses any position closer than 24 blocks to a player.
        if let Some(distance_sqr) = self.nearest_player_distance_sqr(center)
            && distance_sqr < MIN_SPAWN_DISTANCE * MIN_SPAWN_DISTANCE
        {
            return;
        }

        let Some(entity_type) = self.pick_mob_for(pos, category) else {
            return;
        };

        if !spawn_placement_for(entity_type).is_spawn_position_ok(self, pos, entity_type) {
            return;
        }

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
        if !mob.check_spawn_rules(self, EntitySpawnReason::Natural, pos) {
            return;
        }

        // Vanilla parity: `NaturalSpawner.spawnCategoryForPosition` finalizes the
        // mob before it joins the world, which is what gives it its biome variant
        // and its spawn-time attributes. Foton spawns one mob at a time rather
        // than a pack, so there is no group data to thread from a previous mob.
        let _ = mob.finalize_spawn(self, EntitySpawnReason::Natural, None);

        let mut spawn_event = crate::event::CreatureSpawnEvent::new(
            entity.uuid(),
            self.key.to_string(),
            entity.position().x,
            entity.position().y,
            entity.position().z,
            "Natural".to_owned(),
        );
        self.fire_event(&mut spawn_event);
        if spawn_event.is_cancelled() {
            return;
        }

        if let Err(error) = self.try_add_entity(entity) {
            log::debug!("natural spawn rejected: {error}");
        }
    }

    /// Picks a mob of `category` from the biome's weighted spawn list.
    ///
    /// Vanilla parity: `NaturalSpawner.getRandomSpawnMobAt`.
    fn pick_mob_for(
        self: &Arc<Self>,
        pos: BlockPos,
        category: MobCategory,
    ) -> Option<EntityTypeRef> {
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
                return REGISTRY.entity_types.by_key(&entry.entity_type);
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
                category_cap(category) > 0,
                "{category:?} would never spawn anything"
            );
        }
    }

    #[test]
    fn misc_never_spawns() {
        assert_eq!(category_cap(MobCategory::Misc), 0);
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
    fn budgets_keep_vanillas_ordering_between_categories() {
        // Vanilla lets a chunk hold seven times as many monsters as animals, and
        // twice as many drifting fish as animals. The ring budgets are smaller
        // but must not reshuffle that ordering.
        assert!(category_cap(MobCategory::Monster) > category_cap(MobCategory::Ambient));
        assert!(category_cap(MobCategory::Ambient) > category_cap(MobCategory::Creature));
        assert!(category_cap(MobCategory::WaterAmbient) > category_cap(MobCategory::WaterCreature));
    }
}
