//! Structure block behavior.
//!
//! Vanilla parity: `StructureBlock`. The block itself is thin -- it holds the
//! mode in its state, opens the editor for a gamemaster, and turns a redstone
//! edge into whichever action its mode names. Everything it remembers is in
//! [`StructureBlockEntity`].
//!
//! A redstone pulse triggers `save` and `load`, and both are the halves Steel
//! cannot do yet (see the block entity's own note). Corner mode's `unloadStructure`
//! is a client-side bounding-box hint with no server state, and data mode does
//! nothing on a pulse in vanilla either.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_protocol::packets::game::CBlockEntityData;
use steel_registry::RegistryEntry as _;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::properties::StructureMode;
use steel_registry::vanilla_block_entity_types;
use steel_utils::serial::OptionalNbt;
use steel_utils::{BlockPos, BlockStateId, Downcast as _};

use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::behavior::{InventoryAccess, PlacementSource};
use crate::block_entity::entities::StructureBlockEntity;
use crate::block_entity::{BLOCK_ENTITIES, BlockEntity};
use crate::player::Player;
use crate::world::{LevelReader as _, SignalGetter as _, World};

/// Behavior for the structure block.
#[block_behavior]
pub struct StructureBlock {
    block: BlockRef,
}

impl StructureBlock {
    /// Creates the behavior for the structure block.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for StructureBlock {
    /// Vanilla parity: `StructureBlock` has no `getStateForPlacement`, so it is
    /// placed in the default `LOAD` mode whichever way the player faces.
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
            &vanilla_block_entity_types::STRUCTURE_BLOCK,
            level,
            pos,
            state,
        ))
    }

    /// Vanilla parity: `StructureBlock.setPlacedBy`, which records who placed
    /// the block so a saved structure carries an author.
    fn set_placed_by(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        source: &PlacementSource<'_>,
    ) {
        let Some(player) = source.player() else {
            return;
        };
        let Some(shared) = world.get_block_entity(pos) else {
            return;
        };
        let Some(block_entity) = shared.downcast_ref::<StructureBlockEntity>() else {
            return;
        };
        block_entity.set_author(player.gameprofile.name.clone());
    }

    /// Opens the editor for a gamemaster.
    ///
    /// Vanilla parity: `StructureBlock.useWithoutItem` through
    /// `StructureBlockEntity.usedBy`, which refuses anyone else.
    fn use_without_item(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        if !player.can_use_game_master_blocks() {
            return InteractionResult::Pass;
        }
        let Some(shared) = world.get_block_entity(pos) else {
            return InteractionResult::Pass;
        };
        let Some(block_entity) = shared.downcast_ref::<StructureBlockEntity>() else {
            return InteractionResult::Pass;
        };
        let Some(nbt) = BlockEntity::get_update_tag(block_entity) else {
            return InteractionResult::Pass;
        };
        player.send_packet(CBlockEntityData {
            pos,
            block_entity_type: BlockEntity::get_type(block_entity).id() as i32,
            nbt: OptionalNbt(Some(nbt)),
        });
        InteractionResult::Success
    }

    /// Vanilla parity: `StructureBlock.neighborChanged`, which triggers on the
    /// rising edge only.
    fn handle_neighbor_changed(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source_block: BlockRef,
        _moved_by_piston: bool,
    ) {
        let Some(shared) = world.get_block_entity(pos) else {
            return;
        };
        let Some(block_entity) = shared.downcast_ref::<StructureBlockEntity>() else {
            return;
        };

        let should_trigger = world.has_neighbor_signal(pos);
        let is_powered = block_entity.is_powered();
        if should_trigger && !is_powered {
            block_entity.set_powered(true);
            trigger(block_entity);
        } else if !should_trigger && is_powered {
            block_entity.set_powered(false);
        }
    }
}

/// Runs whatever this block's mode does on a redstone pulse.
///
/// Vanilla parity: `StructureBlock.trigger`.
///
/// **Gap**: `SAVE` needs `StructureTemplate.fillFromWorld` plus a
/// `StructureTemplateManager` to write the `.nbt` file, and `LOAD` needs
/// `StructureTemplate.place_in_world`, which in Steel writes only into a
/// `WorldGenRegion`. Neither exists, so a pulse on those two modes is logged
/// rather than silently doing nothing that looks like success.
fn trigger(block_entity: &StructureBlockEntity) {
    match block_entity.mode() {
        StructureMode::Save => {
            log::debug!(
                "structure block at {:?} was triggered to save, which Steel cannot do yet",
                block_entity.get_block_pos()
            );
        }
        StructureMode::Load => {
            log::debug!(
                "structure block at {:?} was triggered to load, which Steel cannot do yet",
                block_entity.get_block_pos()
            );
        }
        // Vanilla parity: `CORNER` clears the client's bounding-box preview and
        // falls through to `DATA`, which does nothing. Neither has server state.
        StructureMode::Corner | StructureMode::Data => {}
    }
}
