//! Lectern behavior.
//!
//! Vanilla parity: `LecternBlock`. Put a book on it and it becomes readable;
//! turn a page and it pulses redstone from the block below. Two different
//! signals come out of it -- a full pulse on every page turn, and a comparator
//! reading of how far through the book the reader has got.

use std::sync::{Arc, Weak};

use foton_macros::block_behavior;
use foton_registry::blocks::BlockRef;
use foton_registry::blocks::block_state_ext::BlockStateExt as _;
use foton_registry::blocks::properties::{
    BlockStateProperties, BoolProperty, Direction, EnumProperty,
};
use foton_registry::item_stack::ItemStack;
use foton_registry::vanilla_item_tags::ItemTag;
use foton_registry::{
    REGISTRY, TaggedRegistryExt as _, level_events, sound_events, vanilla_block_entity_types,
    vanilla_game_events,
};
use foton_utils::types::{InteractionHand, UpdateFlags};
use foton_utils::{BlockPos, BlockStateId, Downcast as _, translations};
use text_components::TextComponent;

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::BLOCK_ENTITIES;
use crate::block_entity::entities::LecternBlockEntity;
use crate::inventory::menu::kinds::lectern;
use crate::player::Player;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader, SignalQueryContext, World};

/// Whether a book is on the stand.
const HAS_BOOK: &BoolProperty = &BlockStateProperties::HAS_BOOK;

/// Whether the page-turn pulse is currently high.
const POWERED: &BoolProperty = &BlockStateProperties::POWERED;

/// Which way the stand faces.
const FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;

/// How long the page-turn pulse lasts.
///
/// Vanilla parity: the `scheduleTick(pos, block, 2)` of `signalPageChange`.
const PULSE_TICKS: i32 = 2;

/// What a powered lectern gives out.
const PULSE_SIGNAL: i32 = 15;

/// Behavior for the lectern.
#[block_behavior]
pub struct LecternBlock {
    block: BlockRef,
}

impl LecternBlock {
    /// Creates a lectern behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }

    /// Runs `f` on the lectern at `pos`, if there is one.
    fn with_lectern<T>(
        world: &dyn LevelReader,
        pos: BlockPos,
        f: impl FnOnce(&LecternBlockEntity) -> T,
    ) -> Option<T> {
        let entity = world.get_block_entity(pos)?;
        let lectern = entity.downcast_ref::<LecternBlockEntity>()?;
        Some(f(lectern))
    }
}

