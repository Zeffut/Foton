//! The items that hang an entity on a wall.
//!
//! Vanilla parity: `HangingEntityItem`. Foton maps only the painting to this
//! behavior -- the two frame items go through `ItemFrameItem`, which predates
//! it -- so the frame branches of `useOn` have no counterpart here.

use std::sync::Arc;

use foton_macros::item_behavior;
use foton_registry::data_components::vanilla_components::PAINTING_VARIANT;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::{sound_events, vanilla_game_events};

use crate::behavior::{InteractionResult, ItemBehavior, UseOnContext};
use crate::entity::entities::PaintingEntity;
use crate::entity::{Entity as _, next_entity_id};
use crate::world::game_event::GameEventContext;

/// Behavior for the items that hang an entity on a wall.
#[item_behavior]
pub struct HangingEntityItem {
    /// The entity this item hangs.
    #[json_arg(vanilla_entities, json = "type")]
    entity_type: EntityTypeRef,
}

impl HangingEntityItem {
    /// Creates a hanging entity item behavior.
    #[must_use]
    pub const fn new(entity_type: EntityTypeRef) -> Self {
        Self { entity_type }
    }
}

impl ItemBehavior for HangingEntityItem {
    /// Vanilla parity: `HangingEntityItem.useOn`.
    ///
    /// A painting needs a wall, so a click on the top or bottom of a block
    /// places nothing. Which picture goes up is decided by what fits, and a
    /// wall too small for any of them consumes the click rather than failing
    /// it -- vanilla returns `CONSUME` there.
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        let face = context.hit_result.direction;
        // Vanilla parity: `HangingEntityItem.mayPlace`. Its second half,
        // `Player.mayUseItemAt`, has no counterpart -- Foton has no
        // adventure-mode placement permission on an item stack yet.
        if !face.is_horizontal() {
            return InteractionResult::Fail;
        }

        let painting_pos = context.hit_result.block_pos.relative(face);
        let Some(painting) = PaintingEntity::create(
            self.entity_type,
            next_entity_id(),
            context.world,
            painting_pos,
            face,
        ) else {
            return InteractionResult::Consume;
        };
        let painting = Arc::new(painting);

        // Vanilla parity: the `EntityType.createDefaultStackConfig` of `useOn`,
        // which copies the stack's implicit components onto the entity. Only
        // `PAINTING_VARIANT` is copied here: a stack that names its picture
        // overrides the one `create` rolled, so a player who crafted a
        // specific painting hangs that one.
        let stack_variant = context.inv.with_item(|item| {
            item.get(PAINTING_VARIANT)
                .and_then(|component| component.variant().as_reference())
        });
        if let Some(variant) = stack_variant {
            painting.set_variant(variant);
        }

        if !painting.survives() {
            return InteractionResult::Consume;
        }

        let position = painting.position();
        painting.play_sound(&sound_events::ENTITY_PAINTING_PLACE, 1.0, 1.0);
        context.world.game_event_at(
            &vanilla_game_events::ENTITY_PLACE,
            position,
            &GameEventContext::new(Some(context.player), None),
        );

        if context.world.try_add_entity(painting).is_err() {
            return InteractionResult::Fail;
        }

        // Deviation: vanilla shrinks unconditionally and lets the creative
        // inventory put the stack back. Foton has no such restore, so the
        // creative case is skipped here, as it is for the item frame.
        if !context.player.has_infinite_materials() {
            context.inv.with_item(|item| item.shrink(1));
        }

        InteractionResult::Success
    }
}
