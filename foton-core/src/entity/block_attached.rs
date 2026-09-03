//! Shared behavior for entities hung on a block.
//!
//! Vanilla parity: `BlockAttachedEntity`. Item frames, paintings and the leash
//! knot are not living entities, so the ordinary damage path refuses them and
//! they would be indestructible. What breaks them instead is this: any hit that
//! gets past the invulnerability check removes the entity outright and asks it
//! what to leave behind.
//!
//! Each concrete hanging entity performs Vanilla's periodic support check in
//! its own tick implementation and emits a cancellable physics break event.

use crate::entity::damage::DamageSource;
use crate::entity::{Entity, SharedEntity};
use crate::event::HangingBreakEvent;
use crate::player::Player;
use crate::world::World;
use foton_registry::vanilla_damage_type_tags::DamageTypeTag;
use foton_registry::vanilla_game_rules::MOB_GRIEFING;

/// An entity that hangs on a block.
///
/// Vanilla parity: `BlockAttachedEntity`.
pub trait BlockAttached: Entity {
    /// Leaves behind whatever breaking this entity yields.
    ///
    /// Vanilla parity: the abstract `BlockAttachedEntity.dropItem`. `caused_by`
    /// is the entity ultimately responsible, which decides whether anything
    /// drops at all -- a creative player gets nothing back.
    fn drop_item(&self, world: &World, caused_by: Option<&SharedEntity>);

    /// Takes a hit, removing this entity and dropping what it leaves.
    ///
    /// Vanilla parity: `BlockAttachedEntity.hurtServer`. The damage amount is
    /// ignored on purpose: one hit of any size takes the whole thing down.
    fn hurt_block_attached(&self, world: &World, source: &DamageSource) -> bool {
        if self.is_invulnerable_to_base(source) {
            return false;
        }

        // Vanilla parity: the `mobGriefing` guard, which is what stops a
        // skeleton's stray arrow from stripping a wall of paintings.
        let caused_by = caused_by_entity(world, source);
        if !world.get_game_rule(&MOB_GRIEFING)
            && caused_by
                .as_ref()
                .is_some_and(|entity| entity.as_mob().is_some())
        {
            return false;
        }

        let cause = if source.is(&DamageTypeTag::IS_EXPLOSION) {
            "EXPLOSION"
        } else if caused_by.is_some() {
            "ENTITY"
        } else {
            "DEFAULT"
        };
        let mut event = caused_by.as_ref().map_or_else(
            || HangingBreakEvent::new(self.uuid(), cause),
            |entity| HangingBreakEvent::new_with_remover(self.uuid(), cause, entity.uuid()),
        );
        world.fire_event(&mut event);
        if event.is_cancelled() {
            return false;
        }

        if !self.is_removed() {
            self.kill(world);
            self.mark_hurt();
            self.drop_item(world, caused_by.as_ref());
        }
        true
    }
}

/// Resolves the entity a damage source blames, if it is still around.
///
/// Vanilla parity: `DamageSource.getEntity`, which Foton stores as an id rather
/// than a reference.
pub fn caused_by_entity(world: &World, source: &DamageSource) -> Option<SharedEntity> {
    world.get_entity_by_id(source.causing_entity_id?)
}

/// Returns whether the entity that caused this damage keeps the drop.
///
/// Vanilla parity: the `causedBy instanceof Player player && player.hasInfiniteMaterials()`
/// that both `ItemFrame.dropItem` and `Painting.dropItem` guard their drops
/// with. A creative player breaking one gets nothing back.
#[must_use]
pub fn drop_would_be_wasted(caused_by: Option<&SharedEntity>) -> bool {
    caused_by.is_some_and(|entity| {
        entity
            .as_player()
            .is_some_and(Player::has_infinite_materials)
    })
}
