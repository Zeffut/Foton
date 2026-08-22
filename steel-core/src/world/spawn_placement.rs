//! Where a mob is allowed to appear.
//!
//! Vanilla parity: `SpawnPlacementTypes`, plus the per-entity-type table that
//! `SpawnPlacements` builds in code rather than in data. Nearly every mob wants
//! solid ground, so that is the default; the exceptions are listed here exactly
//! as vanilla registers them, because they cannot be derived from the mob
//! category. A drowned is a monster and spawns in water, a strider is a monster
//! and spawns in lava, and a phantom is a monster that spawns in mid-air.

use std::sync::Arc;

use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::entity_type::EntityTypeRef;
use steel_registry::fluid::{is_lava_fluid, is_water_fluid};
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::{vanilla_blocks, vanilla_entities};
use steel_utils::{BlockPos, BlockStateId, Direction};

use crate::behavior::BLOCK_BEHAVIORS;
use crate::entity::ai::walk::WalkPathEvaluator;
use crate::world::signal_getter::SignalQueryContext;
use crate::world::{LevelReader as _, World};

/// Brightest a block may glow and still be spawned on.
///
/// Vanilla parity: the `getLightEmission() < 14` half of the default
/// `BlockBehaviour.Properties.isValidSpawn`.
const MAX_SPAWNABLE_LIGHT_EMISSION: u8 = 14;

/// The kind of place a mob needs in order to appear there.
///
/// Vanilla parity: `SpawnPlacementType`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum SpawnPlacementType {
    /// Anywhere at all, including mid-air.
    ///
    /// Vanilla parity: `SpawnPlacementTypes.NO_RESTRICTIONS`.
    NoRestrictions,
    /// In water, with something other than a solid block overhead.
    ///
    /// Vanilla parity: `SpawnPlacementTypes.IN_WATER`.
    InWater,
    /// In lava.
    ///
    /// Vanilla parity: `SpawnPlacementTypes.IN_LAVA`.
    InLava,
    /// Standing on a block, with room to stand.
    ///
    /// Vanilla parity: `SpawnPlacementTypes.ON_GROUND`.
    OnGround,
}

/// Every mob that does not spawn on the ground.
///
/// Vanilla parity: the non-`ON_GROUND` entries of `SpawnPlacements`, in full,
/// including the mobs Steel has yet to implement. Listing them all now means a
/// mob added later is placed correctly without anyone remembering to come back
/// here.
const PLACEMENT_EXCEPTIONS: &[(EntityTypeRef, SpawnPlacementType)] = &[
    (&vanilla_entities::AXOLOTL, SpawnPlacementType::InWater),
    (&vanilla_entities::COD, SpawnPlacementType::InWater),
    (&vanilla_entities::DROWNED, SpawnPlacementType::InWater),
    (
        &vanilla_entities::ELDER_GUARDIAN,
        SpawnPlacementType::InWater,
    ),
    (&vanilla_entities::GLOW_SQUID, SpawnPlacementType::InWater),
    (&vanilla_entities::GUARDIAN, SpawnPlacementType::InWater),
    (&vanilla_entities::NAUTILUS, SpawnPlacementType::InWater),
    (&vanilla_entities::PUFFERFISH, SpawnPlacementType::InWater),
    (&vanilla_entities::SALMON, SpawnPlacementType::InWater),
    (&vanilla_entities::SQUID, SpawnPlacementType::InWater),
    (
        &vanilla_entities::TROPICAL_FISH,
        SpawnPlacementType::InWater,
    ),
    (&vanilla_entities::STRIDER, SpawnPlacementType::InLava),
    (
        &vanilla_entities::EVOKER,
        SpawnPlacementType::NoRestrictions,
    ),
    (&vanilla_entities::FOX, SpawnPlacementType::NoRestrictions),
    (
        &vanilla_entities::ILLUSIONER,
        SpawnPlacementType::NoRestrictions,
    ),
    (&vanilla_entities::PANDA, SpawnPlacementType::NoRestrictions),
    (
        &vanilla_entities::PHANTOM,
        SpawnPlacementType::NoRestrictions,
    ),
    (
        &vanilla_entities::SHULKER,
        SpawnPlacementType::NoRestrictions,
    ),
    (
        &vanilla_entities::TRADER_LLAMA,
        SpawnPlacementType::NoRestrictions,
    ),
    (&vanilla_entities::VEX, SpawnPlacementType::NoRestrictions),
    (
        &vanilla_entities::VINDICATOR,
        SpawnPlacementType::NoRestrictions,
    ),
    (
        &vanilla_entities::WARDEN,
        SpawnPlacementType::NoRestrictions,
    ),
];

/// Returns where this kind of mob needs to be for a natural spawn.
///
/// Vanilla parity: `SpawnPlacements.getPlacementType`. Vanilla returns
/// `ON_GROUND` for anything it was never told about, and so does this.
#[must_use]
pub fn spawn_placement_for(entity_type: EntityTypeRef) -> SpawnPlacementType {
    PLACEMENT_EXCEPTIONS
        .iter()
        .find(|(candidate, _)| std::ptr::eq(*candidate, entity_type))
        .map_or(SpawnPlacementType::OnGround, |(_, placement)| *placement)
}

