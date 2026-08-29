//! Vanilla `FrogspawnBlock` behavior.

use std::sync::Arc;

use foton_macros::block_behavior;
use foton_protocol::packets::game::SoundSource;
use foton_registry::blocks::BlockRef;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::vanilla_fluid_tags::FluidTag;
use foton_registry::{sound_events, vanilla_blocks, vanilla_entities};
use foton_utils::{BlockPos, BlockStateId, Direction};

use crate::behavior::block::BlockBehavior;
use crate::behavior::context::BlockPlaceContext;
use crate::entity::entities::spawn_tadpoles_from_frogspawn;
use crate::entity::{Entity, InsideBlockEffectCollector};
use crate::fluid::get_fluid_state_from_block;
use crate::world::{LevelReader, ScheduledTickAccess, World};

/// Vanilla `FrogspawnBlock` behavior.
#[block_behavior]
pub struct FrogspawnBlock {
    block: BlockRef,
}

/// Vanilla `FrogspawnBlock.DEFAULT_MIN_HATCH_TICK_DELAY`.
const MIN_HATCH_TICK_DELAY: i32 = 3_600;
/// Vanilla `FrogspawnBlock.DEFAULT_MAX_HATCH_TICK_DELAY`.
const MAX_HATCH_TICK_DELAY: i32 = 12_000;

impl FrogspawnBlock {
    /// Creates a frogspawn behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Vanilla `FrogspawnBlock.mayPlaceOn`: still water below, open air above.
    fn may_place_on(world: &dyn LevelReader, pos: BlockPos) -> bool {
        let below = world.get_block_state(pos);
        let below_fluid = get_fluid_state_from_block(below);
        let above_fluid = get_fluid_state_from_block(world.get_block_state(pos.above()));

        (below_fluid.fluid_id.has_tag(&FluidTag::SUPPORTS_FROGSPAWN)
            || below.get_block().has_tag(&BlockTag::SUPPORTS_FROGSPAWN))
            && above_fluid.is_empty()
    }

    /// Vanilla `FrogspawnBlock.getFrogspawnHatchDelay`.
    fn hatch_delay() -> i32 {
        rand::random_range(MIN_HATCH_TICK_DELAY..MAX_HATCH_TICK_DELAY)
    }

    /// Vanilla `FrogspawnBlock.hatchFrogspawn`.
    fn hatch_frogspawn(world: &Arc<World>, pos: BlockPos) {
        world.destroy_block(pos, false);
        world.play_sound(
            &sound_events::BLOCK_FROGSPAWN_HATCH,
            SoundSource::Blocks,
            pos,
            1.0,
            1.0,
            None,
        );
        spawn_tadpoles_from_frogspawn(world, pos);
    }
}

