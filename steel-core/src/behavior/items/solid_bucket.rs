//! The powder snow bucket.

use std::sync::Arc;

use steel_macros::item_behavior;
use steel_registry::blocks::BlockRef;
use steel_registry::blocks::block_state_ext::BlockStateExt as _;
use steel_registry::item_stack::ItemStack;
use steel_registry::sound_event::SoundEventRef;
use steel_registry::vanilla_game_events;
use steel_registry::vanilla_items;
use steel_utils::BlockPos;
use steel_utils::types::UpdateFlags;

use crate::behavior::{
    BucketHit, DispensibleContainerItem, InteractionResult, ItemBehavior, UseOnContext,
};
use crate::entity::Entity;
use crate::player::Player;
use crate::world::World;
use crate::world::game_event::GameEventContext;

use super::BlockItem;

/// A bucket that places a block instead of a fluid.
///
/// Vanilla parity: `SolidBucketItem`, a `BlockItem` with its own place sound
/// that hands back an empty bucket.
#[item_behavior]
pub struct SolidBucketItem {
    #[json_arg(vanilla_blocks, json = "block")]
    block: BlockRef,
    #[json_arg(sound_events, json = "place_sound")]
    place_sound: SoundEventRef,
    base: BlockItem,
}

impl SolidBucketItem {
    /// Creates a solid bucket behavior.
    #[must_use]
    pub const fn new(block: BlockRef, place_sound: SoundEventRef) -> Self {
        Self {
            block,
            place_sound,
            base: BlockItem::new(block).with_place_sound(place_sound),
        }
    }
}

impl DispensibleContainerItem for SolidBucketItem {
    /// Vanilla parity: `SolidBucketItem.emptyContents`, which is what lets a
    /// dispenser lay powder snow. Unlike a fluid bucket it insists on an empty
    /// block and never retries at the neighbour, so the hit result goes unread.
    fn empty_contents(
        &self,
        user: Option<&Player>,
        world: &Arc<World>,
        pos: BlockPos,
        _hit: Option<BucketHit>,
    ) -> bool {
        if !world.is_in_valid_bounds(pos) || !world.get_block_state(pos).is_air() {
            return false;
        }

        world.set_block(pos, self.block.default_state(), UpdateFlags::UPDATE_ALL);
        world.game_event(
            &vanilla_game_events::FLUID_PLACE,
            pos,
            &GameEventContext::new(user.map(|player| player as &dyn Entity), None),
        );
        world.play_block_sound(self.place_sound, pos, 1.0, 1.0, None);
        true
    }
}

impl ItemBehavior for SolidBucketItem {
    fn as_dispensible_container(&self) -> Option<&dyn DispensibleContainerItem> {
        Some(self)
    }

    /// Vanilla parity: `SolidBucketItem.useOn`.
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        let result = self.base.place(context.build_place_context());
        if !result.consumes_action() {
            return result;
        }

        // Vanilla parity: `BucketItem.getEmptySuccessItem` -- an empty bucket,
        // or the untouched stack in creative.
        let empty = if context.player.has_infinite_materials() {
            context
                .inv
                .with_item(|item| item.copy_with_count(item.count()))
        } else {
            ItemStack::new(&vanilla_items::BUCKET)
        };
        context.inv.with_item(|item| *item = empty);

        result
    }
}
