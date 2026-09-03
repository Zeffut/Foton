//! Item frame items.
//!
//! Vanilla parity: `HangingEntityItem` as it is used for `ItemFrame` and
//! `GlowItemFrame`. Without this a frame can only be summoned by a command,
//! which is not something a player has -- and the frame entity, comparator
//! output and all, has been in the tree with no way to reach it.

use std::sync::Arc;

use foton_macros::item_behavior;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::vanilla_game_events;

use crate::behavior::{InteractionResult, ItemBehavior, UseOnContext};
use crate::entity::entities::ItemFrameEntity;
use crate::entity::{Entity as _, next_entity_id};
use crate::event::HangingPlaceEvent;
use crate::world::LevelReader as _;
use crate::world::game_event::GameEventContext;

/// Behavior for the item frame items.
#[item_behavior]
pub struct ItemFrameItem {
    /// The frame this item hangs.
    #[json_arg(vanilla_entities, json = "type")]
    entity_type: EntityTypeRef,
}

impl ItemFrameItem {
    /// Creates an item frame item behavior.
    #[must_use]
    pub const fn new(entity_type: EntityTypeRef) -> Self {
        Self { entity_type }
    }
}

impl ItemBehavior for ItemFrameItem {
    /// Vanilla parity: `HangingEntityItem.useOn`.
    ///
    /// A frame hangs on the side of something, so a click on the top or bottom
    /// of a block places nothing, and the block clicked has to be solid enough
    /// to hold it.
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        let clicked = context.hit_result.block_pos;
        let face = context.hit_result.direction;
        if !face.is_horizontal() {
            return InteractionResult::Fail;
        }

        // Vanilla parity: the support half of `HangingEntity.survives`. The
        // collision and the rule against two frames sharing a square are not
        // checked yet, so frames can be stacked where vanilla would refuse.
        if !context
            .world
            .is_face_sturdy(context.world.get_block_state(clicked), clicked, face)
        {
            return InteractionResult::Fail;
        }

        let frame_pos = clicked.relative(face);
        let frame = Arc::new(ItemFrameEntity::new_attached(
            self.entity_type,
            next_entity_id(),
            frame_pos,
            face,
            Arc::downgrade(context.world),
        ));
        let position = frame.position();

        let mut event = HangingPlaceEvent::new(
            frame.uuid(),
            context.player.uuid(),
            context.world.key.to_string(),
            clicked,
            format!("{face:?}"),
        );
        context.world.fire_event(&mut event);
        if event.is_cancelled() {
            return InteractionResult::Fail;
        }

        if context.world.try_add_entity(frame).is_err() {
            return InteractionResult::Fail;
        }

        context.world.game_event_at(
            &vanilla_game_events::ENTITY_PLACE,
            position,
            &GameEventContext::new(Some(context.player), None),
        );

        if !context.player.has_infinite_materials() {
            context.inv.with_item(|item| item.shrink(1));
        }

        InteractionResult::Success
    }
}