impl BlockBehavior for FrogspawnBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn can_survive(&self, _state: BlockStateId, world: &dyn LevelReader, pos: BlockPos) -> bool {
        Self::may_place_on(world, pos.below())
    }

    fn on_place(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        world.schedule_block_tick_default(pos, self.block, Self::hatch_delay());
    }

    fn update_shape(
        &self,
        state: BlockStateId,
        world: &dyn ScheduledTickAccess,
        pos: BlockPos,
        _direction: Direction,
        _neighbor_pos: BlockPos,
        _neighbor_state: BlockStateId,
    ) -> BlockStateId {
        if self.can_survive(state, world, pos) {
            state
        } else {
            vanilla_blocks::AIR.default_state()
        }
    }

    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        if self.can_survive(state, world.as_ref(), pos) {
            Self::hatch_frogspawn(world, pos);
            return;
        }

        world.destroy_block(pos, false);
    }

    fn entity_inside(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        entity: &dyn Entity,
        _effect_collector: &mut InsideBlockEffectCollector,
        _is_precise: bool,
    ) {
        if entity.entity_type() == &vanilla_entities::FALLING_BLOCK {
            world.destroy_block(pos, false);
        }
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::blocks::properties::BlockStateProperties;
    use foton_registry::{init_vanilla_registry, vanilla_blocks};

    use super::*;
    use crate::entity::Mob;
    use crate::entity::entities::{MAX_TADPOLES_SPAWN_EXCLUSIVE, MIN_TADPOLES_SPAWN};
    use crate::test_support::TestLevel;

    fn frogspawn() -> BlockStateId {
        vanilla_blocks::FROGSPAWN.default_state()
    }

    #[test]
    fn frogspawn_survives_on_still_water_with_air_above_it() {
        init_vanilla_registry();
        let behavior = FrogspawnBlock::new(&vanilla_blocks::FROGSPAWN);
        let pos = BlockPos::new(0, 64, 0);
        let world =
            TestLevel::default().with_block(pos.below(), vanilla_blocks::WATER.default_state());

        assert!(behavior.can_survive(frogspawn(), &world, pos));
    }

    #[test]
    fn frogspawn_dies_when_its_water_is_taken_away() {
        init_vanilla_registry();
        let behavior = FrogspawnBlock::new(&vanilla_blocks::FROGSPAWN);
        let pos = BlockPos::new(0, 64, 0);
        let world = TestLevel::default();

        assert!(!behavior.can_survive(frogspawn(), &world, pos));
        assert!(
            behavior
                .update_shape(
                    frogspawn(),
                    &world,
                    pos,
                    Direction::Down,
                    pos.below(),
                    vanilla_blocks::AIR.default_state(),
                )
                .is_air()
        );
    }

    #[test]
    fn frogspawn_will_not_stay_once_water_covers_it() {
        init_vanilla_registry();
        let behavior = FrogspawnBlock::new(&vanilla_blocks::FROGSPAWN);
        let pos = BlockPos::new(0, 64, 0);
        // Vanilla calls `mayPlaceOn` with the block below, so its `fluidAbove`
        // check reads the frogspawn's own position: the spawn floats on the
        // surface and cannot sit inside the water column.
        let world = TestLevel::default()
            .with_block(pos.below(), vanilla_blocks::WATER.default_state())
            .with_block(pos, vanilla_blocks::WATER.default_state());

        assert!(!behavior.can_survive(frogspawn(), &world, pos));
    }

    #[test]
    fn a_waterlogged_block_below_also_holds_frogspawn_up() {
        init_vanilla_registry();
        let behavior = FrogspawnBlock::new(&vanilla_blocks::FROGSPAWN);
        let pos = BlockPos::new(0, 64, 0);
        let waterlogged = vanilla_blocks::OAK_SLAB
            .default_state()
            .set_value(&BlockStateProperties::WATERLOGGED, true);
        let world = TestLevel::default().with_block(pos.below(), waterlogged);

        assert!(behavior.can_survive(frogspawn(), &world, pos));
    }

    #[test]
    fn placing_frogspawn_starts_its_hatch_countdown() {
        init_vanilla_registry();
        let delay = FrogspawnBlock::hatch_delay();

        assert!((MIN_HATCH_TICK_DELAY..MAX_HATCH_TICK_DELAY).contains(&delay));
    }

    #[test]
    fn hatching_frogspawn_leaves_two_to_five_tadpoles_in_the_water() {
        // This is the loop: without the spawn the block just disappeared, which
        // is what the comment on this line used to say.
        use foton_registry::vanilla_entities;
        use foton_utils::ChunkPos;
        use foton_utils::types::UpdateFlags;

        use crate::behavior::init_behaviors;
        use crate::entity::init_entities;
        use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

        init_vanilla_registry();
        init_behaviors();
        init_entities();
        let world = fresh_test_world("frogspawn_hatch");
        let pos = BlockPos::new(8, 64, 8);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
        assert!(world.set_block(
            pos.below(),
            vanilla_blocks::WATER.default_state(),
            UpdateFlags::UPDATE_NONE
        ));
        assert!(world.set_block(
            pos,
            vanilla_blocks::FROGSPAWN.default_state(),
            UpdateFlags::UPDATE_NONE
        ));

        let behavior = FrogspawnBlock::new(&vanilla_blocks::FROGSPAWN);
        behavior.tick(frogspawn(), &world, pos);

        assert!(
            world.get_block_state(pos).is_air(),
            "hatching destroys the spawn block"
        );
        let search = foton_utils::WorldAabb::new(
            f64::from(pos.x()) - 4.0,
            f64::from(pos.y()) - 4.0,
            f64::from(pos.z()) - 4.0,
            f64::from(pos.x()) + 5.0,
            f64::from(pos.y()) + 5.0,
            f64::from(pos.z()) + 5.0,
        );
        let tadpoles = world.get_entities_in_aabb_matching(&search, |entity| {
            entity.entity_type() == &vanilla_entities::TADPOLE
        });

        assert!(
            (MIN_TADPOLES_SPAWN..MAX_TADPOLES_SPAWN_EXCLUSIVE).contains(&(tadpoles.len() as i32)),
            "vanilla releases two to five tadpoles, this released {}",
            tadpoles.len()
        );
        assert!(
            tadpoles
                .iter()
                .all(|tadpole| tadpole.as_mob().is_some_and(Mob::is_persistence_required)),
            "a hatched tadpole is persistent, so it does not despawn before growing up"
        );
    }
}
