//! Beehive block behavior implementation.

use std::sync::{Arc, Weak};

use steel_macros::block_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt;
use steel_registry::blocks::properties::{
    BlockStateProperties, BoolProperty, Direction, EnumProperty, IntProperty,
};
use steel_registry::item_stack::ItemStack;
use steel_registry::vanilla_block_tags::BlockTag;
use steel_registry::{
    REGISTRY, TaggedRegistryExt as _, sound_events, vanilla_block_entity_types,
    vanilla_game_events, vanilla_items,
};
use steel_utils::types::{InteractionHand, UpdateFlags};
use steel_utils::{BlockPos, BlockStateId};

use crate::behavior::InventoryAccess;
use crate::behavior::block::{BlockBehavior, BlockEntityCreation};
use crate::behavior::context::{BlockHitResult, BlockPlaceContext, InteractionResult};
use crate::block_entity::BLOCK_ENTITIES;
use crate::player::Player;
use crate::world::game_event::GameEventContext;
use crate::world::{LevelReader, World};

/// The honey level at which a hive can be harvested.
///
/// Vanilla parity: the `honeyLevel >= 5` of `BeehiveBlock.useItemOn`.
const FULL_HONEY_LEVEL: u8 = 5;

/// How much honeycomb a full hive gives.
///
/// Vanilla parity: the `harvest_beehive` loot table, which is a flat three.
/// Steel has no loot tables, so the number is written out.
const HONEYCOMB_PER_HARVEST: i32 = 3;

/// How far below a hive a campfire still calms its bees.
///
/// Vanilla parity: the `i <= 5` of `CampfireBlock.isSmokeyPos`.
const SMOKE_REACH: i32 = 5;

/// Whether a campfire is lit.
const LIT: &BoolProperty = &BlockStateProperties::LIT;

/// Returns whether smoke from a campfire reaches `pos`.
///
/// Vanilla parity: `CampfireBlock.isSmokeyPos`. This is what lets a player
/// harvest a hive without the bees turning on them, and it is the whole reason
/// beekeepers put a campfire under the hive.
///
/// Deviation: vanilla stops the search at a block whose collision shape
/// intersects the thin slab just under the hive, then looks one block further.
/// Steel has no shape-intersection helper reachable from here, so a full
/// collision block stands in -- which agrees with vanilla for every block a
/// player would actually put there, and differs only for shapes that are solid
/// at the top and open at the bottom.
fn is_smokey_pos(world: &Arc<World>, pos: BlockPos) -> bool {
    for step in 1..=SMOKE_REACH {
        let below = BlockPos::new(pos.x(), pos.y() - step, pos.z());
        let state = world.get_block_state(below);
        if is_lit_campfire(state) {
            return true;
        }
        if world.is_collision_shape_full_block_at(below, state) {
            let further = BlockPos::new(below.x(), below.y() - 1, below.z());
            return is_lit_campfire(world.get_block_state(further));
        }
    }
    false
}

/// Vanilla parity: `CampfireBlock.isLitCampfire`.
fn is_lit_campfire(state: BlockStateId) -> bool {
    REGISTRY
        .blocks
        .is_in_tag(state.get_block(), &BlockTag::CAMPFIRES)
        && state.get_value(LIT)
}

/// Behavior for beehive and bee nest blocks.
// TODO: Implement full vanilla beehive interactions, bee release, smoke/fire handling, loot/data components, and ticking.
#[block_behavior]
pub struct BeehiveBlock {
    block: BlockRef,
}

const HORIZONTAL_FACING: &EnumProperty<Direction> = &BlockStateProperties::HORIZONTAL_FACING;
const LEVEL_HONEY: &IntProperty = &BlockStateProperties::LEVEL_HONEY;

impl BeehiveBlock {
    /// Creates a new beehive block behavior.
    #[must_use]
    pub const fn new(block: BlockRef) -> Self {
        Self { block }
    }
}

impl BlockBehavior for BeehiveBlock {
    /// Vanilla parity: `BeehiveBlock.useItemOn`. A full hive gives honeycomb
    /// to shears and a honey bottle to a glass bottle, and empties either way.
    ///
    /// Not implemented: the bees. Vanilla turns every bee within eight blocks
    /// on the harvester unless a campfire is smoking below, and empties the
    /// hive of its occupants at the same time. Steel has no bee entity, so the
    /// campfire check is here and does nothing yet -- it is what the anger
    /// would hang off, and leaving it out would make the honey free forever
    /// once bees arrive.
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
        if state.get_value(LEVEL_HONEY) < FULL_HONEY_LEVEL {
            return InteractionResult::TryEmptyHandInteraction;
        }

        let held = inv.with_item(|item| item.copy_with_count(item.count()));
        let emptied = if held.is(&vanilla_items::SHEARS) {
            for _ in 0..HONEYCOMB_PER_HARVEST {
                world.drop_item_stack(pos, ItemStack::new(&vanilla_items::HONEYCOMB));
            }
            world.play_block_sound(&sound_events::BLOCK_BEEHIVE_SHEAR, pos, 1.0, 1.0, None);
            if !player.has_infinite_materials() {
                inv.with_item(|item| item.hurt_and_break(1, false));
            }
            world.game_event(
                &vanilla_game_events::SHEAR,
                pos,
                &GameEventContext::new(Some(player), None),
            );
            true
        } else if held.is(&vanilla_items::GLASS_BOTTLE) {
            inv.with_item(|item| item.shrink(1));
            player.add_item_or_drop(ItemStack::new(&vanilla_items::HONEY_BOTTLE));
            world.play_block_sound(&sound_events::ITEM_BOTTLE_FILL, pos, 1.0, 1.0, None);
            world.game_event(
                &vanilla_game_events::FLUID_PICKUP,
                pos,
                &GameEventContext::new(Some(player), None),
            );
            true
        } else {
            false
        };

        if !emptied {
            return InteractionResult::TryEmptyHandInteraction;
        }

        // TODO: Award stat ITEM_USED; Steel has no statistics registry.
        // TODO: Anger the nearby bees and empty the hive when there is no
        // smoke, once Steel has a bee entity.
        let _calm = is_smokey_pos(world, pos);
        world.set_block(
            pos,
            state.set_value(LEVEL_HONEY, 0),
            UpdateFlags::UPDATE_ALL,
        );
        InteractionResult::Success
    }

    fn get_state_for_placement(&self, context: &BlockPlaceContext<'_>) -> Option<BlockStateId> {
        Some(
            self.block
                .default_state()
                .set_value(HORIZONTAL_FACING, context.horizontal_direction().opposite()),
        )
    }

    fn new_block_entity(
        &self,
        level: Weak<World>,
        pos: BlockPos,
        state: BlockStateId,
    ) -> BlockEntityCreation {
        BlockEntityCreation::from_registered_factory(BLOCK_ENTITIES.create(
            &vanilla_block_entity_types::BEEHIVE,
            level,
            pos,
            state,
        ))
    }

    fn has_analog_output_signal(&self, _state: BlockStateId) -> bool {
        true
    }

    fn get_analog_output_signal(
        &self,
        state: BlockStateId,
        _world: &dyn LevelReader,
        _pos: BlockPos,
        _direction: Direction,
    ) -> i32 {
        state.get_value(LEVEL_HONEY).into()
    }
}