impl BlockBehavior for LecternBlock {
    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(
            self.block
                .default_state()
                .set_value(FACING, context.horizontal_direction().opposite()),
        )
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::LECTERN,
            level,
            pos,
            state,
        ))
    }

    /// Vanilla parity: `LecternBlock.tick`, which drops the page-turn pulse
    /// two ticks after it went up.
    fn tick(&self, state: BlockStateId, world: &Arc<World>, pos: BlockPos) {
        if !state.get_value(POWERED) {
            return;
        }
        world.set_block(
            pos,
            state.set_value(POWERED, false),
            UpdateFlags::UPDATE_ALL,
        );
        world.update_neighbors_at(pos.below(), self.block);
    }

    /// Vanilla parity: `LecternBlock.useItemOn`, which only takes the books a
    /// lectern can hold.
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
        if state.get_value(HAS_BOOK) {
            return InteractionResult::TryEmptyHandInteraction;
        }

        let held = {
            let inventory = player.inventory.lock();
            inventory.get_item_in_hand(hand).copy_with_count(1)
        };
        if held.is_empty()
            || !REGISTRY
                .items
                .is_in_tag(held.item(), &ItemTag::LECTERN_BOOKS)
        {
            return InteractionResult::TryEmptyHandInteraction;
        }

        if Self::with_lectern(world.as_ref(), pos, |lectern| lectern.set_book(held)).is_none() {
            return InteractionResult::TryEmptyHandInteraction;
        }

        world.set_block(
            pos,
            state.set_value(POWERED, false).set_value(HAS_BOOK, true),
            UpdateFlags::UPDATE_ALL,
        );
        world.update_neighbors_at(pos.below(), self.block);
        world.game_event(
            &vanilla_game_events::BLOCK_CHANGE,
            pos,
            &GameEventContext::new(Some(player), None),
        );
        world.play_block_sound(&sound_events::ITEM_BOOK_PUT, pos, 1.0, 1.0, None);

        if !player.has_infinite_materials() {
            inv.with_item(|item| item.shrink(1));
        }

        InteractionResult::Success
    }

    /// Vanilla parity: `LecternBlock.useWithoutItem`, which opens the book.
    fn use_without_item(
        &self,
        state: BlockStateId,
        world: &Arc<World>,
        pos: BlockPos,
        player: &Player,
        _hit_result: &BlockHitResult,
        _inv: &mut InventoryAccess,
    ) -> InteractionResult {
        if !state.get_value(HAS_BOOK) {
            return InteractionResult::Consume;
        }

        let inventory = player.inventory.clone();
        let world = Arc::clone(world);
        player.open_menu(
            TextComponent::translated(translations::CONTAINER_LECTERN.msg()),
            move |context| lectern(inventory, context.container_id, pos, &world),
        );

        // TODO: Award stat INTERACT_WITH_LECTERN; Foton has no statistics
        // registry.
        InteractionResult::Success
    }

    fn is_signal_source(&self, _state: BlockStateId, _context: SignalQueryContext) -> bool {
        true
    }

    /// Vanilla parity: `LecternBlock.ownSignal` -- full while the page-turn
    /// pulse is high, nothing otherwise.
    fn get_own_signal(
        &self,
        state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
        _context: SignalQueryContext,
    ) -> i32 {
        if state.get_value(POWERED) {
            PULSE_SIGNAL
        } else {
            0
        }
    }

    /// Vanilla parity: `LecternBlock.getDirectSignal`, which only powers the
    /// block above.
    fn get_direct_signal(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        direction: Direction,
        context: SignalQueryContext,
    ) -> i32 {
        if direction == Direction::Up {
            self.get_own_signal(state, world, pos, context)
        } else {
            0
        }
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    /// Vanilla parity: `LecternBlock.getAnalogOutputSignal`, which reads how
    /// far through the book the reader is rather than whether it is powered.
    fn get_analog_output_signal(
        &self,
        state: BlockStateId,
        world: &dyn LevelReader,
        pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        if !state.get_value(HAS_BOOK) {
            return 0;
        }
        Self::with_lectern(world, pos, LecternBlockEntity::redstone_signal).unwrap_or(0)
    }
}

/// Pulses the lectern, the way turning a page does.
///
/// Vanilla parity: `LecternBlock.signalPageChange`. The pulse is read from the
/// block *below*, which is why a lectern's redstone is wired underneath it
/// rather than beside it. A free function rather than a method because the
/// menu reaches it without holding the behavior.
pub fn signal_lectern_page_change(world: &Arc<World>, pos: BlockPos) {
    let state = world.get_block_state(pos);
    let block = state.get_block();
    world.set_block(pos, state.set_value(POWERED, true), UpdateFlags::UPDATE_ALL);
    world.update_neighbors_at(pos.below(), block);
    world.schedule_block_tick_default(pos, block, PULSE_TICKS);
    world.level_event(level_events::SOUND_PAGE_TURN, pos, 0, None);
}

/// Drops the book back and clears the block's state.
///
/// Vanilla parity: the `onBookItemRemove` half of `LecternBlockEntity`, which
/// is reached from the menu's Take Book button rather than from the block.
pub fn take_book_from(world: &Arc<World>, pos: BlockPos) -> ItemStack {
    let state = world.get_block_state(pos);
    let Some(book) = LecternBlock::with_lectern(world.as_ref(), pos, LecternBlockEntity::take_book)
    else {
        return ItemStack::empty();
    };

    world.set_block(
        pos,
        state.set_value(POWERED, false).set_value(HAS_BOOK, false),
        UpdateFlags::UPDATE_ALL,
    );
    world.update_neighbors_at(pos.below(), state.get_block());
    book
}