impl SpawnPlacementType {
    /// Returns whether a mob of `entity_type` fits at `pos`.
    ///
    /// Vanilla parity: `SpawnPlacementType.isSpawnPositionOk`. The world-border
    /// test vanilla runs first is left to the caller, which already has to
    /// reject positions for being too near or too far from a player.
    #[must_use]
    pub fn is_spawn_position_ok(
        self,
        world: &Arc<World>,
        pos: BlockPos,
        entity_type: EntityTypeRef,
    ) -> bool {
        match self {
            Self::NoRestrictions => true,
            Self::InWater => {
                let above = pos.above();
                is_water_fluid(world.get_block_state(pos).get_fluid_state().fluid_id)
                    && !is_redstone_conductor(world, above)
            }
            Self::InLava => is_lava_fluid(world.get_block_state(pos).get_fluid_state().fluid_id),
            Self::OnGround => {
                let below = pos.below();
                is_valid_spawn_block(world, below)
                    && is_valid_empty_spawn_block(world, pos, entity_type)
                    && is_valid_empty_spawn_block(world, pos.above(), entity_type)
            }
        }
    }
}

/// Returns whether a block can be stood on by a spawning mob.
///
/// Vanilla parity: the default `BlockBehaviour.Properties.isValidSpawn`.
///
/// TODO: the handful of blocks that override `isValidSpawn` -- magma, nether
/// wart blocks, the ones that only take fire-immune mobs -- still answer with
/// the default here, because Steel's block behaviors have no such hook yet.
pub(crate) fn is_valid_spawn_block(world: &Arc<World>, pos: BlockPos) -> bool {
    let state = world.get_block_state(pos);
    world.is_face_sturdy(state, pos, Direction::Up)
        && state.get_light_emission() < MAX_SPAWNABLE_LIGHT_EMISSION
}

/// Returns whether a mob may occupy this block.
///
/// Vanilla parity: `NaturalSpawner.isValidEmptySpawnBlock`.
fn is_valid_empty_spawn_block(
    world: &Arc<World>,
    pos: BlockPos,
    entity_type: EntityTypeRef,
) -> bool {
    let state = world.get_block_state(pos);
    if world.is_collision_shape_full_block_at(pos, state) {
        return false;
    }
    if is_signal_source(state) {
        return false;
    }
    if !state.get_fluid_state().fluid_id.is_empty {
        return false;
    }
    if state
        .get_block()
        .has_tag(&BlockTag::PREVENT_MOB_SPAWNING_INSIDE)
    {
        return false;
    }
    !is_block_dangerous_for(entity_type.fire_immune, state)
}

/// Returns whether the block at `pos` blocks redstone the way a full block does.
///
/// Vanilla parity: the default `BlockBehaviour.Properties.isRedstoneConductor`,
/// which is `isCollisionShapeFullBlock`.
fn is_redstone_conductor(world: &Arc<World>, pos: BlockPos) -> bool {
    world.is_collision_shape_full_block_at(pos, world.get_block_state(pos))
}

/// Returns whether a block emits redstone on its own.
///
/// Vanilla parity: `BlockState.isSignalSource`, which natural spawning uses to
/// keep mobs out of a running circuit. Steel keeps that answer on the block
/// behavior rather than on the state, so it is asked here with the same default
/// context the rest of the redstone code uses.
fn is_signal_source(state: BlockStateId) -> bool {
    BLOCK_BEHAVIORS
        .get_behavior(state.get_block())
        .is_signal_source(state, SignalQueryContext::DEFAULT)
}

/// Returns whether a block would hurt a mob that spawned in it.
///
/// Vanilla parity: `EntityType.isBlockDangerous`, keyed by the entity type
/// rather than by a live entity because natural spawning tests the position
/// before anything is created.
pub(crate) fn is_block_dangerous_for(fire_immune: bool, state: BlockStateId) -> bool {
    // TODO: mirror vanilla EntityType.immuneTo once entity types carry their
    // entity-specific immune block tag.
    if !fire_immune && WalkPathEvaluator::is_burning_block(state) {
        return true;
    }

    let block = state.get_block();
    block == &vanilla_blocks::WITHER_ROSE
        || block == &vanilla_blocks::SWEET_BERRY_BUSH
        || block == &vanilla_blocks::CACTUS
        || block == &vanilla_blocks::POWDER_SNOW
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn unlisted_mobs_spawn_on_the_ground() {
        steel_registry::init_vanilla_registry();
        assert_eq!(
            spawn_placement_for(&vanilla_entities::COW),
            SpawnPlacementType::OnGround
        );
        assert_eq!(
            spawn_placement_for(&vanilla_entities::ZOMBIE),
            SpawnPlacementType::OnGround
        );
    }

    #[test]
    fn water_mobs_are_not_derived_from_their_category() {
        steel_registry::init_vanilla_registry();

        // A drowned is a monster, and a squid is a water creature. Both spawn in
        // water, which is why the table is explicit rather than category-driven.
        assert_eq!(
            spawn_placement_for(&vanilla_entities::DROWNED),
            SpawnPlacementType::InWater
        );
        assert_eq!(
            spawn_placement_for(&vanilla_entities::SQUID),
            SpawnPlacementType::InWater
        );
    }

    #[test]
    fn striders_spawn_in_lava_and_phantoms_anywhere() {
        steel_registry::init_vanilla_registry();
        assert_eq!(
            spawn_placement_for(&vanilla_entities::STRIDER),
            SpawnPlacementType::InLava
        );
        assert_eq!(
            spawn_placement_for(&vanilla_entities::PHANTOM),
            SpawnPlacementType::NoRestrictions
        );
    }

    #[test]
    fn fire_burns_anything_not_immune_to_it() {
        steel_registry::init_vanilla_registry();
        let fire = vanilla_blocks::FIRE.default_state();
        assert!(is_block_dangerous_for(false, fire));
        assert!(!is_block_dangerous_for(true, fire));
    }

    #[test]
    fn cactus_hurts_even_a_fire_immune_mob() {
        steel_registry::init_vanilla_registry();
        let cactus = vanilla_blocks::CACTUS.default_state();
        assert!(is_block_dangerous_for(true, cactus));
    }
}
