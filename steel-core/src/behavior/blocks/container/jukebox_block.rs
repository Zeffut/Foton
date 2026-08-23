//! Jukebox behavior.
//!
//! Vanilla parity: `JukeboxBlock`. Right-click with a disc to put it in,
//! right-click empty-handed to get it back. The block itself decides almost
//! nothing; the disc, the timing and the two redstone answers all live in
//! [`JukeboxBlockEntity`].

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::block_entity_type::BlockEntityTypeRef;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::blocks::properties::{BlockStateProperties, BoolProperty, Direction};
use steel_registry::data_components::vanilla_components;
use steel_registry::{vanilla_block_entity_types, vanilla_game_events};
use steel_utils::types::InteractionHand;
use steel_utils::{BlockPos, BlockStateId, Downcast as _};

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::entities::JukeboxBlockEntity;
use crate::block_entity::{BLOCK_ENTITIES, BlockEntityTicker};
use crate::player::Player;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader, SignalQueryContext, World};

/// Whether the jukebox is holding a disc.
const HAS_RECORD: &BoolProperty = &BlockStateProperties::HAS_RECORD;

/// What a playing jukebox gives out.
///
/// Vanilla parity: the `15` of `JukeboxBlock.ownSignal`.
const PLAYING_SIGNAL: i32 = 15;

/// Behavior for the jukebox.
#[block_behavior]
pub struct JukeboxBlock {
    block: BlockRef,
}

impl JukeboxBlock {
    /// Creates a jukebox behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Returns the jukebox at `pos`, if there is one.
    fn with_jukebox<T>(
        world: &dyn LevelReader,
        pos: BlockPos,
        f: impl FnOnce(&JukeboxBlockEntity) -> T,
    ) -> Option<T> {
        let entity = world.get_block_entity(pos)?;
        let jukebox = entity.downcast_ref::<JukeboxBlockEntity>()?;
        Some(f(jukebox))
    }
}

impl BlockBehavior for JukeboxBlock {
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
            &vanilla_block_entity_types::JUKEBOX,
            level,
            pos,
            state,
        ))
    }

    /// Vanilla parity: `JukeboxBlock.getTicker`.
    ///
    /// Vanilla only attaches the ticker while `has_record` is set, since an
    /// empty jukebox has nothing to count. Steel ticks it either way: the tick
    /// returns immediately with no song, and skipping the state check keeps the
    /// ticker from having to be re-attached the moment a disc goes in.
    fn get_block_entity_ticker(
        &self,
        _world: &Arc<World>,
        _state: BlockStateId,
        block_entity_type: BlockEntityTypeRef,
    ) -> Option<BlockEntityTicker> {
        BlockEntityTicker::for_matching_entity_tick(
            block_entity_type,
            &vanilla_block_entity_types::JUKEBOX,
        )
    }

    /// Vanilla parity: `JukeboxBlock.useWithoutItem`, which ejects the disc.
    fn use_without_item(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        _player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        if !state.get_value(HAS_RECORD) {
            return InteractionResult::Pass;
        }
        if Self::with_jukebox(world.as_ref(), pos, |jukebox| jukebox.pop_out_item(world)).is_none()
        {
            return InteractionResult::Pass;
        }
        InteractionResult::Success
    }

    /// Vanilla parity: `JukeboxBlock.useItemOn` and
    /// `JukeboxPlayable.tryInsertIntoJukebox`. Anything that is not a disc
    /// falls through to the empty-handed path rather than being swallowed.
    fn use_item_on(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        hand: InteractionHand,
        _hit_result: &BlockHitResult,
        inv: &mut InventoryAccess,
    ) -> InteractionResult {
        if state.get_value(HAS_RECORD) {
            return InteractionResult::TryEmptyHandInteraction;
        }

        let held = {
            let inventory = player.inventory.lock();
            let stack = inventory.get_item_in_hand(hand);
            stack.copy_with_count(1)
        };
        if held.get(vanilla_components::JUKEBOX_PLAYABLE).is_none() {
            return InteractionResult::TryEmptyHandInteraction;
        }
        if Self::with_jukebox(world.as_ref(), pos, |jukebox| jukebox.insert(world, held)).is_none()
        {
            return InteractionResult::TryEmptyHandInteraction;
        }

        if !player.has_infinite_materials() {
            inv.with_item(|stack| stack.shrink(1));
        }

        world.game_event(
            &vanilla_game_events::BLOCK_CHANGE,
            pos,
            &GameEventContext::new(Some(player), None),
        );

        // TODO: Award stat PLAY_RECORD; Steel has no statistics registry.
        InteractionResult::Success
    }

    /// Vanilla parity: `JukeboxBlock.isSignalSource`.
    fn is_signal_source(&self, _state: BlockStateId, _context: SignalQueryContext) -> bool {
        true
    }

    /// Vanilla parity: `JukeboxBlock.ownSignal`, which is full while the music
    /// runs rather than while a disc is merely inside.
    fn get_own_signal(
        &self,
        _state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _context: SignalQueryContext,
    ) -> i32 {
        Self::with_jukebox(world, pos, |jukebox| {
            if jukebox.is_playing() {
                PLAYING_SIGNAL
            } else {
                0
            }
        })
        .unwrap_or(0)
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    /// Vanilla parity: `JukeboxBlock.getAnalogOutputSignal`, which reads the
    /// disc rather than how full the block is: each record has its own number.
    fn get_analog_output_signal(
        &self,
        _state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        Self::with_jukebox(world, pos, JukeboxBlockEntity::comparator_output).unwrap_or(0)
    }
}
