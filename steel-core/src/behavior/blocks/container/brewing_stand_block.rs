//! Brewing stand block behavior.
//!
//! Vanilla parity: `BrewingStandBlock`.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::properties::Direction;
use steel_registry::vanilla_block_entity_types;
use steel_utils::{BlockPos, BlockStateId, Downcast as _, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::entities::BrewingStandBlockEntity;
use crate::block_entity::{BLOCK_ENTITIES, BlockEntityTicker};
use crate::inventory::container::calculate_redstone_signal_from_container;
use crate::inventory::lock::{ContainerLockGuard, ContainerRef};
use crate::inventory::menu::kinds::brewing_stand;
use crate::player::Player;
use crate::world::{LevelReader, World};

/// Behavior for the brewing stand block.
#[block_behavior]
pub struct BrewingStandBlock {
    _block: BlockRef,
}

impl BrewingStandBlock {
    /// Creates the behavior for this block.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { _block: block }
    }
}

impl BlockBehavior for BrewingStandBlock {
    /// A brewing stand has no facing to choose; the three bottle flags are set
    /// by the block entity as bottles come and go.
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        None
    }

    /// Vanilla parity: `BrewingStandBlock.useWithoutItem`.
    fn use_without_item(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let Some(block_entity) = world.get_block_entity(pos) else {
            return InteractionResult::Pass;
        };
        let Some(stand) = block_entity.downcast_ref::<BrewingStandBlockEntity>() else {
            return InteractionResult::Pass;
        };
        let Some(container_ref) = ContainerRef::from_block_entity(block_entity.clone()) else {
            return InteractionResult::Pass;
        };

        let data = stand.data();
        let inventory = player.inventory.clone();
        player.open_menu(
            TextComponent::translated(translations::CONTAINER_BREWING.msg()),
            move |context| brewing_stand(inventory, context.container_id, container_ref, data),
        );

        // TODO: award the INTERACT_WITH_BREWINGSTAND stat once player statistics
        // exist.
        InteractionResult::Success
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::BREWING_STAND,
            level,
            pos,
            state,
        ))
    }

    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::BREWING_STAND,
        )
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    /// Vanilla parity: `BrewingStandBlock.getAnalogOutputSignal`.
    fn get_analog_output_signal(
        &self,
        _state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        let Some(container_ref) = world
            .get_block_entity(pos)
            .and_then(ContainerRef::from_block_entity)
        else {
            return 0;
        };
        let guard = ContainerLockGuard::lock_all(&[&container_ref]);
        guard
            .get(container_ref.container_id())
            .map_or(0, calculate_redstone_signal_from_container)
    }
}
