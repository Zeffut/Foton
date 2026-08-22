//! Natural mob spawning.
//!
//! Vanilla parity: `NaturalSpawner`, with one documented structural difference.
//! Vanilla walks every ticking chunk and applies a per-chunk cap; Steel samples
//! candidate positions in the ring around each player instead, because chunk
//! ticking does not expose the per-chunk mob accounting vanilla relies on. The
//! observable rules that matter are reproduced: the 24-block exclusion around
//! players, the 128-block outer limit, the darkness test, and the biome's own
//! weighted spawn list.

use std::f64::consts::TAU;
use std::sync::Arc;

use glam::DVec3;
use steel_registry::REGISTRY;
use steel_registry::RegistryExt as _;
use steel_registry::biome::SpawnerData;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::entity_type::{EntityTypeRef, MobCategory};
use steel_registry::vanilla_game_rules::{SPAWN_MOBS, SPAWN_MONSTERS};
use steel_utils::{BlockPos, WorldAabb};

use crate::entity::ENTITIES;
use crate::entity::{Entity, next_entity_id};
use crate::world::{LevelReader as _, World};

/// Closest a mob may spawn to a player.
///
/// Vanilla parity: `NaturalSpawner.MIN_SPAWN_DISTANCE`.
const MIN_SPAWN_DISTANCE: f64 = 24.0;

/// Furthest a mob may spawn from a player.
///
/// Vanilla parity: `NaturalSpawner.SPAWN_DISTANCE_BLOCK`.
const MAX_SPAWN_DISTANCE: f64 = 128.0;

/// Candidate positions tried per player each spawn cycle.
const ATTEMPTS_PER_PLAYER: u32 = 16;

/// Ticks between spawn cycles.
///
/// Vanilla runs the spawner every tick over many chunks; Steel samples fewer
/// positions less often for the same rough rate.
const SPAWN_INTERVAL_TICKS: u64 = 20;

/// Monsters allowed within the sampling radius of one player.
///
/// Vanilla derives its cap from `MobCategory::max_instances_per_chunk` scaled by
/// the number of spawnable chunks; this is the equivalent local budget.
const MONSTER_CAP_PER_PLAYER: usize = 24;

impl World {
    /// Runs one natural spawning cycle.
    ///
    /// Vanilla parity: `NaturalSpawner.spawnForChunk`, applied per player.
    pub fn tick_natural_spawn(self: &Arc<Self>, tick_count: u64) {
        if !tick_count.is_multiple_of(SPAWN_INTERVAL_TICKS) {
            return;
        }
        if !self.get_game_rule(&SPAWN_MOBS) || !self.get_game_rule(&SPAWN_MONSTERS) {
            return;
        }

        let mut player_positions = Vec::new();
        self.players.iter_players(|_, player| {
            player_positions.push(player.position());
            true
        });

        for origin in player_positions {
            if self.monsters_near(origin) >= MONSTER_CAP_PER_PLAYER {
                continue;
            }
            for _ in 0..ATTEMPTS_PER_PLAYER {
                if let Some(pos) = self.pick_spawn_position(origin) {
                    self.try_spawn_at(pos);
                }
            }
        }
    }

    /// Counts monsters already alive within the sampling radius.
    fn monsters_near(self: &Arc<Self>, origin: DVec3) -> usize {
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
            .filter(|entity| entity.entity_type().mob_category == MobCategory::Monster)
            .count()
    }

    /// Picks a candidate position in the spawn ring around `origin`.
    ///
    /// Returns `None` when the sampled column has no ground a mob could stand on.
    fn pick_spawn_position(self: &Arc<Self>, origin: DVec3) -> Option<BlockPos> {
        let angle = rand::random::<f64>() * TAU;
        let distance = (MAX_SPAWN_DISTANCE - MIN_SPAWN_DISTANCE)
            .mul_add(rand::random::<f64>(), MIN_SPAWN_DISTANCE);
        let x = distance.mul_add(angle.cos(), origin.x).floor() as i32;
        let z = distance.mul_add(angle.sin(), origin.z).floor() as i32;

        // Search a vertical band around the player rather than the whole column.
        let center_y = origin.y.floor() as i32;
        for offset in -16..=16 {
            let y = center_y + offset;
            let pos = BlockPos::new(x, y, z);
            if self.is_valid_spawn_position(pos) {
                return Some(pos);
            }
        }
        None
    }

    /// Returns whether a mob fits at `pos` with solid ground beneath it.
    fn is_valid_spawn_position(self: &Arc<Self>, pos: BlockPos) -> bool {
        if self.is_outside_build_height(pos.y()) {
            return false;
        }
        let below = self.get_block_state(pos.below());
        if !below.is_solid() {
            return false;
        }
        // A mob needs its own block and headroom, and must not spawn inside a block.
        self.get_block_state(pos).is_air() && self.get_block_state(pos.above()).is_air()
    }

    /// Spawns one mob at `pos` when the light and distance rules allow it.
    fn try_spawn_at(self: &Arc<Self>, pos: BlockPos) {
        let center = DVec3::new(
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()),
            f64::from(pos.z()) + 0.5,
        );

        // Vanilla refuses any position closer than 24 blocks to a player.
        if let Some(distance_sqr) = self.nearest_player_distance_sqr(center)
            && distance_sqr < MIN_SPAWN_DISTANCE * MIN_SPAWN_DISTANCE
        {
            return;
        }

        if !self.is_dark_enough_to_spawn(pos) {
            return;
        }

        let Some(entity_type) = self.pick_monster_for(pos) else {
            return;
        };
        let Some(entity) =
            ENTITIES.create(entity_type, next_entity_id(), center, Arc::downgrade(self))
        else {
            return;
        };

        if let Err(error) = self.try_add_entity(entity) {
            log::debug!("natural spawn rejected: {error}");
        }
    }

    /// Returns whether `pos` is dark enough for a monster.
    ///
    /// Vanilla parity: `Monster.isDarkEnoughToSpawn`, without the dimension's
    /// configurable light test.
    fn is_dark_enough_to_spawn(self: &Arc<Self>, pos: BlockPos) -> bool {
        // TODO: honor DimensionType.monsterSpawnBlockLightLimit and
        // monsterSpawnLightTest instead of the fixed vanilla-overworld thresholds.
        let sky_darkening = self.sky_darkening();
        self.raw_brightness(pos, sky_darkening) <= rand::random_range(0..8)
    }

    /// Picks a monster from the biome's weighted spawn list.
    ///
    /// Vanilla parity: `NaturalSpawner.getRandomSpawnMobAt`.
    fn pick_monster_for(self: &Arc<Self>, pos: BlockPos) -> Option<EntityTypeRef> {
        let biome = self.biome_at(pos)?;
        let candidates: &Vec<SpawnerData> = biome.spawners.get("monster")?;

        let total: i32 = candidates.iter().map(|entry| entry.weight).sum();
        if total <= 0 {
            return None;
        }

        let mut roll = rand::random_range(0..total);
        for entry in candidates {
            roll -= entry.weight;
            if roll < 0 {
                // Only entity types Steel actually implements can spawn; the rest
                // are skipped until their entity exists.
                return REGISTRY.entity_types.by_key(&entry.entity_type);
            }
        }
        None
    }
}
