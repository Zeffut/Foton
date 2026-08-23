//! Bell behavior.
//!
//! Vanilla parity: `BellBlock`. Hitting a bell rings it, but only from a side
//! it can actually swing towards -- a floor bell hangs on an axis and will not
//! ring if struck end-on. Redstone rings it too, once per rising edge.
//!
//! Not implemented: the raid part. Vanilla's bell reveals nearby raiders for
//! three seconds, and Steel has no raids.
//!
//! Nor is there a block entity. Vanilla keeps one to hold the swing timer and
//! the raid scan; the swing itself is a block event the client animates, and
//! with no raids there is nothing left for the block entity to remember.

use std::sync::Arc;

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{
    BellAttachType, BlockStateProperties, BoolProperty, Direction, EnumProperty,
};
use steel_registry::blocks::shapes::SupportType;
use steel_registry::{sound_events, vanilla_game_events};
use steel_utils::axis::Axis;
use steel_utils::types::UpdateFlags;
use steel_utils::{BlockPos, BlockStateId};

use crate::behavior::InventoryAccess;
use crate::behavior::block::BlockBehavior;
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::entity::Entity;
use crate::player::Player;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader as _, SignalGetter as _, World};

/// How the bell is hung.
const ATTACHMENT: &EnumProperty<BellAttachType> = &BlockStateProperties::BELL_ATTACHMENT;

/// Which way it faces.
const FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;

/// Whether redstone is currently holding it.
const POWERED: &BoolProperty = &BlockStateProperties::POWERED;

/// How loud a bell is.
///
/// Vanilla parity: the `2.0F` of `attemptToRing` -- twice as loud as most
/// blocks, which is the point of a bell.
const BELL_VOLUME: f32 = 2.0;

/// Above this height on the block, a side hit counts as hitting the top.
///
/// Vanilla parity: the `0.8124F` of `isProperHit`. Striking the very top of a
/// bell does not swing it.
const TOP_HIT_HEIGHT: f64 = 0.812_4;

/// Vanilla parity: the `1` event id of `BellBlockEntity`, which starts the
/// swing on the client.
const RING_EVENT: i32 = 1;

/// Behavior for the bell.
#[block_behavior]
pub struct BellBlock {
    block: BlockRef,
}

impl BellBlock {
    /// Creates a bell behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Returns whether a hit from `direction` at `hit_height` can swing it.
    ///
    /// Vanilla parity: `BellBlock.isProperHit`. A bell swings on one axis, and
    /// which axis depends on how it is hung: a floor bell swings along its
    /// facing, a wall bell across it.
    fn is_proper_hit(state: BlockStateId, direction: Direction, hit_height: f64) -> bool {
        if direction.axis() == Axis::Y || hit_height > TOP_HIT_HEIGHT {
            return false;
        }

        let facing = state.get_value(FACING);
        match state.get_value(ATTACHMENT) {
            BellAttachType::Floor => facing.axis() == direction.axis(),
            BellAttachType::SingleWall | BellAttachType::DoubleWall => {
                facing.axis() != direction.axis()
            }
            BellAttachType::Ceiling => true,
        }
    }

    /// Rings the bell.
    ///
    /// Vanilla parity: `BellBlock.attemptToRing`.
    pub fn ring(
        &self,
        world: &Arc<World>,
        pos: BlockPos,
        direction: Direction,
        ringer: Option<&dyn Entity>,
    ) {
        world.block_event(pos, self.block, RING_EVENT, direction.get_3d_data_value());
        world.play_block_sound(&sound_events::BLOCK_BELL_USE, pos, BELL_VOLUME, 1.0, None);
        world.game_event(
            &vanilla_game_events::BLOCK_CHANGE,
            pos,
            &GameEventContext::new(ringer, None),
        );
    }
}

impl BlockBehavior for BellBlock {
    /// Vanilla parity: `BellBlock.getStateForPlacement`.
    ///
    /// A bell hung between two walls reads as `DoubleWall` and swings the
    /// other way from one hung on a single wall.
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        let clicked_face = context.clicked_face();
        let pos = context.place_pos();
        let world = context.world;

        let (attachment, facing) = match clicked_face {
            Direction::Up => (BellAttachType::Floor, context.horizontal_direction()),
            Direction::Down => (BellAttachType::Ceiling, context.horizontal_direction()),
            side => {
                let opposite = side.opposite();
                let has_other_wall = world
                    .get_block_state(pos.relative(side))
                    .is_face_sturdy_for_at(pos.relative(side), opposite, SupportType::Full);
                let attachment = if has_other_wall {
                    BellAttachType::DoubleWall
                } else {
                    BellAttachType::SingleWall
                };
                (attachment, opposite)
            }
        };

        Some(
            self.block
                .default_state()
                .set_value(ATTACHMENT, attachment)
                .set_value(FACING, facing),
        )
    }

    /// Vanilla parity: `BellBlock.useWithoutItem`.
    fn use_without_item(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        let hit_height = hit_result.location.y - f64::from(pos.y());
        if !Self::is_proper_hit(state, hit_result.direction, hit_height) {
            return InteractionResult::Pass;
        }

        self.ring(world, pos, hit_result.direction, Some(player));
        // TODO: Award stat BELL_RING; Steel has no statistics registry.
        InteractionResult::Success
    }

    /// Vanilla parity: `BellBlock.neighborChanged`, which rings once on the
    /// rising edge rather than continuously while powered.
    fn handle_neighbor_changed(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _source_block: BlockRef,
        _moved_by_piston: bool,
    ) {
        let signal = world.has_neighbor_signal(pos);
        if signal == state.get_value(POWERED) {
            return;
        }
        if signal {
            self.ring(world, pos, state.get_value(FACING), None);
        }
        world.set_block(
            pos,
            state.set_value(POWERED, signal),
            UpdateFlags::UPDATE_ALL,
        );
    }
}
