//! Bell behavior.
//!
//! Vanilla parity: `BellBlock`. Hitting a bell rings it, but only from a side
//! it can actually swing towards -- a floor bell hangs on an axis and will not
//! ring if struck end-on. Redstone rings it too, once per rising edge.
//!
//! Ringing is only the start of it. The swing, the sixty-block sweep that tells
//! every mob nearby the bell rang, and the resonance that gives away the
//! raiders standing in it all live on [`BellBlockEntity`]; this block is the
//! part that decides whether a hit counts and hands the ring over to it.

use std::sync::{Arc, Weak};

use foton_macros::block_behavior;
use foton_registry::block_entity_type::BlockEntityTypeRef;
use foton_registry::blocks::BlockRef;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::{
    BellAttachType, BlockStateProperties, BoolProperty, Direction, EnumProperty,
};
use foton_registry::blocks::shapes::SupportType;
use foton_registry::{sound_events, vanilla_block_entity_types, vanilla_game_events};
use foton_utils::axis::Axis;
use foton_utils::types::UpdateFlags;
use foton_utils::{BlockPos, BlockStateId, Downcast as _};

use crate::behavior::block::BlockBehavior;
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::behavior::{BlockEntityCreation, InventoryAccess};
use crate::block_entity::entities::BellBlockEntity;
use crate::block_entity::{BLOCK_ENTITIES, BlockEntityTicker};
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

    /// Rings the bell at `pos`, if there is one there to ring.
    ///
    /// Vanilla parity: `BellBlock.attemptToRing`, both overloads -- `direction`
    /// of `None` is the `@Nullable Direction` it falls back to the bell's own
    /// facing for. It is an associated function because vanilla's is an
    /// instance method that never touches the instance: everything it needs it
    /// reads back out of the level.
    ///
    /// Returns whether there was a bell block entity to ring, which is what
    /// tells a player whether the swing earned them the statistic.
    pub fn attempt_to_ring(
        world: &Arc<World>,
        pos: BlockPos,
        direction: Option<Direction>,
        ringer: Option<&dyn Entity>,
    ) -> bool {
        let Some(block_entity) = world.get_block_entity(pos) else {
            return false;
        };
        let Some(bell) = block_entity.downcast_ref::<BellBlockEntity>() else {
            return false;
        };
        let direction = direction.unwrap_or_else(|| world.get_block_state(pos).get_value(FACING));
        bell.on_hit(world, direction);
        world.play_block_sound(&sound_events::BLOCK_BELL_USE, pos, BELL_VOLUME, 1.0, None);
        world.game_event(
            &vanilla_game_events::BLOCK_CHANGE,
            pos,
            &GameEventContext::new(ringer, None),
        );
        true
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

        Self::attempt_to_ring(world, pos, Some(hit_result.direction), Some(player));
        // TODO: Award stat BELL_RING; Foton has no statistics registry.
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
            Self::attempt_to_ring(world, pos, None, None);
        }
        world.set_block(
            pos,
            state.set_value(POWERED, signal),
            UpdateFlags::UPDATE_ALL,
        );
    }

    /// Vanilla parity: `BellBlock.newBlockEntity`.
    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::BELL,
            level,
            pos,
            state,
        ))
    }

    /// Vanilla parity: the server half of `BellBlock.getTicker`.
    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::BELL,
        )
    }

    /// Vanilla parity: `BaseEntityBlock.triggerEvent`, which forwards the event
    /// to the block entity -- the swing counter and the raider sweep both start
    /// from there rather than from the block.
    fn trigger_event(
        &self,
        _state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        param_a: i32,
        param_b: i32,
    ) -> bool {
        world
            .get_block_entity(pos)
            .is_some_and(|block_entity| block_entity.trigger_event(param_a, param_b))
    }
}
