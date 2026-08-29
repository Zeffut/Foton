//! Building a wither out of soul sand and skulls.
//!
//! Vanilla parity: the pattern half of `WitherSkullBlock` -- `checkSpawn` and
//! the `BlockPattern` it searches with. It lives beside the skull rather than
//! inside it because the skull file already carries the seven block classes
//! that share a block entity.
//!
//! Vanilla's other pattern, the headless `getOrCreateWitherBase` behind
//! `canSpawnMob`, is not here: its only caller is the dispenser's skull
//! behavior, and Foton has no `DispenseItemBehavior` registry yet. It belongs
//! in this module when that lands.

use std::sync::{Arc, LazyLock};

use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::{vanilla_blocks, vanilla_entities};
use foton_utils::axis::Axis;
use foton_utils::types::Difficulty;
use foton_utils::{BlockPos, BlockStateId, Downcast as _};
use glam::DVec3;

use crate::behavior::blocks::CarvedPumpkinBlock;
use crate::entity::entities::WitherBoss;
use crate::entity::{ENTITIES, next_entity_id};
use crate::world::World;
use crate::world::block_pattern::{
    BlockPattern, BlockPatternBuilder, BlockPatternMatch, has_state,
};

#[cfg(test)]
mod tests;

/// How high above the block the pattern was anchored on the wither appears.
///
/// Vanilla parity: the `spawnPos.getY() + 0.55` of `checkSpawn`.
const SPAWN_Y_OFFSET: f64 = 0.55;

/// Returns whether a state is one of the two wither skeleton skulls.
fn is_wither_skull(state: BlockStateId) -> bool {
    let block = state.get_block();
    block == &vanilla_blocks::WITHER_SKELETON_SKULL
        || block == &vanilla_blocks::WITHER_SKELETON_WALL_SKULL
}

/// Vanilla parity: `WitherSkullBlock.getOrCreateWitherFull`.
static WITHER_FULL: LazyLock<BlockPattern> = LazyLock::new(|| {
    BlockPatternBuilder::start()
        .aisle(&["^^^", "###", "~#~"])
        .where_char(
            '#',
            has_state(|state| {
                state
                    .get_block()
                    .has_tag(&BlockTag::WITHER_SUMMON_BASE_BLOCKS)
            }),
        )
        .where_char('^', has_state(is_wither_skull))
        .where_char('~', has_state(|state| state.is_air()))
        .build()
});

/// Turns a finished wither frame into the wither.
///
/// Vanilla parity: `WitherSkullBlock.checkSpawn`. The advancement trigger
/// vanilla fires for every player within fifty blocks has no equivalent in
/// Foton yet.
pub fn check_wither_spawn(world: &Arc<World>, pos: BlockPos) {
    if !is_wither_skull(world.get_block_state(pos)) {
        return;
    }
    if pos.y() < world.get_min_y() || world.difficulty() == Difficulty::Peaceful {
        return;
    }

    let Some(found) = WITHER_FULL.find(world.as_ref(), pos) else {
        return;
    };

    // Vanilla reads the spawn position out of the pattern before it clears it,
    // because clearing replaces those blocks with air.
    let spawn_pos = found.block(1, 2, 0).pos();
    let y_rot = if found.forwards().axis() == Axis::X {
        0.0
    } else {
        90.0
    };
    let position = DVec3::new(
        f64::from(spawn_pos.x()) + 0.5,
        f64::from(spawn_pos.y()) + SPAWN_Y_OFFSET,
        f64::from(spawn_pos.z()) + 0.5,
    );

    let Some(wither) = ENTITIES.create(
        &vanilla_entities::WITHER,
        next_entity_id(),
        position,
        Arc::downgrade(world),
    ) else {
        return;
    };

    clear_and_update(world, &found, || {
        wither.set_rotation((y_rot, 0.0));
        if let Some(living) = wither.as_living_entity() {
            living.set_y_body_rot(y_rot);
        }
        if let Some(boss) = wither.downcast_ref::<WitherBoss>() {
            boss.make_invulnerable();
        }
        if let Err(error) = world.try_add_entity(Arc::clone(&wither)) {
            log::debug!("wither could not be summoned: {error}");
        }
    });
}

/// Eats the frame, runs the summon, then tells the neighbours the frame is
/// gone.
///
/// Vanilla parity: the `clearPatternBlocks` / `addFreshEntity` /
/// `updatePatternBlocks` order of `checkSpawn`, which matters because the
/// neighbour updates must not run while the wither is still being placed.
fn clear_and_update(world: &Arc<World>, found: &BlockPatternMatch<'_>, summon: impl FnOnce()) {
    CarvedPumpkinBlock::clear_pattern_blocks(world, found);
    summon();
    CarvedPumpkinBlock::update_pattern_blocks(world, found);
}
