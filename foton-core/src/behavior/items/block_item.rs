//! Block item behavior implementation.

use foton_macros::item_behavior;
use foton_registry::data_components::vanilla_components::BLOCK_STATE;
use foton_registry::sound_event::SoundEventRef;
use foton_registry::vanilla_block_tags::BlockTag;
use foton_registry::{
    REGISTRY, TaggedRegistryExt as _,
    blocks::{BlockRef, block_state_ext::BlockStateExt},
    vanilla_blocks, vanilla_game_events,
};
use foton_utils::{BlockPos, BlockStateId, types::UpdateFlags};

use crate::advancement::triggers;
use crate::behavior::context::{BlockPlaceContext, InteractionResult, UseOnContext};
use crate::behavior::{BLOCK_BEHAVIORS, ItemBehavior};
use crate::entity::Entity;
use crate::event::{BlockPlaceEvent, Event as _};
use crate::fluid::{FluidStateExt as _, get_fluid_state};
use crate::world::game_event::GameEventContext;

/// Behavior for items that place blocks.
#[item_behavior]
pub struct BlockItem {
    /// The block this item places.
    #[json_arg(vanilla_blocks, json = "block")]
    pub block: BlockRef,
    /// Vanilla parity: `BlockItem.getPlaceSound`, which only `SolidBucketItem`
    /// overrides. `None` keeps the placed block's own sound type.
    place_sound: Option<SoundEventRef>,
    /// Vanilla parity: `BlockItem.mustSurvive`, which only scaffolding turns off.
    must_survive: bool,
}

impl BlockItem {
    const PLACE_BLOCK_FLAGS: UpdateFlags = UpdateFlags::UPDATE_ALL_IMMEDIATE;

