//! The vault block.
//!
//! Vanilla parity: `net.minecraft.world.level.block.VaultBlock`.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, VaultState};
use steel_registry::vanilla_block_entity_types;
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, Downcast as _};

use crate::behavior::InteractionResult;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InventoryAccess};
use crate::block_entity::vault::VaultBlockEntity;
use crate::block_entity::{BLOCK_ENTITIES, BlockEntityTicker};
use crate::player::Player;
use crate::world::World;

/// Vanilla `VaultBlock`.
#[block_behavior]
pub struct VaultBlock {
    block: BlockRef,
}

impl VaultBlock {
    /// Creates the vault block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for VaultBlock {
    /// Vanilla parity: `VaultBlock.getStateForPlacement`, which faces the vault
    /// back at whoever placed it.
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(self.block.default_state().set_value(
            &BlockStateProperties::HORIZONTAL_FACING,
            context.horizontal_direction().opposite(),
        ))
    }

    /// Vanilla parity: `VaultBlock.useItemOn`.
    fn use_item_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hand: InteractionHand,
        _hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        if state.try_get_value(&BlockStateProperties::VAULT_STATE) != Some(VaultState::Active) {
            return InteractionResult::TryEmptyHandInteraction;
        }

        let held = inv.with_item(|item| item.copy_with_count(item.count()));
        if held.is_empty() {
            return InteractionResult::TryEmptyHandInteraction;
        }

        let Some(block_entity) = world.get_block_entity(pos) else {
            return InteractionResult::TryEmptyHandInteraction;
        };
        let Some(vault) = block_entity.downcast_ref::<VaultBlockEntity>() else {
            return InteractionResult::TryEmptyHandInteraction;
        };

        if vault.try_insert_key(world, pos, player, &held) {
            // Vanilla shrinks the key inside `tryInsertKey`; Steel does it here
            // because the held stack lives behind the inventory guard the block
            // behavior owns, and `tryInsertKey` must not take that lock.
            let taken = held.count().min(vault.key_item_count());
            inv.with_item(|item| item.shrink(taken));
        }

        InteractionResult::SuccessServer
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::VAULT,
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
            &vanilla_block_entity_types::VAULT,
        )
    }
}
