//! Vanilla `FrostedIceBlock` behavior.

use std::sync::Arc;

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, IntProperty};
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_dimension_types;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId, Direction};

use super::ice_block::{BASE_MELT_LIGHT_LEVEL, IceBlock};
use crate::behavior::block::BlockBehavior;
use crate::behavior::context::BlockPlaceContext;
use crate::block_entity::SharedBlockEntity;
use crate::chunk::light::LightLayer;
use crate::player::Player;
use crate::world::{LevelReader as _, World};

/// Vanilla `FrostedIceBlock`: the ice Frost Walker leaves behind, which ages
/// through four stages before turning back into water.
///
/// It extends `IceBlock`, so melting on block light and the pickaxe drop rules
/// are inherited rather than redefined; only the ageing schedule and the
/// neighbour count are its own.
#[block_behavior]
pub struct FrostedIceBlock {
    block: BlockRef,
    /// The inherited `IceBlock` half of the vanilla class.
    ice: IceBlock,
}

const AGE: &IntProperty = &BlockStateProperties::AGE_3;

/// Vanilla `FrostedIceBlock.MAX_AGE`.
const MAX_AGE: u8 = 3;
/// Vanilla `FrostedIceBlock.NEIGHBORS_TO_AGE`.
const NEIGHBORS_TO_AGE: i32 = 4;
/// Vanilla `FrostedIceBlock.NEIGHBORS_TO_MELT`.
const NEIGHBORS_TO_MELT: i32 = 2;
/// Vanilla `Mth.nextInt(level.getRandom(), 60, 120)` of `onPlace`.
const FIRST_TICK_DELAY: (i32, i32) = (60, 120);
/// Vanilla `Mth.nextInt(random, 20, 40)` of every following schedule.
const NEXT_TICK_DELAY: (i32, i32) = (20, 40);
/// Vanilla `random.nextInt(3) == 0`, the chance a tick tries to melt at all.
const MELT_ATTEMPT_CHANCE: i32 = 3;

/// Vanilla `Mth.nextInt(random, min, max)`, which is inclusive on both ends.
fn next_delay((min, max): (i32, i32)) -> i32 {
    rand::random_range(min..=max)
}

impl FrostedIceBlock {
    /// Creates a frosted ice behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            block,
            ice: IceBlock::new(block),
        }
    }

    /// Vanilla `FrostedIceBlock.slightlyMelt`.
    ///
    /// Returns whether the block melted away rather than merely aged.
    fn slightly_melt(state: BlockStateId, world: &Arc<World>, pos: BlockPos) -> bool {
        let age = state.get_value(AGE);
        if age < MAX_AGE {
            world.set_block(
                pos,
                state.set_value(AGE, age + 1),
                UpdateFlags::UPDATE_CLIENTS,
            );
            return false;
        }

        IceBlock::melt(state, world, pos);
        true
    }

    /// Vanilla `FrostedIceBlock.fewerNeigboursThan`.
    fn fewer_neighbours_than(&self, world: &Arc<World>, pos: BlockPos, limit: i32) -> bool {
        let mut found = 0;
        for direction in Direction::ALL {
            if world.get_block_state(pos.relative(direction)).get_block() == self.block {
                found += 1;
                if found >= limit {
                    return false;
                }
            }
        }

        true
    }

    /// The brightness vanilla compares against, which is block light only in
    /// the End and the full local brightness everywhere else.
    fn melt_brightness(world: &Arc<World>, pos: BlockPos) -> u8 {
        if world.dimension_type == &vanilla_dimension_types::THE_END {
            world.light_value_at(LightLayer::Block, pos)
        } else {
            world.max_local_raw_brightness(pos, world.sky_darkening())
        }
    }

    /// Vanilla `11 - state.getValue(AGE) - state.getLightDampening()`.
    fn melt_light_threshold(state: BlockStateId) -> i32 {
        i32::from(BASE_MELT_LIGHT_LEVEL)
            - i32::from(state.get_value(AGE))
            - i32::from(state.get_light_dampening())
    }
}

