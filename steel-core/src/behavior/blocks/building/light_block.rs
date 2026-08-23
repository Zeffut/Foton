//! Vanilla `LightBlock` behavior.

use std::collections::BTreeMap;
use std::sync::Arc;

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, Direction, IntProperty};
use steel_registry::data_components::components::BlockItemStateProperties;
use steel_registry::data_components::vanilla_components::BLOCK_STATE;
use steel_registry::item_stack::ItemStack;
use steel_registry::{REGISTRY, RegistryExt as _};
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId};

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, schedule_water_tick_if_waterlogged};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::player::Player;
use crate::world::{ScheduledTickAccess, World};

/// Vanilla `LightBlock`: the invisible light source.
///
/// Its emission and its skylight behavior come from the extracted per-state
/// light properties. What the server still owns is the operator-only level
/// cycling, the water tick every waterloggable block schedules, and carrying
/// the level into the picked item.
#[block_behavior]
pub struct LightBlock {
    block: BlockRef,
}

const LEVEL: &IntProperty = &BlockStateProperties::LEVEL;

impl LightBlock {
    /// Creates a light block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Vanilla `Player.canUseGameMasterBlocks`.
    ///
    /// Vanilla asks for creative-mode instant build plus permission level two;
    /// Steel expresses the second half as membership of the operator group.
    fn can_use_game_master_blocks(player: &Player) -> bool {
        player.abilities.lock().instabuild && player.is_operator()
    }

    /// Vanilla `state.cycle(LEVEL)`.
    fn cycle_level(state: BlockStateId) -> BlockStateId {
        let level = state.get_value(LEVEL);
        let next = if level == LEVEL.max {
            LEVEL.min
        } else {
            level + 1
        };
        state.set_value(LEVEL, next)
    }

    /// Vanilla `LightBlock.setLightOnStack`.
    fn set_light_on_stack(mut stack: ItemStack, level: u8) -> ItemStack {
        stack.set(
            BLOCK_STATE,
            BlockItemStateProperties::new(BTreeMap::from([(
                LEVEL.name.to_owned(),
                level.to_string(),
            )])),
        );
        stack
    }
}

impl BlockBehavior for LightBlock {
    /// Vanilla `LightBlock` does not override `getStateForPlacement`, so a light
    /// block placed into water is not waterlogged; only a bucket poured onto one
    /// waterlogs it.
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn use_without_item(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        if !Self::can_use_game_master_blocks(player) {
            return InteractionResult::Consume;
        }

        world.set_block(pos, Self::cycle_level(state), UpdateFlags::UPDATE_CLIENTS);
        InteractionResult::SuccessServer
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

    fn get_clone_item_stack(
        &self,
        block: BlockRef,
        state: BlockStateId,
        _include_data: bool,
    ) -> Option<ItemStack> {
        let stack = REGISTRY.items.by_key(&block.key).map(ItemStack::new)?;
        Some(Self::set_light_on_stack(stack, state.get_value(LEVEL)))
    }
}

#[cfg(test)]
mod tests {
    use steel_registry::{init_vanilla_registry, vanilla_blocks};

    use super::*;

    #[test]
    fn cycling_a_light_level_wraps_from_the_brightest_back_to_dark() {
        init_vanilla_registry();
        let mut state = vanilla_blocks::LIGHT.default_state();
        assert_eq!(state.get_value(LEVEL), LEVEL.max);

        state = LightBlock::cycle_level(state);
        assert_eq!(state.get_value(LEVEL), LEVEL.min);

        state = LightBlock::cycle_level(state);
        assert_eq!(state.get_value(LEVEL), LEVEL.min + 1);
    }

    #[test]
    fn a_picked_light_block_carries_its_level_in_the_block_state_component() {
        init_vanilla_registry();
        let behavior = LightBlock::new(&vanilla_blocks::LIGHT);
        let state = vanilla_blocks::LIGHT.default_state().set_value(LEVEL, 7);

        let picked = behavior
            .get_clone_item_stack(&vanilla_blocks::LIGHT, state, false)
            .expect("light has an item form");

        assert_eq!(
            picked
                .get(BLOCK_STATE)
                .as_ref()
                .and_then(|properties| properties.get(LEVEL.name)),
            Some("7")
        );
    }
}