    /// Creates a new block item behavior for the given block.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            block,
            place_sound: None,
            must_survive: true,
        }
    }

    /// Returns this behavior with `BlockItem.getPlaceSound` overridden.
    #[must_use]
    pub const fn with_place_sound(mut self, place_sound: SoundEventRef) -> Self {
        self.place_sound = Some(place_sound);
        self
    }

    /// Returns this behavior with `BlockItem.mustSurvive` turned off.
    #[must_use]
    pub const fn without_must_survive(mut self) -> Self {
        self.must_survive = false;
        self
    }

    /// Runs vanilla's `BlockItem.place` with the two steps subclasses replace.
    ///
    /// `placement_state` stands in for `getPlacementState`, which the
    /// standing-and-wall item overrides to choose between two blocks;
    /// `place_block` stands in for `placeBlock`, which the double-high item
    /// overrides to clear the space above.
    pub(super) fn place_with(
        &self,
        mut context: BlockPlaceContext<'_>,
        placement_state: impl FnOnce(&BlockPlaceContext<'_>) -> Option<BlockStateId>,
        place_block: impl FnOnce(&BlockPlaceContext<'_>, BlockStateId) -> bool,
    ) -> InteractionResult {
        if !context.can_place() {
            return InteractionResult::Fail;
        }
        let place_pos = context.place_pos();

        let Some(new_state) = placement_state(&context) else {
            return InteractionResult::Fail;
        };

        let behavior = BLOCK_BEHAVIORS.get_behavior(new_state.get_block());
        if self.must_survive && !behavior.can_survive(new_state, context.world, place_pos) {
            return InteractionResult::Fail;
        }

        let collision_shape = new_state.get_collision_shape_at(place_pos);
        if !context.world.is_unobstructed(collision_shape, place_pos) {
            return InteractionResult::Fail;
        }

        // Every vanilla check has passed by now, so a listener is only asked
        // about placements that would otherwise have happened. A dispenser
        // firing a block has no player and does not reach this.
        if let Some(player) = context.player()
            && let Some(shared) = player.shared()
        {
            let item = context.with_item(|stack| stack.clone());
            let mut event = BlockPlaceEvent::new(shared, place_pos, new_state, item);
            player.fire_event(&mut event);
            if event.is_cancelled() {
                return InteractionResult::Fail;
            }
        }

        if !place_block(&context, new_state) {
            return InteractionResult::Fail;
        }

        let mut placed_state = context.world.get_block_state(place_pos);
        if placed_state.get_block() == new_state.get_block() {
            placed_state = Self::update_block_state_from_tag(&context, place_pos, placed_state);
            Self::update_block_entity_components(&context, place_pos);
            let placed_behavior = BLOCK_BEHAVIORS.get_behavior(placed_state.get_block());
            placed_behavior.set_placed_by(placed_state, context.world, place_pos, context.source());
            // Vanilla parity: the `CriteriaTriggers.PLACED_BLOCK` of
            // `BlockItem.place`, which sits inside this same
            // "the block that landed is the one we asked for" branch and runs
            // before the stack is shrunk below.
            if let Some(player) = context.player() {
                let tool = context.with_item(Clone::clone);
                triggers::world::placed_block(player, place_pos, placed_state, &tool);
            }
        }

        // Play place sound (exclude the placing player, they hear it
        // client-side). Vanilla reads the sound off the state that ended up in
        // the world, which is how a wall banner sounds like the wall form.
        let sound_type = &placed_state.get_block().config.sound_type;
        context.world.play_block_sound(
            self.place_sound.unwrap_or(sound_type.place_sound),
            place_pos,
            sound_type.volume,
            sound_type.pitch,
            context.player().map(Entity::id),
        );
        context.world.game_event(
            &vanilla_game_events::BLOCK_PLACE,
            place_pos,
            &GameEventContext::new(
                context.player().map(|player| player as &dyn Entity),
                Some(placed_state),
            ),
        );

        context.with_item_mut(|item| item.shrink(1));

        InteractionResult::Success
    }

    /// Places this block using an already constructed placement context.
    pub fn place(&self, context: BlockPlaceContext<'_>) -> InteractionResult {
        self.place_with(context, |c| self.placement_state(c), Self::place_block)
    }

    /// Vanilla parity: `BlockItem.getPlacementState`.
    pub(super) fn placement_state(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        BLOCK_BEHAVIORS
            .get_behavior(self.block)
            .get_state_for_placement(context)
    }

    pub(super) fn place_block(context: &BlockPlaceContext<'_>, state: BlockStateId) -> bool {
        context
            .world
            .set_block(context.place_pos(), state, Self::PLACE_BLOCK_FLAGS)
    }

    /// Re-applies the block properties the item was carrying.
    ///
    /// Vanilla parity: `BlockItem.updateBlockStateFromTag`, the other half of
    /// the `minecraft:copy_state` loot function. Without it a picked-up hive
    /// carries its `honey_level` and then forgets it the moment it goes back
    /// down.
    fn update_block_state_from_tag(
        context: &BlockPlaceContext<'_>,
        pos: BlockPos,
        placed_state: BlockStateId,
    ) -> BlockStateId {
        let properties = context.with_item(|item| item.get(BLOCK_STATE).cloned());
        let Some(properties) = properties.filter(|properties| !properties.is_empty()) else {
            return placed_state;
        };

        let modified_state = properties.apply(placed_state);
        if modified_state != placed_state {
            context
                .world
                .set_block(pos, modified_state, UpdateFlags::UPDATE_CLIENTS);
        }
        modified_state
    }

    /// Hands the placed block entity the components the item carried.
    ///
    /// Vanilla parity: `BlockItem.updateBlockEntityComponents`.
    fn update_block_entity_components(context: &BlockPlaceContext<'_>, pos: BlockPos) {
        let Some(block_entity) = context.world.get_block_entity(pos) else {
            return;
        };
        // The stack is copied out first: `with_item` holds the placing player's
        // inventory lock for the whole closure, and a block entity taking its
        // components has no business running under it.
        let stack = context.with_item(Clone::clone);
        block_entity.apply_components_from_item_stack(&stack);
        block_entity.set_changed();
    }
}

impl ItemBehavior for BlockItem {
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        self.place(context.build_place_context())
    }

    fn placed_block(&self) -> Option<BlockRef> {
        Some(self.block)
    }

    /// Vanilla parity: `BlockItem.canFitInsideContainerItems`. Foton has no
    /// `ShulkerBoxBlock` behavior to test against, so the block tag holding
    /// exactly those seventeen blocks stands in, as it already does for banners
    /// in the loom.
    fn can_fit_inside_container_items(&self) -> bool {
        !REGISTRY
            .blocks
            .is_in_tag(self.block, &BlockTag::SHULKER_BOXES)
    }
}

/// Behavior for double-high block items (doors, tall flowers, etc.).
///
/// Vanilla's `DoubleHighBlockItem` extends `BlockItem` and overrides `placeBlock`
/// to place the upper half block above the lower half.
///
/// The `_block` field is read by the build script via `#[json_arg]` to generate constructor
/// calls from `classes.json`. The actual value is forwarded into `base`.
#[item_behavior]
pub struct DoubleHighBlockItem {
    #[json_arg(vanilla_blocks, json = "block")]
    _block: BlockRef,
    base: BlockItem,
}

impl DoubleHighBlockItem {
    const PREPARE_UPPER_FLAGS: UpdateFlags =
        UpdateFlags::UPDATE_ALL_IMMEDIATE.union(UpdateFlags::UPDATE_KNOWN_SHAPE);

    /// Creates a new double-high block item behavior for the given block.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self {
            _block: block,
            base: BlockItem::new(block),
        }
    }

    fn place_block(context: &BlockPlaceContext<'_>, state: BlockStateId) -> bool {
        let above = context.place_pos().above();
        let above_state = if get_fluid_state(context.world, above).is_water() {
            vanilla_blocks::WATER.default_state()
        } else {
            vanilla_blocks::AIR.default_state()
        };
        let _ = context
            .world
            .set_block(above, above_state, Self::PREPARE_UPPER_FLAGS);

        BlockItem::place_block(context, state)
    }
}

impl ItemBehavior for DoubleHighBlockItem {
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        self.base.place_with(
            context.build_place_context(),
            |c| self.base.placement_state(c),
            Self::place_block,
        )
    }
}
