//! Whether a mob is allowed to appear where the spawner picked.
//!
//! Vanilla parity: the predicates `SpawnPlacements` registers alongside each
//! entity type — `Mob.checkMobSpawnRules` and the per-mob refinements layered
//! over it. Vanilla can test them before creating anything because it holds a
//! method reference per entity type. Foton reaches a mob's behavior only
//! through an instance, so the spawner creates the mob and then asks it; a mob
//! that answers no is dropped before it ever joins the world.

use std::sync::Arc;

use glam::DVec3;

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::fluid::is_water_fluid;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::vanilla_blocks;
use foton_utils::BlockPos;
use foton_utils::types::GameType;

use crate::entity::EntitySpawnReason;

use crate::world::spawn_placement::is_valid_spawn_block;
use crate::world::{LevelReader as _, World};

/// Deepest a surface water animal spawns below sea level.
///
/// Vanilla parity: the `seaLevel - 13` of
/// `WaterAnimal.checkSurfaceWaterAnimalSpawnRules`.
const SURFACE_WATER_SPAWN_DEPTH: i32 = 13;

/// How close a player may be and still let a silverfish appear.
///
/// Vanilla parity: the `5.0` radius of `Silverfish.checkSilverfishSpawnRules`.
const SILVERFISH_PLAYER_EXCLUSION: f64 = 5.0;

/// Brightest a spot may be for a bat, before the roll.
///
/// Vanilla parity: the `nextInt(4)` bound of `Bat.checkBatSpawnRules`.
const BAT_MAX_BRIGHTNESS_ROLL: u8 = 4;

/// Returns whether the ground beneath `pos` will hold a mob.
///
/// Vanilla parity: `Mob.checkMobSpawnRules`. This is the floor every other rule
/// here builds on, and the only one a mob gets if it asks for nothing more.
#[must_use]
pub fn check_mob_spawn_rules(
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
) -> bool {
    spawn_reason.is_spawner() || is_valid_spawn_block(world, pos.below())
}

/// Returns whether `pos` is dark enough for a monster.
///
/// Vanilla parity: `Monster.isDarkEnoughToSpawn`, without the dimension's
/// configurable light test.
#[must_use]
pub fn is_dark_enough_to_spawn(world: &Arc<World>, pos: BlockPos) -> bool {
    // TODO: honor DimensionType.monsterSpawnBlockLightLimit and
    // monsterSpawnLightTest instead of the fixed vanilla-overworld thresholds.
    let sky_darkening = world.sky_darkening();
    world.raw_brightness(pos, sky_darkening) <= rand::random_range(0..8)
}

/// Returns whether a monster may appear at `pos`.
///
/// Vanilla parity: `Monster.checkMonsterSpawnRules`.
#[must_use]
pub fn check_monster_spawn_rules(
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
) -> bool {
    (spawn_reason.ignores_light_requirements() || is_dark_enough_to_spawn(world, pos))
        && check_mob_spawn_rules(world, spawn_reason, pos)
}

/// Returns whether a monster that ignores light may appear at `pos`.
///
/// Vanilla parity: `Monster.checkAnyLightMonsterSpawnRules`.
#[must_use]
pub fn check_any_light_monster_spawn_rules(
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
) -> bool {
    check_mob_spawn_rules(world, spawn_reason, pos)
}

/// Returns whether a monster that needs open sky may appear at `pos`.
///
/// Vanilla parity: `Monster.checkSurfaceMonstersSpawnRules`, which is what
/// keeps husks out of caves.
#[must_use]
pub fn check_surface_monster_spawn_rules(
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
) -> bool {
    check_monster_spawn_rules(world, spawn_reason, pos)
        && (spawn_reason.is_spawner() || world.can_see_sky(pos))
}

/// Returns whether a fish or squid may appear at `pos`.
///
/// Vanilla parity: `WaterAnimal.checkSurfaceWaterAnimalSpawnRules`, which is
/// character for character the same as
/// `AgeableWaterCreature.checkSurfaceAgeableWaterCreatureSpawnRules`. It keeps
/// them in the top thirteen blocks of the sea, which is why the deep ocean is
/// empty of cod.
#[must_use]
pub fn check_surface_water_animal_spawn_rules(world: &Arc<World>, pos: BlockPos) -> bool {
    let sea_level = world.sea_level;
    pos.y() >= sea_level - SURFACE_WATER_SPAWN_DEPTH
        && pos.y() <= sea_level
        && is_water_fluid(
            world
                .get_block_state(pos.below())
                .get_fluid_state()
                .fluid_id,
        )
        && is_water_fluid(
            world
                .get_block_state(pos.above())
                .get_fluid_state()
                .fluid_id,
        )
}

/// Returns whether a stray may appear at `pos`.
///
/// Vanilla parity: `Stray.checkStraySpawnRules`. A stray needs open sky, but
/// the snow it stands in keeps drifting over it, so vanilla climbs out of the
/// powder snow first and asks from there.
#[must_use]
pub fn check_stray_spawn_rules(
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
) -> bool {
    if !check_monster_spawn_rules(world, spawn_reason, pos) {
        return false;
    }
    if spawn_reason.is_spawner() {
        return true;
    }

    let mut above = pos;
    while world.get_block_state(above.above()).get_block() == &vanilla_blocks::POWDER_SNOW {
        above = above.above();
    }
    world.can_see_sky(above)
}

/// Returns whether a silverfish may appear at `pos`.
///
/// Vanilla parity: `Silverfish.checkSilverfishSpawnRules`. Silverfish ignore
/// light but refuse to appear in sight of a player, which is why they seem to
/// come out of nowhere rather than in front of you.
#[must_use]
pub fn check_silverfish_spawn_rules(
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
) -> bool {
    if !check_any_light_monster_spawn_rules(world, spawn_reason, pos) {
        return false;
    }
    if spawn_reason.is_spawner() {
        return true;
    }
    let (x, y, z) = pos.get_center();
    world
        .nearest_player(DVec3::new(x, y, z), SILVERFISH_PLAYER_EXCLUSION, |player| {
            // Vanilla parity: the `true` argument of `getNearestPlayer` selects
            // `NO_CREATIVE_OR_SPECTATOR`, so neither kind of watching player
            // keeps a silverfish from appearing.
            !matches!(player.game_mode(), GameType::Creative | GameType::Spectator)
        })
        .is_none()
}

/// Returns whether a bat may appear at `pos`.
///
/// Vanilla parity: `Bat.checkBatSpawnRules`. Bats want to be underground, in
/// the dark, on a block bats spawn on, and even then only half the time.
#[must_use]
pub fn check_bat_spawn_rules(
    world: &Arc<World>,
    spawn_reason: EntitySpawnReason,
    pos: BlockPos,
) -> bool {
    if pos.y() >= world.world_surface_height(pos) {
        return false;
    }
    if rand::random::<bool>() {
        return false;
    }
    if world.max_local_raw_brightness(pos, world.sky_darkening())
        > rand::random_range(0..BAT_MAX_BRIGHTNESS_ROLL)
    {
        return false;
    }
    world
        .get_block_state(pos.below())
        .get_block()
        .has_tag(&BlockTag::BATS_SPAWNABLE_ON)
        && check_mob_spawn_rules(world, spawn_reason, pos)
}
