//! Sculk shrieker behavior.
//!
//! Vanilla parity: `SculkShriekerBlock`.
//!
//! A shrieker hears through the block entity's vibration listener as well as through
//! `stepOn`, which is why walking onto one and setting off a sculk sensor beside one both
//! start a shriek.
//!
//! The shriek itself lasts ninety ticks; what answers it is on
//! `SculkShriekerBlockEntity`, which is where the warning level climbs and where a warden
//! is summoned from.

use std::sync::{Arc, Weak};

use foton_macros::block_behavior;
use foton_registry::block_entity_type::BlockEntityTypeRef;
use foton_registry::blocks::BlockRef;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::{BlockStateProperties, BoolProperty, Direction};
use foton_registry::fluid::FluidStateExt as _;
use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_block_entity_types;
use foton_utils::types::UpdateFlags;
use foton_utils::value_providers::IntProvider;
use foton_utils::{BlockPos, BlockStateId, Downcast as _};

use crate::behavior::block::{
    BlockBehavior, BlockEntityCreation, schedule_water_tick_if_waterlogged,
};
use crate::behavior::context::BlockPlaceContext;
use crate::behavior::try_drop_experience;
use crate::block_entity::entities::{SculkShriekerBlockEntity, with_shrieking_player};
use crate::block_entity::{BLOCK_ENTITIES, BlockEntityTicker};
use crate::entity::Entity;
use crate::fluid::get_fluid_state;
use crate::world::{ScheduledTickAccess, World};

/// Vanilla `SculkShriekerBlock.spawnAfterBreak` uses `ConstantInt.of(5)`.
const SHRIEKER_EXPERIENCE: IntProvider = IntProvider::Constant(5);

const SHRIEKING: &BoolProperty = &BlockStateProperties::SHRIEKING;
const WATERLOGGED: &BoolProperty = &BlockStateProperties::WATERLOGGED;

/// Vanilla `SculkShriekerBlock`.
#[block_behavior]
pub struct SculkShriekerBlock {
    block: BlockRef,
}

impl SculkShriekerBlock {
    /// Creates the sculk shrieker behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    fn with_shrieker<R>(
        world: &Arc<World>,
        pos: BlockPos,
        action: impl FnOnce(&SculkShriekerBlockEntity) -> R,
    ) -> Option<R> {
        let block_entity = world.get_block_entity(pos)?;
        let shrieker = block_entity.downcast_ref::<SculkShriekerBlockEntity>()?;
        Some(action(shrieker))
    }
}

impl BlockBehavior for SculkShriekerBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let waterlogged = get_fluid_state(context.world, context.place_pos()).is_water();
        Some(
            self.block
                .default_state()
                .set_value(WATERLOGGED, waterlogged),
        )
    }

    /// Vanilla `SculkShriekerBlock.stepOn`.
    fn step_on(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos, entity: &dyn Entity) {
        with_shrieking_player(entity, |player| {
            Self::with_shrieker(world, pos, |shrieker| shrieker.try_shriek(world, player));
        });
        self.default_step_on(state, world, pos, entity);
    }

    /// Vanilla `SculkShriekerBlock.tick`.
    ///
    /// The shriek lasts ninety ticks; when it ends the shrieker answers itself, which is
    /// the step that would summon a warden in vanilla.
    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        if !state.get_value(SHRIEKING) {
            return;
        }

        world.set_block(
            pos,
            state.set_value(SHRIEKING, false),
            UpdateFlags::UPDATE_ALL,
        );
        Self::with_shrieker(world, pos, |shrieker| shrieker.try_respond(world));
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
        schedule_water_tick_if_waterlogged(state, world, pos);
        state
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::SCULK_SHRIEKER,
            level,
            pos,
            state,
        ))
    }

    /// Vanilla `SculkShriekerBlock.getTicker`, which only ticks the vibration system.
    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::SCULK_SHRIEKER,
        )
    }

    fn spawn_after_break(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        tool: &ItemStack,
        drop_experience: bool,
    ) {
        if drop_experience {
            try_drop_experience(world, pos, tool, &SHRIEKER_EXPERIENCE);
        }
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::{init_vanilla_registry, vanilla_blocks};
    use foton_utils::ChunkPos;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::block_entity::init_block_entities;
    use crate::test_support::{TestPlayerBuilder, fresh_test_world, insert_ready_full_chunk};

    /// Block states are registry lookups, so nothing in a test may name one before this.
    fn init() {
        init_vanilla_registry();
        init_behaviors();
        init_block_entities();
    }

    fn world_with_shrieker(name: &'static str, pos: BlockPos, state: BlockStateId) -> Arc<World> {
        let world = fresh_test_world(name);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));
        assert!(world.set_block(pos, state, UpdateFlags::UPDATE_ALL));
        world
    }

    /// Walking onto a shrieker is what starts it, and the shriek has to end on its own
    /// ninety ticks later. A shrieker that never cleared `shrieking` would be deaf from
    /// then on, since `tryShriek` refuses to start a second one.
    #[test]
    fn a_player_stepping_on_a_shrieker_starts_a_shriek_that_ends_itself() {
        init();
        let pos = BlockPos::new(8, 70, 8);
        let quiet = vanilla_blocks::SCULK_SHRIEKER.default_state();
        let world = world_with_shrieker("sculk_shrieker_step", pos, quiet);
        let behavior = SculkShriekerBlock::new(&vanilla_blocks::SCULK_SHRIEKER);
        let player = TestPlayerBuilder::new(Arc::clone(&world), "stepper", 1).build();

        behavior.step_on(quiet, &world, pos, player.as_ref());

        let shrieking = world.get_block_state(pos);
        assert!(shrieking.get_value(SHRIEKING));
        assert!(world.has_scheduled_block_tick(pos, &vanilla_blocks::SCULK_SHRIEKER));

        behavior.tick(shrieking, &world, pos);

        assert!(!world.get_block_state(pos).get_value(SHRIEKING));
    }

    /// Vanilla will not start a second shriek over a running one; the block state is the
    /// only guard, so it has to be read from the world rather than from the caller.
    #[test]
    fn a_shrieker_already_shrieking_does_not_start_again() {
        init();
        let pos = BlockPos::new(8, 70, 8);
        let shrieking = vanilla_blocks::SCULK_SHRIEKER
            .default_state()
            .set_value(SHRIEKING, true);
        let world = world_with_shrieker("sculk_shrieker_reentry", pos, shrieking);
        let behavior = SculkShriekerBlock::new(&vanilla_blocks::SCULK_SHRIEKER);
        let player = TestPlayerBuilder::new(Arc::clone(&world), "stepper", 1).build();

        behavior.step_on(shrieking, &world, pos, player.as_ref());

        assert!(!world.has_scheduled_block_tick(pos, &vanilla_blocks::SCULK_SHRIEKER));
    }
}
