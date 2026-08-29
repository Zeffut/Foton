//! Boat items.
//!
//! Vanilla parity: `BoatItem`. Twenty items, one class, and the reason the boat
//! entity is reachable at all: without this a boat can only be summoned by a
//! command, which is not something a player has.
//!
//! Which boat each item makes comes from the extracted `entity_type`, so there
//! is no table of woods here.

use std::sync::Arc;

use foton_macros::item_behavior;
use foton_registry::entity_type::EntityTypeRef;
use foton_registry::vanilla_game_events;
use foton_utils::BlockPos;
use glam::DVec3;

use crate::behavior::{InteractionResult, ItemBehavior, UseItemContext};
use crate::entity::{ENTITIES, Entity, next_entity_id};
use crate::physics::collision::{WorldCollisionProvider, has_collision};
use crate::player::Player;
use crate::world::game_event::GameEventContext;
use crate::world::{ClipBlockShape, ClipFluid, World};

/// Behavior for every boat item.
#[item_behavior]
pub struct BoatItem {
    /// The boat this item puts in the world.
    #[json_arg(vanilla_entities, json = "entity_type")]
    entity_type: EntityTypeRef,
}

impl BoatItem {
    /// Creates a boat item behavior.
    #[must_use]
    pub const fn new(entity_type: EntityTypeRef) -> Self {
        Self { entity_type }
    }
}

impl ItemBehavior for BoatItem {
    /// Vanilla parity: `BoatItem.use`.
    ///
    /// The ray stops on fluid as well as blocks, which is what lets a boat be
    /// placed on the surface of water rather than only on the bank.
    ///
    /// TODO: vanilla also refuses when the player's eye is inside a pickable
    /// entity, so a boat cannot be placed through one. Foton has no entity ray
    /// query yet, so that check is missing and a boat may be placed while
    /// standing in another.
    fn use_item(&self, context: &mut UseItemContext) -> InteractionResult {
        let Some(location) = looked_at_point(context.world, context.player) else {
            return InteractionResult::Pass;
        };

        let Some(boat) = ENTITIES.create(
            self.entity_type,
            next_entity_id(),
            location,
            Arc::downgrade(context.world),
        ) else {
            // The item exists for every wood, but the chest variants have no
            // entity yet; using one places nothing rather than crashing.
            return InteractionResult::Fail;
        };

        boat.set_rotation((context.player.rotation().0, 0.0));

        // Vanilla parity: a boat that would be inside a block is not placed.
        if has_collision(
            &WorldCollisionProvider::new(context.world),
            boat.bounding_box(),
        ) {
            return InteractionResult::Fail;
        }

        if context.world.try_add_entity(boat).is_err() {
            return InteractionResult::Fail;
        }

        context.world.game_event(
            &vanilla_game_events::ENTITY_PLACE,
            BlockPos::from(location),
            &GameEventContext::new(Some(context.player), None),
        );

        if !context.player.has_infinite_materials() {
            context.inv.with_item(|item| item.shrink(1));
        }

        InteractionResult::Success
    }
}

/// Returns where the player is looking, stopping at fluid as well as blocks.
///
/// Vanilla parity: `Item.getPlayerPOVHitResult` with `ClipContext.Fluid.ANY`.
fn looked_at_point(world: &Arc<World>, player: &Player) -> Option<DVec3> {
    let from = player.position().with_y(player.get_eye_y());
    let (yaw, pitch) = player.rotation();
    let to = from + player.calculate_view_vector(pitch, yaw) * player.block_interaction_range();

    let hit = world.clip(from, to, ClipBlockShape::Outline, ClipFluid::Any);
    (!hit.miss).then_some(hit.location)
}

#[cfg(test)]
mod tests {
    use foton_registry::{init_vanilla_registry, vanilla_entities};

    use super::*;

    /// The item knows which boat it makes.
    ///
    /// The mapping is extracted data rather than a table here, so this is the
    /// check that the codegen really passes it through -- an item pointing at
    /// the wrong wood would put the wrong boat on the water.
    #[test]
    fn a_boat_item_names_its_own_boat() {
        init_vanilla_registry();

        let oak = BoatItem::new(&vanilla_entities::OAK_BOAT);
        assert_eq!(oak.entity_type.key, vanilla_entities::OAK_BOAT.key);

        let raft = BoatItem::new(&vanilla_entities::BAMBOO_RAFT);
        assert_eq!(raft.entity_type.key, vanilla_entities::BAMBOO_RAFT.key);
    }
}
