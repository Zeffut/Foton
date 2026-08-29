//! Beacon block behavior.
//!
//! Vanilla parity: `BeaconBlock`. The block itself does almost nothing -- it
//! owns a block entity and opens a menu. The pyramid count, the sky check and
//! the effects all live on the block entity.

use std::sync::{Arc, Weak};

use foton_macros::block_behavior;
use foton_registry::block_entity_type::BlockEntityTypeRef;
use foton_registry::blocks::BlockRef;
use foton_registry::vanilla_block_entity_types;
use foton_utils::{BlockPos, BlockStateId, Downcast as _, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::entities::BeaconBlockEntity;
use crate::block_entity::{BLOCK_ENTITIES, BlockEntityTicker};
use crate::inventory::menu::kinds::beacon;
use crate::player::Player;
use crate::world::{LevelReader as _, World};

/// Behavior for the beacon block.
#[block_behavior]
pub struct BeaconBlock {
    block: BlockRef,
}

impl BeaconBlock {
    /// Creates the beacon behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for BeaconBlock {
    fn get_state_for_placement(&self, _context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state())
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::BEACON,
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
            &vanilla_block_entity_types::BEACON,
        )
    }

    /// Vanilla parity: `BeaconBlock.useWithoutItem`.
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
        let Some(data) = block_entity
            .downcast_ref::<BeaconBlockEntity>()
            .map(BeaconBlockEntity::data)
        else {
            return InteractionResult::Pass;
        };

        let inventory = player.inventory.clone();
        player.open_menu(
            block_entity.display_name(TextComponent::translated(
                translations::CONTAINER_BEACON.msg(),
            )),
            move |context| {
                beacon(
                    inventory,
                    context.container_id,
                    Arc::clone(&data),
                    Arc::clone(&block_entity),
                )
            },
        );

        // TODO: Award stat INTERACT_WITH_BEACON; Foton has no statistics
        // registry.
        InteractionResult::Success
    }
}