impl BlockBehavior for FrostedIceBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        self.ice.get_state_for_placement(context)
    }

    fn on_place(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _old_state: BlockStateId,
        _moved_by_piston: bool,
    ) {
        world.schedule_block_tick_default(pos, self.block, next_delay(FIRST_TICK_DELAY));
    }

    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        if rand::random_range(0..MELT_ATTEMPT_CHANCE) == 0
            || self.fewer_neighbours_than(world, pos, NEIGHBORS_TO_AGE)
        {
            let brightness = Self::melt_brightness(world, pos);
            if i32::from(brightness) > Self::melt_light_threshold(state)
                && Self::slightly_melt(state, world, pos)
            {
                // The melt left a hole, so every frosted neighbour ages with it
                // and the ones that survive get an earlier next look.
                for direction in Direction::ALL {
                    let neighbour_pos = pos.relative(direction);
                    let neighbour = world.get_block_state(neighbour_pos);
                    if neighbour.get_block() == self.block
                        && !Self::slightly_melt(neighbour, world, neighbour_pos)
                    {
                        world.schedule_block_tick_default(
                            neighbour_pos,
                            self.block,
                            next_delay(NEXT_TICK_DELAY),
                        );
                    }
                }

                return;
            }
        }

        world.schedule_block_tick_default(pos, self.block, next_delay(NEXT_TICK_DELAY));
    }

    fn handle_neighbor_changed(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source_block: BlockRef,
        _moved_by_piston: bool,
    ) {
        if source_block == self.block && self.fewer_neighbours_than(world, pos, NEIGHBORS_TO_MELT) {
            IceBlock::melt(state, world, pos);
        }
    }

    fn random_tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        self.ice.random_tick(state, world, pos);
    }

    fn player_destroy(
        &self,
        world: &Arc<World>,
        player: &Player,
        pos: BlockPos,
        state: BlockStateId,
        block_entity: Option<&SharedBlockEntity>,
        tool: &ItemStack,
    ) {
        self.ice
            .player_destroy(world, player, pos, state, block_entity, tool);
    }

    /// Vanilla returns `ItemStack.EMPTY`: frosted ice has no item form.
    fn get_clone_item_stack(
        &self,
        _block: BlockRef,
        _state: BlockStateId,
        _include_data: bool,
    ) -> Option<ItemStack> {
        None
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_blocks};
    use steel_utils::ChunkPos;

    use super::*;
    use crate::behavior::init_behaviors;
    use crate::test_support::{fresh_test_world, insert_ready_full_chunk};

    /// Adds frosted ice to the first `arms` horizontal neighbours of `pos`.
    fn frosted_arms(world: &Arc<World>, pos: BlockPos, arms: usize) {
        let frosted = vanilla_blocks::FROSTED_ICE.default_state();
        for direction in Direction::HORIZONTAL.into_iter().take(arms) {
            world.set_block(pos.relative(direction), frosted, UpdateFlags::UPDATE_NONE);
        }
    }

    #[test]
    fn frosted_ice_ages_before_it_melts_and_reports_which_happened() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("frosted_ice_ages");
        let pos = BlockPos::new(8, 64, 8);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

        let mut state = vanilla_blocks::FROSTED_ICE.default_state();
        assert!(world.set_block(pos, state, UpdateFlags::UPDATE_NONE));

        for expected_age in 1..=MAX_AGE {
            assert!(!FrostedIceBlock::slightly_melt(state, &world, pos));
            state = world.get_block_state(pos);
            assert_eq!(state.get_value(AGE), expected_age);
        }

        assert!(FrostedIceBlock::slightly_melt(state, &world, pos));
        assert_eq!(
            world.get_block_state(pos).get_block(),
            IceBlock::melts_into().get_block()
        );
    }

    #[test]
    fn a_frosted_ice_neighbour_count_is_measured_against_the_vanilla_limits() {
        init_vanilla_registry();
        init_behaviors();
        let world = fresh_test_world("frosted_ice_neighbours");
        let pos = BlockPos::new(8, 64, 8);
        insert_ready_full_chunk(&world, ChunkPos::from_block_pos(pos));

        let behavior = FrostedIceBlock::new(&vanilla_blocks::FROSTED_ICE);
        assert!(world.set_block(
            pos,
            vanilla_blocks::FROSTED_ICE.default_state(),
            UpdateFlags::UPDATE_NONE,
        ));

        frosted_arms(&world, pos, 1);
        assert!(behavior.fewer_neighbours_than(&world, pos, NEIGHBORS_TO_MELT));
        assert!(behavior.fewer_neighbours_than(&world, pos, NEIGHBORS_TO_AGE));

        frosted_arms(&world, pos, 2);
        assert!(!behavior.fewer_neighbours_than(&world, pos, NEIGHBORS_TO_MELT));
        assert!(behavior.fewer_neighbours_than(&world, pos, NEIGHBORS_TO_AGE));

        frosted_arms(&world, pos, 4);
        assert!(!behavior.fewer_neighbours_than(&world, pos, NEIGHBORS_TO_AGE));
    }

    #[test]
    fn the_melt_light_threshold_drops_as_frosted_ice_ages() {
        init_vanilla_registry();
        let fresh = vanilla_blocks::FROSTED_ICE.default_state();
        let oldest = fresh.set_value(AGE, MAX_AGE);

        assert_eq!(
            FrostedIceBlock::melt_light_threshold(fresh)
                - FrostedIceBlock::melt_light_threshold(oldest),
            i32::from(MAX_AGE)
        );
    }
}
