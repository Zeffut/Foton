//! Minecart items.
//!
//! Vanilla parity: `MinecartItem`. Six items, one class, and the reason the
//! minecart entity is reachable at all: without this a cart can only be
//! summoned by a command, which is not something a player has.
//!
//! Which cart each item makes comes from the extracted `type`, so there is no
//! table of variants here.

use std::sync::Arc;

use foton_macros::item_behavior;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::vanilla_game_events;
use glam::DVec3;

use crate::behavior::blocks::rail_shape_at;
use crate::behavior::{InteractionResult, ItemBehavior, UseOnContext};
use crate::entity::{ENTITIES, next_entity_id};
use crate::world::LevelReader as _;
use crate::world::game_event::GameEventContext;

/// How far above the rail block a cart sits.
///
/// Vanilla parity: the `0.0625` of `MinecartItem.useOn`, one pixel.
const RAIL_HEIGHT: f64 = 0.0625;

/// And how much higher on a slope.
const SLOPE_LIFT: f64 = 0.5;

/// Behavior for every minecart item.
#[item_behavior]
pub struct MinecartItem {
    /// The cart this item puts on the rail.
    #[json_arg(vanilla_entities, json = "type")]
    entity_type: EntityTypeRef,
}

impl MinecartItem {
    /// Creates a minecart item behavior.
    #[must_use]
    pub const fn new(entity_type: EntityTypeRef) -> Self {
        Self { entity_type }
    }
}

impl ItemBehavior for MinecartItem {
    /// Vanilla parity: `MinecartItem.useOn`.
    ///
    /// A minecart goes on a rail and nowhere else, which is the whole of the
    /// check: clicking anything that is not a rail fails outright rather than
    /// falling through to placing something.
    fn use_on(&self, context: &mut UseOnContext) -> InteractionResult {
        let pos = context.hit_result.block_pos;
        let Some(shape) = rail_shape_at(context.world.get_block_state(pos)) else {
            return InteractionResult::Fail;
        };

        let lift = if shape.is_slope() { SLOPE_LIFT } else { 0.0 };
        let location = DVec3::new(
            f64::from(pos.x()) + 0.5,
            f64::from(pos.y()) + RAIL_HEIGHT + lift,
            f64::from(pos.z()) + 0.5,
        );

        let Some(cart) = ENTITIES.create(
            self.entity_type,
            next_entity_id(),
            location,
            Arc::downgrade(context.world),
        ) else {
            // The item exists for every variant, but only the plain cart has an
            // entity yet; using one of the others places nothing rather than
            // crashing.
            return InteractionResult::Fail;
        };

        if context.world.try_add_entity(cart).is_err() {
            return InteractionResult::Fail;
        }

        context.world.game_event(
            &vanilla_game_events::ENTITY_PLACE,
            pos,
            &GameEventContext::new(Some(context.player), None),
        );

        if !context.player.has_infinite_materials() {
            context.inv.with_item(|item| item.shrink(1));
        }

        InteractionResult::Success
    }
}

#[cfg(test)]
mod tests {
    use foton_registry::{init_vanilla_registry, vanilla_entities};

    use super::*;

    /// The item knows which cart it makes.
    ///
    /// The mapping is extracted data rather than a table here, so this is the
    /// check that the codegen really passes it through -- an item pointing at
    /// the wrong variant would put the wrong cart on the rail.
    #[test]
    fn a_minecart_item_names_its_own_cart() {
        init_vanilla_registry();
        let item = MinecartItem::new(&vanilla_entities::MINECART);
        assert_eq!(item.entity_type, &vanilla_entities::MINECART);
    }
}
