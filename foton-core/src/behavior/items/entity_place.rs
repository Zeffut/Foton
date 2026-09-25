//! The plugin veto on an entity a player places from an item.

use std::sync::Arc;

use foton_utils::types::InteractionHand;
use foton_utils::{BlockPos, Direction};

use crate::entity::{RemovalReason, SharedEntity};
use crate::event::{EntityPlaceEvent, Event as _};
use crate::player::Player;
use crate::world::World;

/// Asks plugins whether `entity`, just added to the world by `player`, may
/// stay; takes it back out when they refuse.
///
/// Paper parity: `CraftEventFactory.callEntityPlaceEvent` in `BoatItem`,
/// `MinecartItem`, `ArmorStandItem` and `EndCrystalItem`. Paper asks before
/// the entity is added; Foton asks just after, so a listener reads a live
/// entity -- its id, its type, its position -- and a refusal removes it before
/// the tracker has told any client about it. The caller must then leave the
/// item unspent, and the client's inventory is resent as Paper does.
pub(crate) fn entity_place_allowed(
    world: &Arc<World>,
    entity: &SharedEntity,
    player: &Player,
    against: (BlockPos, Direction),
    hand: InteractionHand,
) -> bool {
    let mut event = EntityPlaceEvent::new(
        entity.uuid(),
        player.gameprofile.id,
        world.key.to_string(),
        against.0,
        block_face(against.1),
        hand,
    );
    world.fire_event(&mut event);
    if !event.is_cancelled() {
        return true;
    }
    entity.set_removed(RemovalReason::Discarded);
    player.send_inventory_to_remote();
    false
}

/// The Bukkit `BlockFace` name of a direction.
const fn block_face(direction: Direction) -> &'static str {
    match direction {
        Direction::Down => "DOWN",
        Direction::Up => "UP",
        Direction::North => "NORTH",
        Direction::South => "SOUTH",
        Direction::West => "WEST",
        Direction::East => "EAST",
    }
}
