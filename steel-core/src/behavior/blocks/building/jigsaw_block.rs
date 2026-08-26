//! Jigsaw block behavior.
//!
//! Vanilla parity: `JigsawBlock`. One orientation property, an editor for a
//! gamemaster, and a `canAttach` rule the pool placer uses. Everything the
//! block remembers lives in [`JigsawBlockEntity`].
//!
//! The orientation is a `FrontAndTop` rather than a plain facing, because a
//! jigsaw pointing up or down still has to say which way round the piece it
//! connects sits. Which half of that pair matters where is tested through
//! [`crate::block_entity::entities::default_joint_type`], the one place Steel
//! reads it today.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_protocol::packets::game::CBlockEntityData;
use steel_registry::RegistryEntry as _;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{
    BlockStateProperties, Direction, EnumProperty, FrontAndTop,
};
use steel_registry::vanilla_block_entity_types;
use steel_utils::axis::Axis;
use steel_utils::serial::OptionalNbt;
use steel_utils::{BlockPos, BlockStateId, Downcast as _};

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::entities::JigsawBlockEntity;
use crate::block_entity::{BLOCK_ENTITIES, BlockEntity};
use crate::player::Player;
use crate::world::{LevelReader as _, World};

/// Which way the jigsaw points, and which way is up from its point of view.
const ORIENTATION: &EnumProperty<FrontAndTop> = &BlockStateProperties::ORIENTATION;

/// Behavior for the jigsaw block.
#[block_behavior]
pub struct JigsawBlock {
    block: BlockRef,
}

impl JigsawBlock {
    /// Creates the behavior for the jigsaw block.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for JigsawBlock {
    /// Vanilla parity: `JigsawBlock.getStateForPlacement`. The clicked face is
    /// the front; a jigsaw placed on a floor or ceiling takes its top from the
    /// direction the player was facing, so it lands the way it looked in hand.
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let front = context.clicked_face();
        let top = if front.axis() == Axis::Y {
            context.horizontal_direction().opposite()
        } else {
            Direction::Up
        };
        let orientation = FrontAndTop::from_front_and_top(front, top)?;
        Some(
            self.block
                .default_state()
                .set_value(ORIENTATION, orientation),
        )
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::JIGSAW,
            level,
            pos,
            state,
        ))
    }

    /// Opens the editor for a gamemaster.
    ///
    /// Vanilla parity: `JigsawBlock.useWithoutItem`, whose `openJigsawBlock`
    /// sends the block entity's data to that one player.
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
        let Some(block_entity) = shared.downcast_ref::<JigsawBlockEntity>() else {
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
}
